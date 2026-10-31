//! C5 Wi-Fi, DHCP/SNTP and a minimal verified Bluetooth controller startup.
use crate::{
    desktop::{self, AccessPoint, Command},
    driver::{hosted_net::HostedNet, hosted_v3::HostedV3},
};
use bt_hci_transport::{PacketKind, PacketToController, PacketToHost, ReadHciError, Transport};
use embassy_futures::{join::join, select::select};
use embassy_net::{
    Stack, StackResources,
    dns::DnsQueryType,
    udp::{PacketMetadata, UdpSocket},
};
use embassy_time::{Duration, Instant, Timer, with_timeout};
use esp_hal::{Async, gpio::Output, sdmmc::Slot};
use static_cell::StaticCell;
static HOSTED: StaticCell<embassy_net_esp_hosted::State> = StaticCell::new();
static NET: StaticCell<StackResources<5>> = StaticCell::new();

struct HciCommand(u16);
impl PacketToController for HciCommand {
    const KIND: PacketKind = PacketKind::Cmd;
    fn size(&self) -> usize {
        3
    }
    fn write_hci<W: embedded_io::Write>(&self, mut w: W) -> Result<(), W::Error> {
        let [lo, hi] = self.0.to_le_bytes();
        w.write_all(&[lo, hi, 0])
    }
    async fn write_hci_async<W: embedded_io_async::Write>(&self, mut w: W) -> Result<(), W::Error> {
        let [lo, hi] = self.0.to_le_bytes();
        w.write_all(&[lo, hi, 0]).await
    }
}
struct HciEvent {
    code: u8,
    len: usize,
    data: [u8; 255],
}
impl<'d> PacketToHost<'d> for HciEvent {
    fn read_hci<R: embedded_io::Read>(
        kind: PacketKind,
        r: &mut R,
        _: &'d mut [u8],
    ) -> Result<Self, ReadHciError<R::Error>> {
        if kind != PacketKind::Event {
            return Err(ReadHciError::InvalidValue);
        }
        let mut h = [0; 2];
        r.read_exact(&mut h)?;
        let mut e = Self {
            code: h[0],
            len: h[1] as usize,
            data: [0; 255],
        };
        r.read_exact(&mut e.data[..e.len])?;
        Ok(e)
    }
    async fn read_hci_async<R: embedded_io_async::Read>(
        kind: PacketKind,
        r: &mut R,
        _: &'d mut [u8],
    ) -> Result<Self, ReadHciError<R::Error>> {
        if kind != PacketKind::Event {
            return Err(ReadHciError::InvalidValue);
        }
        let mut h = [0; 2];
        r.read_exact(&mut h).await?;
        let mut e = Self {
            code: h[0],
            len: h[1] as usize,
            data: [0; 255],
        };
        r.read_exact(&mut e.data[..e.len]).await?;
        Ok(e)
    }
}
async fn bluetooth(driver: embassy_net_esp_hosted::bluetooth::BtDriver<'_>) {
    let mut buffer = [0; 1025];
    let initialized = with_timeout(Duration::from_secs(8), async {
        // Reset then read the public address. READY requires successful controller
        // completions, not merely the presence of an HCI transport object.
        for opcode in [0x0c03, 0x1009] {
            driver.write(&HciCommand(opcode)).await.map_err(|_| ())?;
            loop {
                let event: HciEvent = driver.read(&mut buffer).await.map_err(|_| ())?;
                if event.code == 0x0e
                    && event.len >= 4
                    && u16::from_le_bytes([event.data[1], event.data[2]]) == opcode
                {
                    if event.data[3] != 0 {
                        return Err(());
                    }
                    break;
                }
            }
        }
        Ok(())
    })
    .await;
    let status = if matches!(initialized, Ok(Ok(()))) {
        "READY"
    } else {
        "ERROR"
    };
    log::info!("desktop: Bluetooth {}", status);
    desktop::with(|s| {
        s.view.bluetooth = status;
        s.view.revision += 1;
    });
    // Drain events so the shared C5 transport cannot back up. No advertising,
    // pairing or HID profile is enabled in this first Bluetooth implementation.
    loop {
        let _: Result<HciEvent, _> = driver.read(&mut buffer).await;
        Timer::after_millis(10).await;
    }
}

