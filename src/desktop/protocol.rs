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
