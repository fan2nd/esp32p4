//! Shared Wi-Fi scan and BLE advertising workload for standalone and coexistence tests.
use crate::driver::{hosted_net::HostedNet, hosted_v3::HostedV3};
use bt_hci_transport::{PacketKind, PacketToController, PacketToHost, ReadHciError, Transport};
use embassy_futures::{
    join::join,
    select::{Either, select},
};
use embassy_net::{Config as NetConfig, StackResources};
use embassy_net_esp_hosted::{Control, Network, State, bluetooth::BtDriver};
use embassy_time::{Duration, Timer, with_timeout};
use esp_hal::gpio::Output;
use static_cell::StaticCell;
static HOSTED: StaticCell<State> = StaticCell::new();
static NET: StaticCell<StackResources<3>> = StaticCell::new();
struct HciEvent {
    code: u8,
    len: usize,
    data: [u8; 255],
}

struct HciCommand<'a> {
    opcode: u16,
    parameters: &'a [u8],
}
impl PacketToController for HciCommand<'_> {
    const KIND: PacketKind = PacketKind::Cmd;
    fn size(&self) -> usize {
        3 + self.parameters.len()
    }
    fn write_hci<W: embedded_io::Write>(&self, mut w: W) -> Result<(), W::Error> {
        let [lo, hi] = self.opcode.to_le_bytes();
        w.write_all(&[lo, hi, self.parameters.len() as u8])?;
        w.write_all(self.parameters)
    }
    async fn write_hci_async<W: embedded_io_async::Write>(&self, mut w: W) -> Result<(), W::Error> {
        let [lo, hi] = self.opcode.to_le_bytes();
        w.write_all(&[lo, hi, self.parameters.len() as u8]).await?;
        w.write_all(self.parameters).await
    }
}

/// One command at a time: consume its matching completion before issuing another.
/// A timeout aborts BLE setup rather than claiming that advertising succeeded.
async fn command(bt: &BtDriver<'_>, opcode: u16, parameters: &[u8]) -> Result<HciEvent, ()> {
    let operation = async {
        bt.write(&HciCommand { opcode, parameters })
            .await
            .map_err(|_| ())?;
        let mut buffer = [0; 1025];
        loop {
            let event: HciEvent = bt.read(&mut buffer).await.map_err(|_| ())?;
            if event.code == 0x0e
                && event.len >= 4
                && u16::from_le_bytes([event.data[1], event.data[2]]) == opcode
            {
                if event.data[3] != 0 {
                    log::error!("BLE: opcode={:#06x} status={:#04x}", opcode, event.data[3]);
                    return Err(());
                }
                return Ok(event);
            }
            if event.code == 0x0f
                && event.len >= 4
                && event.data[0] != 0
                && u16::from_le_bytes([event.data[2], event.data[3]]) == opcode
            {
                log::error!(
                    "BLE: opcode={:#06x} rejected={:#04x}",
                    opcode,
                    event.data[0]
                );
                return Err(());
            }
        }
    };
    match with_timeout(Duration::from_secs(8), operation).await {
        Ok(result) => result,
        Err(_) => {
            log::error!("BLE: opcode={:#06x} timeout", opcode);
            Err(())
        }
    }
}

async fn bluetooth(bt: BtDriver<'_>) {
    let setup = async {
        command(&bt, 0x0c03, &[]).await?; // HCI Reset
        let address = command(&bt, 0x1009, &[]).await?; // Read BD_ADDR
        if address.len != 10 {
            return Err(());
        }
        log::info!(
            "BLE: address {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            address.data[9],
            address.data[8],
            address.data[7],
            address.data[6],
            address.data[5],
            address.data[4]
        );
        // Legacy non-connectable advertising, 250 ms, public address, channels 37/38/39.
        command(
            &bt,
            0x2006,
            &[0x90, 0x01, 0x90, 0x01, 3, 0, 0, 0, 0, 0, 0, 0, 0, 7, 0],
        )
        .await?;
        let name = b"P4-WiFi-BLE";
        let mut data = [0; 32];
        data[0] = (5 + name.len()) as u8;
        data[1..4].copy_from_slice(&[2, 1, 6]); // General discoverable, no BR/EDR.
        data[4] = (name.len() + 1) as u8;
        data[5] = 9; // Complete local name.
        data[6..6 + name.len()].copy_from_slice(name);
        command(&bt, 0x2008, &data).await?;
        command(&bt, 0x200a, &[1]).await?;
        log::info!("BLE: advertising P4-WiFi-BLE, 250 ms, non-connectable");
        Ok::<(), ()>(())
    }
    .await;
    if setup.is_err() {
        log::error!("BLE: setup failed");
    }
    let mut buffer = [0; 1025];
    loop {
        // Keep consuming events while Wi-Fi scans, including controller errors.
        match bt.read::<HciEvent>(&mut buffer).await {
            Ok(event) => log::info!("BLE: event={:#04x} len={}", event.code, event.len),
            Err(e) => log::warn!("BLE: event error {:?}", e),
        }
        Timer::after_millis(10).await;
    }
}

