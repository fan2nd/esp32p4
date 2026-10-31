//! Removable SD block device. The host owns its filesystem; never format or mount here.
use aligned::{A4, Aligned};
use block_device_driver::BlockDevice;
use embassy_time::{Duration, Instant, with_timeout};
use esp_hal::{Async, gpio::Output, sdmmc::Slot};

type Card = sdio::BlockDevice<sdio::sd::Card, Slot<'static, 0, Async>, embassy_time::Delay, 512>;
pub struct SdCard {
    card: Card,
    _power: Output<'static>,
    blocks: u32,
    probe_after: Instant,
}
impl SdCard {
    pub fn new(slot: Slot<'static, 0, Async>, power: Output<'static>) -> Self {
        Self {
            card: Card::new_uninit_sd_card(slot, embassy_time::Delay),
            _power: power,
            blocks: 0,
            probe_after: Instant::now() + Duration::from_millis(10),
        }
    }
    /// No dedicated card-detect pin is wired. TUR/capacity requests probe at most
    /// once a second; errors drop the medium and permit hot-insert reacquisition.
    pub async fn refresh(&mut self, scratch: &mut Aligned<A4, [u8; 512]>) -> u32 {
        if Instant::now() < self.probe_after {
            return self.blocks;
        }
        if self.blocks != 0 {
            let _ = self.read(0, scratch).await;
        } else {
            // Bound retries so an empty slot cannot block the VIA LUN indefinitely.
            let result =
                with_timeout(Duration::from_secs(2), self.card.reacquire(20_000_000)).await;
            if matches!(result, Ok(Ok(()))) {
                let blocks = self.card.card().csd.block_count();
                if blocks != 0 && blocks <= u32::MAX as u64 {
                    self.blocks = blocks as u32;
                    log::info!(
                        "msc: SD ready blocks={} clock={}",
                        self.blocks,
                        self.card.freq()
                    );
                }
            }
        }
        self.probe_after =
            Instant::now() + Duration::from_secs(if self.blocks == 0 { 2 } else { 1 });
        self.blocks
    }
    pub async fn read(&mut self, lba: u32, block: &mut Aligned<A4, [u8; 512]>) -> bool {
        let ok = self.blocks != 0
            && lba < self.blocks
            && matches!(
                with_timeout(
                    Duration::from_secs(2),
                    self.card.read(lba, core::slice::from_mut(block))
                )
                .await,
                Ok(Ok(()))
            );
        if !ok {
            self.blocks = 0;
        }
        self.probe_after = Instant::now() + Duration::from_secs(1);
        ok
    }
    pub async fn write(&mut self, lba: u32, block: &Aligned<A4, [u8; 512]>) -> bool {
        // sdio waits for R1b/card-ready before returning: CSW success implies
        // programming completed; SYNCHRONIZE CACHE needs no deferred flush.
        let ok = self.blocks != 0
            && lba < self.blocks
            && matches!(
                with_timeout(
                    Duration::from_secs(2),
                    self.card.write(lba, core::slice::from_ref(block))
                )
                .await,
                Ok(Ok(()))
            );
        if !ok {
            self.blocks = 0;
        }
        self.probe_after = Instant::now() + Duration::from_secs(1);
        ok
    }
}
