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
#[derive(Clone, Copy)]
enum HciFailure {
    Rejected,
    Unknown,
}

/// Only this task owns command/response sequencing. A transport failure or
/// timeout is terminal until reboot: a late same-opcode completion must never
/// be mistaken for a later request's acknowledgement.
async fn hci_command(
    driver: &embassy_net_esp_hosted::bluetooth::BtDriver<'_>,
    buffer: &mut [u8],
    opcode: u16,
    parameters: &[u8],
) -> Result<(), HciFailure> {
    match with_timeout(Duration::from_secs(3), async {
        driver
            .write(&HciCommand { opcode, parameters })
            .await
            .map_err(|_| HciFailure::Unknown)?;
        loop {
            let event: HciEvent = driver.read(buffer).await.map_err(|_| HciFailure::Unknown)?;
            if event.code == 0x0e
                && event.len >= 4
                && u16::from_le_bytes([event.data[1], event.data[2]]) == opcode
            {
                return if event.data[3] == 0 {
                    Ok(())
                } else {
                    Err(HciFailure::Rejected)
                };
            }
            // An unsupported command can fail via Command Status instead of
            // Command Complete. Successful status is not completion.
            if event.code == 0x0f
                && event.len >= 4
                && u16::from_le_bytes([event.data[2], event.data[3]]) == opcode
                && event.data[0] != 0
            {
                return Err(HciFailure::Rejected);
            }
        }
    })
    .await
    {
        Ok(result) => result,
        Err(_) => Err(HciFailure::Unknown),
    }
}

async fn apply_ble_name(
    driver: &embassy_net_esp_hosted::bluetooth::BtDriver<'_>,
    buffer: &mut [u8],
    name: &str,
) -> Result<(), HciFailure> {
    // Bluetooth Core Vol 4 Part E 7.8.5, 7.8.7, 7.8.9. Disable before changing
    // parameters; 0x03 is ADV_NONCONN_IND, public address, all three channels.
    hci_command(driver, buffer, 0x200a, &[0]).await?;
    desktop::with(|s| {
        s.view.ble_advertising = false;
        s.view.revision = s.view.revision.wrapping_add(1);
    });
    hci_command(
        driver,
        buffer,
        0x2006,
        &[
            0x20, 0x03, 0x40, 0x03, // 500-520 ms intervals (0.625 ms units)
            0x03, 0x00, 0x00, // non-connectable, own/peer public address types
            0, 0, 0, 0, 0, 0, // ignored peer address for undirected advertising
            0x07, 0x00, // all advertising channels, no filter
        ],
    )
    .await?;
    // Flags: LE General Discoverable + BR/EDR Not Supported. Complete Local
    // Name is bounded to 24 UTF-8 bytes by the command's heapless String.
    let mut data = [0u8; 32];
    data[0] = (5 + name.len()) as u8;
    data[1..4].copy_from_slice(&[2, 0x01, 0x06]);
    data[4] = (1 + name.len()) as u8;
    data[5] = 0x09;
    data[6..6 + name.len()].copy_from_slice(name.as_bytes());
    hci_command(driver, buffer, 0x2008, &data).await?;
    hci_command(driver, buffer, 0x200a, &[1]).await
}

