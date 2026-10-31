//! USB MSC BOT with VIA FAT12 (LUN 0) and removable SD card (LUN 1).
//! Implements USB-IF Bulk-Only Transport 1.0, including reset recovery.
pub mod bridge;
mod disk;
mod scsi;

use crate::driver::sd_card::SdCard;
use aligned::{A4, Aligned};
use bridge::{IN, Link, OUT};
use core::future::pending;
use embassy_futures::{select::select, yield_now};
use embassy_usb::{
    Builder, Handler,
    control::{InResponse, OutResponse, Recipient, Request, RequestType},
    driver::{Driver, Endpoint, EndpointError, EndpointIn, EndpointOut},
};
use scsi::{Data, Scsi};

pub struct Control<'a> {
    link: &'a Link,
    interface: u16,
}
impl<'a> Control<'a> {
    pub fn new(link: &'a Link) -> Self {
        Self { link, interface: 0 }
    }
}
impl Handler for Control<'_> {
    fn reset(&mut self) {
        self.link.configure(false);
    }
    fn configured(&mut self, on: bool) {
        self.link.configure(on);
    }
    fn enabled(&mut self, on: bool) {
        if !on {
            self.link.configure(false);
        }
    }
    fn control_out(&mut self, req: Request, _: &[u8]) -> Option<OutResponse> {
        if req.request_type != RequestType::Class
            || req.recipient != Recipient::Interface
            || req.index != self.interface
        {
            return None;
        }
        Some(
            if req.request == 0xff && req.value == 0 && req.length == 0 {
                self.link.class_reset();
                OutResponse::Accepted
            } else {
                OutResponse::Rejected
            },
        )
    }
    fn control_in<'a>(&'a mut self, req: Request, b: &'a mut [u8]) -> Option<InResponse<'a>> {
        if req.request_type != RequestType::Class
            || req.recipient != Recipient::Interface
            || req.index != self.interface
        {
            return None;
        }
        Some(
            if req.request == 0xfe && req.value == 0 && req.length == 1 {
                b[0] = 1; // Highest supported LUN, not the number of LUNs.
                InResponse::Accepted(&b[..1])
            } else {
                InResponse::Rejected
            },
        )
    }
}
pub struct Msc<'a, D: Driver<'a>> {
    input: D::EndpointIn,
    output: D::EndpointOut,
    link: &'a Link,
    scsi: [Scsi; 2],
    card: SdCard,
}
impl<'a, D: Driver<'a>> Msc<'a, D> {
    pub fn new(
        builder: &mut Builder<'a, D>,
        control: &'a mut Control<'a>,
        link: &'a Link,
        card: SdCard,
    ) -> Self {
        let mut function = builder.function(8, 6, 0x50);
        let mut interface = function.interface();
        control.interface = u8::from(interface.interface_number()) as u16;
        let mut alt = interface.alt_setting(8, 6, 0x50, None);
        let output = alt.endpoint_bulk_out(None, 64);
        let input = alt.endpoint_bulk_in(None, 64);
        link.endpoints(input.info().addr, output.info().addr);
        drop(function);
        builder.handler(control);
        Self {
            input,
            output,
            link,
            scsi: [Scsi::new(disk::BLOCKS, true), Scsi::new(0, false)],
            card,
        }
    }
    pub async fn run(&mut self) -> ! {
        let mut media_epoch = self.link.media_epoch.get();
        loop {
            self.link.ready().await;
            if media_epoch != self.link.media_epoch.get() {
                media_epoch = self.link.media_epoch.get();
                self.scsi = [Scsi::new(disk::BLOCKS, true), Scsi::new(0, false)];
            }
            let link = self.link;
            let epoch = link.epoch.get();
            // This cancels ANY pending CBW, data, halt wait or CSW on BOT/USB
            // reset. The bridge flushes only these bulk endpoints before reuse.
            select(link.wait(|| link.epoch.get() != epoch), self.session()).await;
            for lun in &mut self.scsi {
                lun.reset();
            }
            yield_now().await;
        }
    }
    async fn session(&mut self) -> Result<(), EndpointError> {
        let mut packet = [0; 64];
        let mut reply = [0; 64];
        let mut sector: Aligned<A4, [u8; 512]> = Aligned([0; 512]);
        loop {
            self.link.ready().await;
            self.output.wait_enabled().await;
            self.input.wait_enabled().await;
            let n = self.output.read(&mut packet).await?;
            if n != 31
                || packet[..4] != *b"USBC"
                || packet[12] & 0x7f != 0
                || packet[13] & 0xf0 != 0
                || !(1..=16).contains(&packet[14])
            {
                self.link.locked.set(true);
                self.link.halt(IN | OUT).await;
                pending::<()>().await; // Invalid CBW: no CSW; reset + both clears required.
            }
            let tag: [u8; 4] = packet[4..8].try_into().unwrap();
            let host = u32::from_le_bytes(packet[8..12].try_into().unwrap());
            let host_in = packet[12] & 0x80 != 0;
            let lun = packet[13] as usize;
            let cdb = &packet[15..15 + packet[14] as usize];
            if lun == 1 && !matches!(cdb[0], 0x12 | 0x03 | 0xa0) {
                let blocks = self.card.refresh(&mut sector).await;
                self.scsi[1].set_media(blocks);
            }
            let command = if lun >= self.scsi.len() {
                self.scsi[0].unsupported_lun()
            } else {
                self.scsi[lun].command(cdb, &mut reply)
            };
            let device = match command.data {
                Data::None => 0,
                Data::Reply(n) => n as u32,
                Data::Blocks { bytes, .. } | Data::WriteBlocks { bytes, .. } => bytes,
            };
            let writing = matches!(command.data, Data::WriteBlocks { .. });
            let mut status = u8::from(command.failed);
            let mut processed = 0u32;
            if host == 0 {
                if device != 0 {
                    status = 2;
                } // BOT cases 2/3.
            } else if host_in {
                if device == 0 || writing {
                    self.link.halt(IN).await;
                    self.link.wait(|| self.link.halted.get() & IN == 0).await;
                    if writing && device != 0 {
                        status = 2;
                    } // Hi/Do, case 8.
                } else {
                    let transfer = host.min(device);
                    match command.data {
                        Data::Reply(_) => {
                            for chunk in reply[..transfer as usize].chunks(64) {
                                self.input.write(chunk).await?;
                                processed += chunk.len() as u32;
                                yield_now().await;
                            }
                        }
                        Data::Blocks { lba, .. } => {
                            let mut remaining = transfer;
                            let mut block = lba;
                            while remaining != 0 {
                                if lun == 0 {
                                    disk::read(block, &mut sector);
                                } else if !self.card.read(block, &mut sector).await {
                                    self.scsi[lun].io_failed(false);
                                    status = 1;
                                    // Stop the data phase at the failed sector; never fabricate data.
                                    self.link.halt(IN).await;
                                    self.link.wait(|| self.link.halted.get() & IN == 0).await;
                                    break;
                                }
                                let count = remaining.min(512) as usize;
                                for chunk in sector[..count].chunks(64) {
                                    self.input.write(chunk).await?;
                                    processed += chunk.len() as u32;
                                    yield_now().await;
                                }
                                remaining -= count as u32;
                                block += 1;
                            }
                        }
                        _ => {}
                    }
                    if status == 0 && host > device && device % 64 == 0 {
                        self.input.write(&[]).await?; // Short termination, case 5.
                    }
                    if host < device {
                        status = 2;
                    } // case 7.
                }
            } else if let Data::WriteBlocks { lba, .. } = command.data {
                let mut remaining = host.min(device);
                let mut block = lba;
                // A host-short write is a phase error, never a partial-sector write.
                let mut failed = host < device;
                if failed {
                    status = 2;
                }
                while remaining != 0 {
                    let count = remaining.min(512) as usize;
                    let mut filled = 0;
                    while filled < count {
                        let n = self.output.read(&mut packet).await?;
                        let expected = (count - filled).min(64);
                        if n != expected {
                            self.link.locked.set(true);
                            self.link.halt(OUT).await;
                            let mut csw = [0; 13];
                            csw[..4].copy_from_slice(b"USBS");
                            csw[4..8].copy_from_slice(&tag);
                            csw[8..12].copy_from_slice(&(host - processed).to_le_bytes());
                            csw[12] = 2;
                            self.input.write(&csw).await?;
                            pending::<()>().await; // Recover only via BOT reset.
                        }
                        sector[filled..filled + n].copy_from_slice(&packet[..n]);
                        filled += n;
                    }
                    if !failed {
                        if self.card.write(block, &sector).await {
                            processed += 512;
                        } else {
                            self.scsi[lun].io_failed(true);
                            status = 1;
                            failed = true;
                        }
                    }
                    // After an I/O error drain the rest of host data, keeping the
                    // residue for sectors not committed to the card.
                    remaining -= count as u32;
                    block += 1;
                    yield_now().await;
                }
                if host > device {
                    self.link.halt(OUT).await;
                } // case 11.
            } else {
                self.link.halt(OUT).await; // cases 9/10: Dn or opposite direction.
                if device != 0 {
                    status = 2;
                }
            }
            let mut csw = [0; 13];
            csw[..4].copy_from_slice(b"USBS");
            csw[4..8].copy_from_slice(&tag);
            csw[8..12].copy_from_slice(&(host - processed).to_le_bytes());
            csw[12] = status;
            self.input.write(&csw).await?;
            if status == 2 {
                self.link.locked.set(true);
                pending::<()>().await;
            }
            yield_now().await;
        }
    }
}
