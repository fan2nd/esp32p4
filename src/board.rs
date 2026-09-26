use core::net::Ipv4Addr;

// Locally administered MAC for lab bring-up. Replace it with a unique address
// before putting more than one board on the same LAN.
pub const MAC_ADDR: [u8; 6] = [0x02, 0x04, 0x13, 0x00, 0x00, 0x01];
pub const NET_SEED: u64 = 0x2026_0829_0103_5353;
pub const PHY_RESET_WAIT_CYCLES: u32 = 5_000_000;

pub const HOST_FQDN: &str = "waveshare-p4-eth.local.";
pub const SERVICE_INSTANCE: &str = "waveshare-p4-eth._http._tcp.local.";
pub const SERVICE_TYPE: &str = "_http._tcp.local.";
pub const SERVICE_TXT: &[u8] = b"\x0fboard=waveshare\x07rev=3.2";
pub const MDNS_PORT: u16 = 5353;
pub const HTTP_PORT: u16 = 80;

pub const LOCAL_IPV4: Ipv4Addr = Ipv4Addr::new(192, 168, 1, 42);
pub const GATEWAY_IPV4: Option<Ipv4Addr> = Some(Ipv4Addr::new(192, 168, 1, 1));
pub const SUBNET_PREFIX: u8 = 24;

pub const MDNS_MULTICAST_IPV4: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
