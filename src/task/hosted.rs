//! On-board C5 communication check: init, version RPCs, Wi-Fi scans and heartbeat.

use embassy_futures::select::{Either, select};
use embassy_net_esp_hosted::{Control, FwVersion, Network, State, iface::sdio::SdioInterface};
use embassy_time::{Duration, Timer, with_timeout};
use esp_hal::{Async, gpio::Output, sdmmc::Slot};
use heapless::Vec;
use static_cell::StaticCell;

use crate::driver::hosted_bus::TraceBus;
use crate::driver::hosted_v3::HostedV3;

static STATE: StaticCell<State> = StaticCell::new();

#[embassy_executor::task]
pub async fn verify(bus: Slot<'static, 1, Async>, reset: Output<'static>) {
    log::info!("hosted: starting C5 SDIO verification at 20 MHz");
    let iface = HostedV3::new(SdioInterface::new(TraceBus::new(bus), 20_000_000));
    let resources = embassy_net_esp_hosted::new(STATE.init(State::new()), iface, reset);
    let mut runner = resources.runner;
    let mut control = resources.control;
    // Keep the network device alive; this check only uses control RPCs.
    let _device = resources.net_device;
    match select(runner.run(), check(&mut control)).await {
        Either::First(result) => log::error!("hosted: FAIL runner stopped: {:?}", result),
        Either::Second(()) => log::error!("hosted: FAIL verification stopped"),
    }
}

async fn check(control: &mut Control<'_>) {
    match with_timeout(Duration::from_secs(45), control.init()).await {
        Ok(Ok(())) => log::info!("hosted: init OK, MAC {:?}", control.hardware_address()),
        result => {
            log::error!("hosted: FAIL init: {:?}", result);
            return;
        }
    }
    for round in 1..=3 {
        match with_timeout(Duration::from_secs(5), control.get_fw_version()).await {
            Ok(Ok(FwVersion::Mcu {
                major: 3,
                minor: 0,
                patch: 10,
            })) => {
                log::info!("hosted: version round {} OK: 3.0.10", round);
            }
            result => {
                log::error!("hosted: FAIL version round {}: {:?}", round, result);
                return;
            }
        }
        let mut networks: Vec<Network, 16> = Vec::new();
        match with_timeout(Duration::from_secs(30), control.scan(&mut networks)).await {
            Ok(Ok(())) => log::info!(
                "hosted: scan round {} OK, {} APs (capacity 16)",
                round,
                networks.len()
            ),
            result => {
                log::error!("hosted: FAIL scan round {}: {:?}", round, result);
                return;
            }
        }
        Timer::after_secs(2).await;
    }
    // The runner fails after 20 seconds without a heartbeat. Observe two windows.
    Timer::after_secs(45).await;
    log::info!("hosted: PASS init + 3 version RPCs + 3 scans + 45s heartbeat");
    loop {
        Timer::after_secs(60).await;
    }
}
