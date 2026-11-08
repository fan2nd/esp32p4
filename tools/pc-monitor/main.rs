#[path = "../../src/desktop/protocol.rs"]
#[allow(dead_code)]
mod protocol;
use hidapi::{HidApi, HidDevice};
use protocol::Metrics;
use std::{
    collections::BTreeMap,
    env,
    error::Error,
    io::{self, Write},
    thread,
    time::{Duration, Instant, SystemTime},
};
use sysinfo::{Networks, System};
mod platform;

const VID: u16 = 0xcafe;
const PID: u16 = 0x4014;
type Counters = BTreeMap<String, (u64, u64)>;

#[derive(Default)]
struct Args {
    serial: Option<String>,
    interface: Option<String>,
    dry_run: bool,
    demo: bool,
    list: bool,
    list_interfaces: bool,
    count: usize,
    interval: Option<f64>,
}
fn arguments() -> Result<Args, Box<dyn Error>> {
    let mut result = Args::default();
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--serial" => result.serial = Some(args.next().ok_or("--serial needs a value")?),
            "--interface" => {
                result.interface = Some(args.next().ok_or("--interface needs a value")?)
            }
            "--interval" => {
                result.interval = Some(args.next().ok_or("--interval needs seconds")?.parse()?)
            }
            "--count" => result.count = args.next().ok_or("--count needs a value")?.parse()?,
            "--dry-run" => result.dry_run = true,
            "--demo" => result.demo = true,
            "--list-devices" => result.list = true,
            "--list-interfaces" => result.list_interfaces = true,
            "--help" | "-h" => {
                println!(
                    "p4-pc-monitor [--serial SERIAL] [--interface NAME]\n\
                    [--interval 0.25] [--dry-run] [--demo] [--count N]\n\
                    [--list-devices] [--list-interfaces]\n\
                    P4 USB HID telemetry; Ctrl-C to stop. No virtual serial port required."
                );
                std::process::exit(0);
            }
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
    }
    let interval = result.interval.unwrap_or(0.25);
    if !interval.is_finite() || !(0.25..=2.0).contains(&interval) {
        return Err("--interval must be between 0.25 and 2 seconds".into());
    }
    Ok(result)
}

fn rates(previous: &Counters, current: &Counters, elapsed: f64) -> (u32, u32) {
    if elapsed <= 0.0 {
        return (0, 0);
    }
    let (mut rx, mut tx) = (0u64, 0u64);
    for (name, &(r, t)) in current {
        if let Some(&(old_r, old_t)) = previous.get(name) {
            rx = rx.saturating_add(r.saturating_sub(old_r));
            tx = tx.saturating_add(t.saturating_sub(old_t));
        }
    }
    (
        (rx as f64 / elapsed / 1024.0).round() as u32,
        (tx as f64 / elapsed / 1024.0).round() as u32,
    )
}

struct Sampler {
    system: System,
    networks: Networks,
    previous: Counters,
    at: Instant,
    interface: Option<String>,
    boot_time: u64,
}
impl Sampler {
    fn new(interface: Option<String>) -> Result<Self, Box<dyn Error>> {
        let boot_time = System::boot_time();
        if boot_time == 0 {
            return Err(
                "Cannot read system boot time; run outside a restricted sandbox/container".into(),
            );
        }
        let mut this = Self {
            system: System::new(),
            networks: Networks::new_with_refreshed_list(),
            previous: BTreeMap::new(),
            at: Instant::now(),
            interface,
            boot_time,
        };
        this.system.refresh_cpu_usage();
        if let Some(name) = &this.interface {
            if !this.networks.iter().any(|(n, _)| n == name) {
                return Err(format!("network interface not found: {name}").into());
            }
        }
        this.previous = this.counters();
        this.at = Instant::now();
        Ok(this)
    }
    fn counters(&self) -> Counters {
        self.networks
            .iter()
            .filter(|(name, data)| {
                platform::selected_interface(name, data, self.interface.as_deref())
            })
            .map(|(n, d)| (n.clone(), (d.total_received(), d.total_transmitted())))
            .collect()
    }
    fn sample(&mut self, sequence: u32) -> Metrics {
        self.system.refresh_cpu_usage();
        self.system.refresh_memory();
        self.networks.refresh(true);
        let now = Instant::now();
        let current = self.counters();
        let (rx_kib, tx_kib) = rates(&self.previous, &current, (now - self.at).as_secs_f64());
        self.previous = current;
        self.at = now;
        Metrics {
            sequence,
            cpu: (self.system.global_cpu_usage() * 10.0)
                .round()
                .clamp(0.0, 1000.0) as u32,
            memory: ((self.system.used_memory() as f64 / self.system.total_memory().max(1) as f64)
                * 1000.0)
                .round()
                .clamp(0.0, 1000.0) as u32,
            rx_kib,
            tx_kib,
            uptime_s: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                .saturating_sub(self.boot_time)
                .min(u32::MAX as u64) as u32,
        }
    }
}

