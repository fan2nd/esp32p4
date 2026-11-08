//! PCMON1 over HID feature reports on EP0; never inserts replies into VIA's IN queue.
use crate::desktop::{
    self,
    protocol::{Decoder, Message},
};
use embassy_usb::{
    class::hid::{ReportId, RequestHandler},
    control::OutResponse,
};
use esp_hal::time::Instant;

#[derive(Default)]
pub struct PcMonitor {
    decoder: Decoder,
    response: [u8; 32],
}
impl RequestHandler for PcMonitor {
    fn get_report(&mut self, id: ReportId, buf: &mut [u8]) -> Option<usize> {
        if id != ReportId::Feature(0) || buf.len() < self.response.len() {
            return None;
        }
        buf[..32].copy_from_slice(&self.response);
        Some(32)
    }
    fn set_report(&mut self, id: ReportId, data: &[u8]) -> OutResponse {
        if id != ReportId::Feature(0) || data.len() != 32 || data[0] != 0x80 || data[1] > 30 {
            self.decoder = Decoder::new();
            return OutResponse::Rejected;
        }
        self.response.fill(0);
        self.response[0] = 0x80;
        for &byte in &data[2..2 + usize::from(data[1])] {
            match self.decoder.feed(byte) {
                Some(Message::Hello) => {
                    let hello = b"PCMON1 READY\n";
                    self.response[1] = hello.len() as u8;
                    self.response[2..2 + hello.len()].copy_from_slice(hello);
                }
                Some(Message::Metrics(metrics)) => {
                    let now = Instant::now().duration_since_epoch().as_millis();
                    desktop::with(|s| s.pc_metrics = Some((metrics, now)));
                }
                None => {}
            }
        }
        OutResponse::Accepted
    }
}