async fn bluetooth(driver: embassy_net_esp_hosted::bluetooth::BtDriver<'_>) {
    let mut buffer = [0; 1025];
    let initialized = async {
        hci_command(&driver, &mut buffer, 0x0c03, &[]).await?;
        hci_command(&driver, &mut buffer, 0x1009, &[]).await
    }
    .await;
    let mut healthy = initialized.is_ok();
    desktop::with(|s| {
        s.view.bluetooth = if healthy { "READY" } else { "ERROR" };
        s.view.ble_name_status = if healthy {
            "Not advertising"
        } else {
            "Controller unavailable"
        };
        s.view.revision = s.view.revision.wrapping_add(1);
    });
    // Restore only previously enabled name advertising; no pairing/HID service.
    desktop::with(|s| {
        if s.settings.ble_advertising && s.ble_name_command.is_none() {
            s.ble_name_command = Some(s.settings.ble_name.clone());
        }
    });
    loop {
        if let Some(enabled) = desktop::with(|s| s.ble_enable_command.take()) {
            if enabled && healthy {
                desktop::with(|s| s.ble_name_command = Some(s.settings.ble_name.clone()));
            } else {
                let result = if healthy {
                    hci_command(&driver, &mut buffer, 0x200a, &[0]).await
                } else {
                    Err(HciFailure::Unknown)
                };
                log::info!("desktop: BLE disable ok={}", result.is_ok());
                desktop::with(|s| {
                    if result.is_ok() {
                        s.view.ble_advertising = false;
                        s.view.ble_name_status = "Bluetooth advertising OFF";
                        if !s.reset_requested && s.settings.ble_advertising {
                            s.settings.ble_advertising = false;
                            s.settings.revision = s.settings.revision.wrapping_add(1);
                        }
                    } else {
                        s.view.ble_name_status = "Disable failed; retry/reboot";
                    }
                    s.view.revision = s.view.revision.wrapping_add(1);
                });
                if matches!(result, Err(HciFailure::Unknown)) {
                    healthy = false;
                }
            }
        }
        if let Some(name) = desktop::with(|s| s.ble_name_command.take()) {
            if !healthy {
                desktop::with(|s| {
                    s.view.ble_name_status = "Controller unavailable";
                    s.view.revision = s.view.revision.wrapping_add(1);
                });
            } else if name.is_empty() || name.chars().any(char::is_control) {
                desktop::with(|s| {
                    s.view.ble_name_status = "Invalid name";
                    s.view.revision = s.view.revision.wrapping_add(1);
                });
            } else {
                match apply_ble_name(&driver, &mut buffer, &name).await {
                    Ok(()) => desktop::with(|s| {
                        // Confirm state only after LE Set Advertising Enable completes.
                        if !s.reset_requested
                            && (s.settings.ble_name != name || !s.settings.ble_advertising)
                        {
                            s.settings.ble_name = name.clone();
                            s.settings.ble_advertising = true;
                            s.settings.revision = s.settings.revision.wrapping_add(1);
                        }
                        s.view.ble_name = name;
                        s.view.ble_advertising = true;
                        s.view.ble_name_status = "Advertising name";
                        s.view.revision = s.view.revision.wrapping_add(1);
                    }),
                    Err(HciFailure::Rejected) => desktop::with(|s| {
                        s.view.ble_name_status = "Controller rejected change";
                        s.view.revision = s.view.revision.wrapping_add(1);
                    }),
                    Err(HciFailure::Unknown) => {
                        healthy = false;
                        desktop::with(|s| {
                            s.view.bluetooth = "ERROR";
                            // Last confirmed flag remains; the explicit unknown
                            // status takes precedence until a reboot resets HCI.
                            s.view.ble_name_status = "State unknown; reboot";
                            s.view.revision = s.view.revision.wrapping_add(1);
                        });
                    }
                }
            }
        }
        // Receive is cancellation-safe: BtDriver waits for a whole packet and
        // consumes it synchronously. Bounded idle polling also services Apply.
        let received: Result<Result<HciEvent, _>, _> =
            with_timeout(Duration::from_millis(50), driver.read(&mut buffer)).await;
        let fault = match received {
            Ok(Ok(event)) => event.code == 0x10, // HCI Hardware Error
            Ok(Err(_)) => true,
            Err(_) => false, // no event while idle is expected
        };
        if fault && healthy {
            healthy = false;
            desktop::with(|s| {
                s.view.bluetooth = "ERROR";
                s.view.ble_name_status = "State unknown; reboot";
                s.view.revision = s.view.revision.wrapping_add(1);
            });
        }
        Timer::after_millis(5).await;
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
        s.view.ble_name_status = "Controller unavailable";
        s.view.ip = None;
        s.view.message = "C5 transport stopped";
        s.view.revision += 1;
    });
    log::error!("desktop: C5 service stopped");
}
async fn wifi(control: &mut embassy_net_esp_hosted::Control<'_>, stack: Stack<'static>) {
    let mut enabled = true; // Control::init starts Wi-Fi; restore the saved switch below.
    let mut reconnect = true;
    let mut next_retry = 0u64;
    let mut connected_ssid = heapless::String::<32>::new();
    desktop::with(|s| {
        if !s.settings.wifi_enabled {
            s.command = Some(Command::Enable(false));
        }
    });
    loop {
        let command = desktop::with(|s| {
            let command = s.command.take().or_else(|| {
                if enabled
                    && reconnect
                    && !stack.is_link_up()
                    && Instant::now().as_secs() >= next_retry
                    && !s.settings.ssid.is_empty()
                {
                    Some(Command::Connect {
                        ssid: s.settings.ssid.clone(),
                        password: s.settings.password.clone(),
                    })
                } else {
                    None
                }
            });
            s.wifi_busy = command.is_some();
            command
        });
        match command {
            Some(Command::SyncTime) => {
                // Manual request only: connection and reconnect never trigger SNTP.
                let ready = enabled && stack.is_link_up() && stack.config_v4().is_some();
                desktop::with(|s| {
                    s.view.message = if ready {
                        "Synchronizing time..."
                    } else {
                        "Connect Wi-Fi first"
                    };
                    s.view.revision += 1;
                });
                if ready {
                    let synced = sync_time(stack).await;
                    desktop::with(|s| {
                        if let Some(utc) = synced {
                            s.view.utc = Some((utc, Instant::now().as_secs()));
                            s.view.message = "Time synchronized";
                        } else {
                            s.view.message = "Time sync failed. Tap to retry";
                        }
                        s.view.revision += 1;
                    });
                }
            }
            Some(Command::Enable(value)) => {
                let ok = matches!(
                    with_timeout(Duration::from_secs(5), control.set_wifi_enabled(value)).await,
                    Ok(Ok(()))
                );
                log::info!("desktop: Wi-Fi enable={} ok={}", value, ok);
                if ok {
                    enabled = value;
                    reconnect = value;
                    next_retry = 0;
                    connected_ssid.clear();
                    stack.set_config_v4(embassy_net::ConfigV4::Dhcp(Default::default()));
                }
                desktop::with(|s| {
                    if ok {
                        if !s.reset_requested && s.settings.wifi_enabled != value {
                            s.settings.wifi_enabled = value;
                            s.settings.revision = s.settings.revision.wrapping_add(1);
                        }
                        s.view.ip = None;
                        s.view.wifi = if value { "OFFLINE" } else { "OFF" };
                        s.view.message = if value {
                            if s.settings.ssid.is_empty() {
                                "Wi-Fi ON. Scan to connect"
                            } else {
                                "Wi-Fi ON. Reconnecting..."
                            }
                        } else {
                            "Wi-Fi OFF. Passwords retained"
                        };
                    } else {
                        s.view.message = "Wi-Fi switch failed. Retry";
                    }
                    s.view.revision += 1;
                });
            }
            Some(Command::Forget(ssid)) => {
                if connected_ssid == ssid {
                    let _ = with_timeout(Duration::from_secs(3), control.disconnect()).await;
                    connected_ssid.clear();
                }
                desktop::with(|s| {
                    if !s.reset_requested {
                        s.settings.profiles.retain(|p| p.ssid != ssid);
                        if s.settings.ssid == ssid {
                            s.settings.ssid.clear();
                            unsafe {
                                s.settings.password.as_bytes_mut().fill(0);
                            }
                            s.settings.password.clear();
                            if let Some(p) = s.settings.profiles.last() {
                                s.settings.ssid = p.ssid.clone();
                                s.settings.password = p.password.clone();
                            }
                        }
                        s.settings.revision = s.settings.revision.wrapping_add(1);
                        s.view.message = "Network forgotten";
                        s.view.revision += 1;
                    }
                });
            }
            Some(Command::Scan) if enabled => {
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
                log::info!(
                    "desktop: scan complete ok={} records={} capacity=64",
                    ok,
                    networks.len()
                );
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
            Some(Command::Connect { ssid, mut password }) if enabled => {
                reconnect = true;
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
                // Save only accepted credentials; a failed attempt must not
                // overwrite the last usable network. Never log the password.
                if connected {
                    desktop::with(|s| {
                        if !s.reset_requested {
                            let changed = s.settings.ssid != ssid
                                || s.settings.password != password
                                || !s
                                    .settings
                                    .profiles
                                    .iter()
                                    .any(|p| p.ssid == ssid && p.password == password);
                            if changed {
                                // Most recently used network is preferred at startup. Keep up to eight.
                                s.settings.profiles.retain(|p| p.ssid != ssid);
                                if s.settings.profiles.is_full() {
                                    s.settings.profiles.remove(0);
                                }
                                s.settings
                                    .profiles
                                    .push(crate::desktop_settings::WifiProfile {
                                        ssid: ssid.clone(),
                                        password: password.clone(),
                                    })
                                    .ok();
                                s.settings.ssid = ssid.clone();
                                s.settings.password = password.clone();
                                s.settings.revision = s.settings.revision.wrapping_add(1);
                            }
                        }
                    });
                }
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
                if connected {
                    connected_ssid = ssid;
                }
                next_retry = Instant::now().as_secs() + 30;
                log::info!("desktop: DHCP configured={}", ip.is_some());
                desktop::with(|s| {
                    s.view.ip = ip;
                    s.view.wifi = if !enabled {
                        "OFF"
                    } else if ip.is_some() {
                        "CONNECTED"
                    } else {
                        "OFFLINE"
                    };
                    s.view.message = if ip.is_some() {
                        "Connected"
                    } else {
                        "Connection failed"
                    };
                    s.view.revision += 1;
                });
            }
            Some(Command::Disconnect) => {
                let _ = with_timeout(Duration::from_secs(3), control.disconnect()).await;
                // Disconnect is temporary; only Forget removes stored credentials.
                reconnect = false;
                connected_ssid.clear();
                desktop::with(|s| {
                    s.view.message = "Disconnected. Passwords retained";
                    s.view.revision += 1;
                });
            }
            Some(_) | None => {}
        }
        desktop::with(|s| s.wifi_busy = false);
        let ip = if enabled && stack.is_link_up() {
            stack.config_v4().map(|c| c.address.address().octets())
        } else {
            None
        };
        desktop::with(|s| {
            if s.view.ip != ip {
                s.view.ip = ip;
                s.view.wifi = if !enabled {
                    "OFF"
                } else if ip.is_some() {
                    "CONNECTED"
                } else {
                    "OFFLINE"
                };
                s.view.revision += 1;
            }
        });
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
