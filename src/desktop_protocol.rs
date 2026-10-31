//! Hardware-independent validation for the desktop's time service.
pub fn ntp_utc(reply: &[u8], nonce: [u8; 8]) -> Option<u64> {
    if reply.len() < 48
        || reply[0] & 7 != 4
        || !matches!((reply[0] >> 3) & 7, 3 | 4)
        || reply[0] >> 6 == 3
        || !(1..=15).contains(&reply[1])
        || reply[24..32] != nonce
    {
        return None;
    }
    let seconds = u32::from_be_bytes(reply[40..44].try_into().ok()?) as u64;
    // This firmware targets the current NTP era (through February 2036).
    if seconds <= 3_900_000_000 {
        return None;
    }
    Some(seconds - 2_208_988_800)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_ntp_source_timestamp_and_state() {
        let nonce = 12345u64.to_be_bytes();
        let mut reply = [0u8; 48];
        reply[0] = 0x24;
        reply[1] = 2;
        reply[24..32].copy_from_slice(&nonce);
        reply[40..44].copy_from_slice(&4_000_000_000u32.to_be_bytes());
        assert_eq!(ntp_utc(&reply, nonce), Some(1_791_011_200));
        assert_eq!(ntp_utc(&reply, [0; 8]), None);
        reply[0] |= 0xc0;
        assert_eq!(ntp_utc(&reply, nonce), None);
        reply[0] = 0x23;
        assert_eq!(ntp_utc(&reply, nonce), None);
        reply[0] = 0x24;
        reply[1] = 0;
        assert_eq!(ntp_utc(&reply, nonce), None);
        assert_eq!(ntp_utc(&reply[..47], nonce), None);
    }
}