#[embassy_executor::task]
pub async fn run(bus: Slot<'static, 1, Async>, reset: Output<'static>) {
    let iface = HostedV3::new(embassy_net_esp_hosted::iface::sdio::SdioInterface::new(
        bus, 20_000_000,
    ));
    let resources = embassy_net_esp_hosted::new(
        HOSTED.init(embassy_net_esp_hosted::State::new()),
        iface,
        reset,
    );
    let mut runner = resources.runner;
    let mut control = resources.control;
    let application = async {
        if !matches!(
            with_timeout(Duration::from_secs(45), control.init()).await,
            Ok(Ok(()))
        ) {
            return;
        }
        desktop::with(|s| {
            s.view.wifi = "OFFLINE";
            s.view.revision += 1;
        });
        log::info!("desktop: Wi-Fi initialized, creating IP stack");
        let (stack, mut net) = embassy_net::new(
            HostedNet(resources.net_device),
            embassy_net::Config::dhcpv4(Default::default()),
            NET.init(StackResources::new()),
            0x43f192d827a3,
        );
        join(
            net.run(),
            join(wifi(&mut control, stack), bluetooth(resources.bluetooth)),
        )
        .await;
    };
    let _ = select(runner.run(), application).await;
    desktop::with(|s| {
        s.view.wifi = "ERROR";
        s.view.bluetooth = "ERROR";
        s.view.ip = None;
        s.view.message = "C5 transport stopped";
        s.view.revision += 1;
    });
    log::error!("desktop: C5 service stopped");
}
async fn wifi(control: &mut embassy_net_esp_hosted::Control<'_>, stack: Stack<'static>) {
    let mut next_sync = 0u64;
    loop {
        let command = desktop::with(|s| s.command.take());
        match command {
            Some(Command::Scan) => {
                desktop::with(|s| {
                    s.view.message = "Scanning...";
                    s.view.revision += 1;
                });
                // The backend reads only N records before clearing the C5 scan
                // list. Sixteen records can hide weaker or 5 GHz networks.
                let mut networks = heapless::Vec::<embassy_net_esp_hosted::Network, 64>::new();
                let ok = matches!(
                    with_timeout(Duration::from_secs(30), control.scan(&mut networks)).await,
                    Ok(Ok(()))
                );
                log::info!("desktop: scan complete ok={} records={} capacity=64", ok, networks.len());
                networks.sort_unstable_by(|a, b| b.rssi.cmp(&a.rssi));
                desktop::with(|s| {
                    if ok {
                        s.view.networks.clear();
                        for n in networks {
                            if !n.ssid.is_empty()
                                && !s.view.networks.iter().any(|a| a.ssid == n.ssid)
                            {
                                let _ = s.view.networks.push(AccessPoint {
                                    ssid: n.ssid,
                                    rssi: n.rssi,
                                    secure: n.security != embassy_net_esp_hosted::Security::Open,
                                });
                            }
                        }
                    }
                    s.view.message = if ok {
                        "Choose a network"
                    } else {
                        "Scan failed. Retry"
                    };
                    s.view.revision += 1;
                });
            }
            Some(Command::Connect { ssid, mut password }) => {
                log::info!("desktop: Wi-Fi connect requested");
                desktop::with(|s| {
                    s.view.wifi = "CONNECTING";
                    s.view.ip = None;
                    s.view.message = "Connecting...";
                    s.view.revision += 1;
                });
                let _ = with_timeout(Duration::from_secs(3), control.disconnect()).await;
                log::info!("desktop: previous association released");
                stack.set_config_v4(embassy_net::ConfigV4::Dhcp(Default::default()));
                let connected = matches!(
                    with_timeout(Duration::from_secs(30), control.connect(&ssid, &password)).await,
                    Ok(Ok(()))
                );
                log::info!("desktop: association result={}", connected);
                // Password is never logged or written to flash.
                unsafe {
                    password.as_bytes_mut().fill(0);
                }
                password.clear();
                let configured = connected
                    && with_timeout(Duration::from_secs(20), stack.wait_config_up())
                        .await
                        .is_ok();
                let ip = if configured {
                    stack.config_v4().map(|c| c.address.address().octets())
                } else {
                    None
                };
                log::info!("desktop: DHCP configured={}", ip.is_some());
                desktop::with(|s| {
                    s.view.ip = ip;
                    s.view.wifi = if ip.is_some() { "CONNECTED" } else { "OFFLINE" };
                    s.view.message = if ip.is_some() {
                        "Connected. Syncing time..."
                    } else {
                        "Connection failed"
                    };
                    s.view.revision += 1;
                });
                next_sync = 0;
            }
            Some(Command::Disconnect) => {
                let _ = with_timeout(Duration::from_secs(3), control.disconnect()).await;
            }
            None => {}
        }
        let ip = if stack.is_link_up() {
            stack.config_v4().map(|c| c.address.address().octets())
        } else {
            None
        };
        desktop::with(|s| {
            if s.view.ip != ip {
                s.view.ip = ip;
                s.view.wifi = if ip.is_some() { "CONNECTED" } else { "OFFLINE" };
                s.view.revision += 1;
            }
        });
        let now = Instant::now().as_secs();
        if ip.is_some() && now >= next_sync {
            let synced = sync_time(stack).await;
            next_sync = Instant::now().as_secs() + if synced.is_some() { 3600 } else { 30 };
            desktop::with(|s| {
                if let Some(utc) = synced {
                    s.view.utc = Some((utc, Instant::now().as_secs()));
                    s.view.message = "Time synchronized";
                } else {
                    s.view.message = "Time sync failed. Retrying";
                }
                s.view.revision += 1;
            });
        }
        Timer::after_millis(50).await;
    }
}
async fn sync_time(stack: Stack<'static>) -> Option<u64> {
    let mut rx_meta = [PacketMetadata::EMPTY; 2];
    let mut tx_meta = [PacketMetadata::EMPTY; 2];
    let mut rx = [0; 256];
    let mut tx = [0; 256];
    let mut socket = UdpSocket::new(stack, &mut rx_meta, &mut rx, &mut tx_meta, &mut tx);
    socket.bind(0).ok()?;
    for server in ["time.cloudflare.com", "pool.ntp.org"] {
        let addresses = match with_timeout(
            Duration::from_secs(5),
            stack.dns_query(server, DnsQueryType::A),
        )
        .await
        {
            Ok(Ok(a)) => a,
            _ => continue,
        };
        let Some(address) = addresses.first() else {
            continue;
        };
        let mut request = [0u8; 48];
        request[0] = 0x23;
        let nonce = Instant::now().as_micros().to_be_bytes();
        request[40..48].copy_from_slice(&nonce);
        if socket.send_to(&request, (*address, 123)).await.is_err() {
            continue;
        }
        let mut reply = [0u8; 128];
        if let Ok(Ok((n, meta))) =
            with_timeout(Duration::from_secs(5), socket.recv_from(&mut reply)).await
        {
            if meta.endpoint.addr != *address || meta.endpoint.port != 123 {
                continue;
            }
            if let Some(utc) = crate::desktop_protocol::ntp_utc(&reply[..n], nonce) {
                log::info!("desktop: SNTP synchronized");
                return Some(utc);
            }
        }
    }
    None
}