fn encode(m: Metrics) -> String {
    format!(
        "PCMON1,{},{},{},{},{},{}\n",
        m.sequence, m.cpu, m.memory, m.rx_kib, m.tx_kib, m.uptime_s
    )
}
// Feature requests use EP0, avoiding VIA interrupt-IN response interference.
fn send(device: &HidDevice, bytes: &[u8], read_reply: bool) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut response = Vec::new();
    for chunk in bytes.chunks(30) {
        let mut packet = [0u8; 33]; // Windows HID writes include the zero report ID.
        packet[1] = 0x80;
        packet[2] = chunk.len() as u8;
        packet[3..3 + chunk.len()].copy_from_slice(chunk);
        device.send_feature_report(&packet)?;
    }
    // SET_REPORT already acknowledges delivery; only the handshake needs a reply.
    if read_reply {
        let mut reply = [0u8; 33];
        let n = device.get_feature_report(&mut reply)?;
        if n != 33 || reply[1] != 0x80 || reply[2] > 30 {
            return Err("invalid PCMON1 feature response".into());
        }
        response.extend_from_slice(&reply[3..3 + usize::from(reply[2])]);
    }
    Ok(response)
}
fn connect(serial: Option<&str>) -> Result<Option<HidDevice>, Box<dyn Error>> {
    let api = HidApi::new()?;
    let mut devices = api.device_list().filter(|d| {
        d.vendor_id() == VID
            && d.product_id() == PID
            && d.usage_page() == 0xff60
            && d.usage() == 0x61
            && serial.is_none_or(|s| d.serial_number() == Some(s))
    });
    let Some(info) = devices.next() else {
        return Ok(None);
    };
    if devices.next().is_some() {
        return Err("multiple P4 devices; select --serial".into());
    }
    let device = info.open_device(&api)?;
    if send(&device, b"\nHELLO\n", true)? != b"PCMON1 READY\n" {
        return Err("unexpected PCMON1 handshake".into());
    }
    eprintln!("Connected: {}", info.serial_number().unwrap_or("P4"));
    Ok(Some(device))
}
fn main() -> Result<(), Box<dyn Error>> {
    let args = arguments()?;
    if args.list {
        let api = HidApi::new()?;
        for d in api
            .device_list()
            .filter(|d| d.vendor_id() == VID && d.product_id() == PID && d.usage_page() == 0xff60)
        {
            println!(
                "{} {}",
                d.serial_number().unwrap_or(""),
                d.product_string().unwrap_or("")
            );
        }
        return Ok(());
    }
    if args.list_interfaces {
        for (name, data) in Networks::new_with_refreshed_list().iter() {
            println!(
                "{} {name}  {:?}",
                if platform::selected_interface(name, data, None) {
                    "*"
                } else {
                    " "
                },
                data.ip_networks()
            );
        }
        println!("* included by default; --interface NAME overrides selection");
        return Ok(());
    }
    let interval = Duration::from_secs_f64(args.interval.unwrap_or(0.25));
    let mut sampler = if args.demo {
        None
    } else {
        Some(Sampler::new(args.interface)?)
    };
    let mut port: Option<HidDevice> = None;
    let mut sent = 0usize;
    let mut waiting = false;
    thread::sleep(interval); // prime CPU/network deltas
    while args.count == 0 || sent < args.count {
        if !args.dry_run && port.is_none() {
            match connect(args.serial.as_deref()) {
                Ok(Some(device)) => {
                    port = Some(device);
                    waiting = false;
                }
                Ok(None) => {
                    if !waiting {
                        eprintln!("Waiting for P4 USB HID...");
                        waiting = true;
                    }
                    thread::sleep(Duration::from_secs(2));
                    continue;
                }
                Err(e) => {
                    eprintln!("USB: {e}; retrying in 2s");
                    thread::sleep(Duration::from_secs(2));
                    continue;
                }
            }
        }
        let m = match sampler.as_mut() {
            Some(s) => s.sample(sent as u32),
            None => Metrics {
                sequence: sent as u32,
                cpu: (500.0 + 400.0 * (sent as f64 / 8.0).sin()) as u32,
                memory: 632,
                rx_kib: 1280,
                tx_kib: 96,
                uptime_s: (sent as f64 * interval.as_secs_f64()) as u32,
            },
        };
        let packet = encode(m);
        if args.dry_run {
            print!("{packet}");
            io::stdout().flush()?;
        } else if let Err(e) = send(port.as_ref().unwrap(), packet.as_bytes(), false) {
            eprintln!("Disconnected: {e}; reconnecting");
            port = None;
            thread::sleep(Duration::from_secs(2));
            continue;
        }
        sent += 1;
        thread::sleep(interval);
    }
    Ok(())
}
