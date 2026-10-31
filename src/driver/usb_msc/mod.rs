//! Read-only VIA definition drive: USB MSC BOT + SCSI + synthetic FAT12.
//! Implements USB-IF Bulk-Only Transport 1.0, including reset recovery.
pub mod bridge;
mod disk;
mod scsi;

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
                b[0] = 0;
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
    scsi: Scsi,
}
impl<'a, D: Driver<'a>> Msc<'a, D> {
    pub fn new(builder: &mut Builder<'a, D>, control: &'a mut Control<'a>, link: &'a Link) -> Self {
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
            scsi: Scsi::new(),
        }
    }
    pub async fn run(&mut self) -> ! {
        let mut media_epoch = self.link.media_epoch.get();
        loop {
            self.link.ready().await;
            if media_epoch != self.link.media_epoch.get() {
                media_epoch = self.link.media_epoch.get();
                self.scsi = Scsi::new();
            }
            let link = self.link;
            let epoch = link.epoch.get();
            // This cancels ANY pending CBW, data, halt wait or CSW on BOT/USB
            // reset. The bridge flushes only these bulk endpoints before reuse.
            select(link.wait(|| link.epoch.get() != epoch), self.session()).await;
            self.scsi.reset();
            yield_now().await;
        }
    }
    async fn session(&mut self) -> Result<(), EndpointError> {
        let mut packet = [0; 64];
        let mut reply = [0; 64];
        let mut sector = [0; 512];
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
            let command = if packet[13] != 0 {
                self.scsi.unsupported_lun()
            } else {
                self.scsi
                    .command(&packet[15..15 + packet[14] as usize], &mut reply)
            };
            let device = match command.data {
                Data::None => 0,
                Data::Reply(n) => n as u32,
                Data::Blocks { bytes, .. } => bytes,
            };
            let mut status = u8::from(command.failed);
            let mut processed = 0u32;
            // Read-only SCSI never has a successful Do phase. All writes fail
            // with DATA PROTECT and Dn; host OUT is halted, residue stays full.
            if host == 0 {
                if device != 0 {
                    status = 2;
                } // BOT cases 2/3.
            } else if host_in {
                if device == 0 {
                    self.link.halt(IN).await; // case 4, including command failure.
                    self.link.wait(|| self.link.halted.get() & IN == 0).await;
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
                                disk::read(block, &mut sector);
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
                        Data::None => {}
                    }
                    if host > device && device % 64 == 0 {
                        self.input.write(&[]).await?;
                    } // short termination, case 5.
                    if host < device {
                        status = 2;
                    } // case 7; do not silently truncate successfully.
                }
            } else {
                self.link.halt(OUT).await; // case 9 (Dn) or 10 (Di).
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