async fn wifi(control: &mut Control<'_>, stack: embassy_net::Stack<'static>) {
    let mut round = 0u32;
    loop {
        round += 1;
        let mut networks = heapless::Vec::<Network, 16>::new();
        match with_timeout(Duration::from_secs(30), control.scan(&mut networks)).await {
            Ok(Ok(())) => {
                networks.sort_unstable_by(|a, b| b.rssi.cmp(&a.rssi));
                log::info!(
                    "Wi-Fi: scan {} OK, {} APs (capacity 16)",
                    round,
                    networks.len()
                );
                for ap in &networks {
                    log::info!(
                        "Wi-Fi: SSID={} RSSI={} security={:?}",
                        ap.ssid,
                        ap.rssi,
                        ap.security
                    );
                }
            }
            result => log::error!("Wi-Fi: scan {} failed {:?}", round, result),
        }
        // Optional test credentials are supplied at build time, never printed.
        // Default firmware only scans and does not connect to an arbitrary AP.
        if let Some(ssid) = option_env!("P4_WIFI_SSID").filter(|s| !s.is_empty()) {
            if !stack.is_link_up() {
                log::info!("Wi-Fi: connecting to configured AP");
                match with_timeout(
                    Duration::from_secs(30),
                    control.connect(ssid, option_env!("P4_WIFI_PASSWORD").unwrap_or("")),
                )
                .await
                {
                    Ok(Ok(())) => {
                        if with_timeout(Duration::from_secs(20), stack.wait_config_up())
                            .await
                            .is_ok()
                        {
                            if let Some(config) = stack.config_v4() {
                                log::info!("Wi-Fi: DHCP {}", config.address);
                            }
                        } else {
                            log::warn!("Wi-Fi: DHCP timeout");
                        }
                    }
                    result => log::warn!("Wi-Fi: connect failed {:?}", result),
                }
            }
        }
        Timer::after_secs(15).await;
    }
}

#[embassy_executor::task]
pub async fn run(bus: esp_hal::sdmmc::Slot<'static, 1, esp_hal::Async>, reset: Output<'static>) {
    let iface = HostedV3::new(embassy_net_esp_hosted::iface::sdio::SdioInterface::new(
        bus, 20_000_000,
    ));
    let resources = embassy_net_esp_hosted::new(HOSTED.init(State::new()), iface, reset);
    let mut runner = resources.runner;
    let mut control = resources.control;
    let application = async {
        match with_timeout(Duration::from_secs(45), control.init()).await {
            Ok(Ok(())) => log::info!("Wi-Fi: init OK, MAC {:?}", control.hardware_address()),
            result => {
                log::error!("Wi-Fi: init failed {:?}", result);
                return;
            }
        }
        log::info!(
            "C5 firmware: {:?}",
            with_timeout(Duration::from_secs(5), control.get_fw_version()).await
        );
        let (stack, mut net) = embassy_net::new(
            HostedNet(resources.net_device),
            NetConfig::dhcpv4(Default::default()),
            NET.init(StackResources::new()),
            0x34a932bd80e1,
        );
        join(
            net.run(),
            join(wifi(&mut control, stack), bluetooth(resources.bluetooth)),
        )
        .await;
    };
    match select(runner.run(), application).await {
        Either::First(result) => log::error!("C5: transport stopped {:?}", result),
        Either::Second(()) => log::error!("C5: initialization failed"),
    }
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
