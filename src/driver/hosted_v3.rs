//! ESP-Hosted MCU v3 boot negotiation around Embassy's SDIO transport.
//!
//! Embassy handles RPC and networking. This adapter only acknowledges the
//! private INIT event, which v3 requires before enabling its RPC endpoint.
//! Wire contract: esp-hosted-mcu common/eh_tlv/include/eh_tlv_tags.h and
//! host/mcu/eh_host_mcu_transport/src/eh_host_mcu_transport_send_caps.c.

use aligned::{A4, Aligned};
use embassy_net_esp_hosted::iface::Interface;

const HEADER_LEN: usize = 12;
const PRIV_IF: u8 = 5;
const INIT_EVENT: u8 = 0x22;
const C5_CHIP_ID: u8 = 0x17;

/// Add the v3 private boot handshake to an Embassy SDIO interface.
pub struct HostedV3<I> {
    inner: I,
}

impl<I> HostedV3<I> {
    pub fn new(inner: I) -> Self {
        Self { inner }
    }
}

impl<I: Interface> Interface for HostedV3<I> {
    async fn init(&mut self, cold_boot: bool) {
        self.inner.init(cold_boot).await;
    }

    async fn wait_for_handshake(&mut self) {
        self.inner.wait_for_handshake().await;
    }

    async fn wait_for_ready(&mut self) {
        // Check the pending status even when DAT1 was already asserted before
        // the host armed its interrupt (notably for the first INIT packet).
        embassy_futures::select::select(
            self.inner.wait_for_ready(),
            embassy_time::Timer::after_millis(10),
        )
        .await;
    }

    async fn transfer(&mut self, buffer: &mut Aligned<A4, [u8]>, tx_len: usize) {
        self.inner.transfer(buffer, tx_len).await;
        if buffer.first().copied().unwrap_or(0) & 0x0f != PRIV_IF {
            return;
        }

        match validate_init(buffer) {
            Ok(()) => {
                let len = encode_ack(buffer);
                log::info!(
                    "hosted: C5 private INIT valid, acknowledging RPC v2 / SDIO packet + checksum"
                );
                // Preserve any packet received during the write: the Embassy
                // runner still needs to see the subsequent RPC ESPInit event.
                self.inner.transfer(buffer, len).await;
            }
            Err(reason) => {
                log::error!("hosted: rejecting private INIT: {}", reason);
                let header_len = HEADER_LEN.min(buffer.len());
                buffer[..header_len].fill(0);
            }
        }
    }
}

fn read_u16(bytes: &[u8]) -> usize {
    u16::from_le_bytes([bytes[0], bytes[1]]) as usize
}

fn checksum(frame: &[u8]) -> u16 {
    frame.iter().enumerate().fold(0u16, |sum, (i, &byte)| {
        sum.wrapping_add(if i == 6 || i == 7 { 0 } else { byte as u16 })
    })
}

fn validate_init(frame: &[u8]) -> Result<(), &'static str> {
    if frame.len() < HEADER_LEN + 2 {
        return Err("truncated header");
    }
    let len = read_u16(&frame[2..4]);
    if frame[0] != PRIV_IF || frame[1] != 0 || read_u16(&frame[4..6]) != HEADER_LEN {
        return Err("unexpected interface, flags or header offset");
    }
    if !matches!(frame[11], 0 | 0x33) || len < 2 || len > frame.len() - HEADER_LEN {
        return Err("invalid private packet type or payload length");
    }
    if checksum(&frame[..HEADER_LEN + len]) != read_u16(&frame[6..8]) as u16 {
        return Err("header/payload checksum mismatch");
    }
    let payload = &frame[HEADER_LEN..HEADER_LEN + len];
    if payload[0] != INIT_EVENT || payload[1] as usize != len - 2 {
        return Err("invalid INIT event or TLV length");
    }

    let mut tlvs = &payload[2..];
    let (mut caps, mut chip, mut rpc, mut sdio) = (None, None, None, None);
    while !tlvs.is_empty() {
        if tlvs.len() < 2 || tlvs[1] as usize > tlvs.len() - 2 {
            return Err("truncated capability TLV");
        }
        let (tag, len) = (tlvs[0], tlvs[1] as usize);
        let value = &tlvs[2..2 + len];
        match tag {
            0x11 if len == 1 && caps.is_none() => caps = Some(value[0]),
            0x12 if len == 1 && chip.is_none() => chip = Some(value[0]),
            0x1a if len == 1 && rpc.is_none() => rpc = Some(value[0]),
            0x1b if len == 5 && sdio.is_none() => sdio = Some(value),
            0x11 | 0x12 | 0x1a | 0x1b => return Err("invalid or duplicate capability TLV"),
            _ => {}
        }
        tlvs = &tlvs[2 + len..];
    }
    if caps.is_none_or(|cap| cap & 0x81 != 0x81) {
        return Err("SDIO WLAN and checksum must both be enabled");
    }
    if chip != Some(C5_CHIP_ID) || rpc != Some(2) {
        return Err("expected ESP32-C5 with RPC v2");
    }
    // Transport=SDIO, each direction=packet, 3 x 512-byte buffers. Embassy
    // uses this size both for DMA padding and for its TX credit accounting.
    if sdio != Some(&[0, 2, 3, 2, 3][..]) {
        return Err("expected packet mode and 1536-byte buffers in both directions");
    }
    Ok(())
}

fn encode_ack(frame: &mut [u8]) -> usize {
    // Same tags as the official MCU host. Raw throughput testing and Wi-Fi
    // flow-control thresholds are disabled for this scan-only verification.
    const PAYLOAD: [u8; 20] = [
        INIT_EVENT, 18, 0x44, 1, 0, 0x45, 1, C5_CHIP_ID, 0x46, 1, 0, 0x47, 1, 0, 0x48, 1, 0, 0x1a,
        1, 2,
    ];
    let len = HEADER_LEN + PAYLOAD.len();
    frame[..len].fill(0);
    frame[0] = PRIV_IF;
    frame[2..4].copy_from_slice(&(PAYLOAD.len() as u16).to_le_bytes());
    frame[4..6].copy_from_slice(&(HEADER_LEN as u16).to_le_bytes());
    frame[HEADER_LEN..len].copy_from_slice(&PAYLOAD);
    let sum = checksum(&frame[..len]);
    frame[6..8].copy_from_slice(&sum.to_le_bytes());
    len
}
