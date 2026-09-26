use core::net::Ipv4Addr;

use embassy_net::{
    IpEndpoint, Ipv4Address, Stack,
    udp::{PacketMetadata, UdpMetadata, UdpSocket},
};
use embassy_time::{Duration, Timer};
use static_cell::StaticCell;

use crate::board;

const DNS_TYPE_A: u16 = 1;
const DNS_TYPE_PTR: u16 = 12;
const DNS_TYPE_TXT: u16 = 16;
const DNS_TYPE_SRV: u16 = 33;
const DNS_TYPE_ANY: u16 = 255;
const DNS_CLASS_IN: u16 = 1;
const DNS_CLASS_FLUSH: u16 = 0x8001;
const DNS_FLAGS_RESPONSE: u16 = 0x8400;
const META_SERVICE_TYPE: &str = "_services._dns-sd._udp.local.";

static RX_META: StaticCell<[PacketMetadata; 8]> = StaticCell::new();
static TX_META: StaticCell<[PacketMetadata; 8]> = StaticCell::new();
static RX_BUF: StaticCell<[u8; 768]> = StaticCell::new();
static TX_BUF: StaticCell<[u8; 768]> = StaticCell::new();

#[derive(Clone, Copy)]
pub struct MdnsConfig {
    pub host_fqdn: &'static str,
    pub service_instance: &'static str,
    pub service_type: &'static str,
    pub service_txt: &'static [u8],
    pub port: u16,
    pub ip: Ipv4Addr,
}

#[embassy_executor::task]
pub async fn serve(stack: Stack<'static>, cfg: MdnsConfig) -> ! {
    stack.wait_config_up().await;
    while !stack.is_link_up() {
        Timer::after(Duration::from_millis(100)).await;
    }

    stack
        .join_multicast_group(to_embassy_ipv4(board::MDNS_MULTICAST_IPV4))
        .expect("failed to join mDNS multicast group");

    let rx_meta = RX_META.init([PacketMetadata::EMPTY; 8]);
    let tx_meta = TX_META.init([PacketMetadata::EMPTY; 8]);
    let rx_buf = RX_BUF.init([0; 768]);
    let tx_buf = TX_BUF.init([0; 768]);

    let mut socket = UdpSocket::new(stack, rx_meta, rx_buf, tx_meta, tx_buf);
    socket
        .bind((Ipv4Address::new(0, 0, 0, 0), board::MDNS_PORT))
        .expect("failed to bind mDNS socket");
    socket.set_hop_limit(Some(255));

    let mut announce = [0u8; 768];
    if let Some(len) = build_response(&mut announce, None, &cfg) {
        let _ = socket.send_to(&announce[..len], mdns_endpoint()).await;
    }

    let mut recv = [0u8; 768];
    loop {
        let (len, peer) = match socket.recv_from(&mut recv).await {
            Ok(value) => value,
            Err(_) => continue,
        };

        if let Some(resp_len) = build_response(&mut announce, Some(&recv[..len]), &cfg) {
            let _ = socket
                .send_to(&announce[..resp_len], response_endpoint(peer))
                .await;
        }
    }
}

fn build_response(out: &mut [u8], query: Option<&[u8]>, cfg: &MdnsConfig) -> Option<usize> {
    let mut writer = DnsWriter::new(out);
    let mut question_buf = [0u8; 128];

    match query {
        Some(packet) => {
            let (transaction_id, question, answers) = parse_query(packet, cfg, &mut question_buf)?;
            writer.header(transaction_id, DNS_FLAGS_RESPONSE, 1, answers.count(), 0, 0)?;
            writer.question(question.name, question.qtype, question.qclass)?;
            answers.write(&mut writer, cfg)?;
        }
        None => {
            let answers = AnswerSet::ServiceType;
            writer.header(0, DNS_FLAGS_RESPONSE, 0, answers.count(), 0, 0)?;
            answers.write(&mut writer, cfg)?;
        }
    }

    Some(writer.finish())
}

#[derive(Clone, Copy)]
struct ParsedQuestion {
    name: &'static str,
    qtype: u16,
    qclass: u16,
}

#[derive(Clone, Copy)]
enum AnswerSet {
    Host,
    ServiceType,
    ServiceInstance,
    Meta,
}

impl AnswerSet {
    fn count(&self) -> u16 {
        match self {
            AnswerSet::Host => 1,
            AnswerSet::ServiceType => 4,
            AnswerSet::ServiceInstance => 3,
            AnswerSet::Meta => 1,
        }
    }

    fn write(&self, writer: &mut DnsWriter<'_>, cfg: &MdnsConfig) -> Option<()> {
        match self {
            AnswerSet::Host => {
                writer.a_record(cfg.host_fqdn, cfg.ip, true, 120)?;
            }
            AnswerSet::ServiceType => {
                writer.ptr_record(cfg.service_type, cfg.service_instance, false, 120)?;
                writer.srv_record(cfg.service_instance, cfg.host_fqdn, cfg.port, true, 120)?;
                writer.txt_record(cfg.service_instance, cfg.service_txt, true, 120)?;
                writer.a_record(cfg.host_fqdn, cfg.ip, true, 120)?;
            }
            AnswerSet::ServiceInstance => {
                writer.srv_record(cfg.service_instance, cfg.host_fqdn, cfg.port, true, 120)?;
                writer.txt_record(cfg.service_instance, cfg.service_txt, true, 120)?;
                writer.a_record(cfg.host_fqdn, cfg.ip, true, 120)?;
            }
            AnswerSet::Meta => {
                writer.ptr_record(META_SERVICE_TYPE, cfg.service_type, false, 120)?;
            }
        }

        Some(())
    }
}

fn parse_query(
    packet: &[u8],
    cfg: &MdnsConfig,
    question_buf: &mut [u8],
) -> Option<(u16, ParsedQuestion, AnswerSet)> {
    if packet.len() < 12 {
        return None;
    }

    let transaction_id = u16::from_be_bytes([packet[0], packet[1]]);
    let flags = u16::from_be_bytes([packet[2], packet[3]]);
    let qdcount = u16::from_be_bytes([packet[4], packet[5]]);
    if flags & 0x8000 != 0 || qdcount == 0 {
        return None;
    }

    let mut cursor = 12;
    for _ in 0..qdcount {
        let name_len = decode_name(packet, &mut cursor, question_buf)?;
        let qtype = read_u16(packet, &mut cursor)?;
        let qclass = read_u16(packet, &mut cursor)?;
        let name = core::str::from_utf8(&question_buf[..name_len]).ok()?;
        if let Some((question, answers)) = classify(name, qtype, qclass, cfg) {
            return Some((transaction_id, question, answers));
        }
    }

    None
}

fn classify(
    query_name: &str,
    qtype: u16,
    qclass: u16,
    cfg: &MdnsConfig,
) -> Option<(ParsedQuestion, AnswerSet)> {
    let class = qclass & 0x7fff;
    if class != DNS_CLASS_IN {
        return None;
    }

    let name = query_name.trim_end_matches('.');
    let host = cfg.host_fqdn.trim_end_matches('.');
    let service_type = cfg.service_type.trim_end_matches('.');
    let service_instance = cfg.service_instance.trim_end_matches('.');
    let meta = META_SERVICE_TYPE.trim_end_matches('.');

    let (canonical_name, answers) = match qtype {
        DNS_TYPE_A if name.eq_ignore_ascii_case(host) => (cfg.host_fqdn, AnswerSet::Host),
        DNS_TYPE_PTR if name.eq_ignore_ascii_case(service_type) => {
            (cfg.service_type, AnswerSet::ServiceType)
        }
        DNS_TYPE_SRV | DNS_TYPE_TXT if name.eq_ignore_ascii_case(service_instance) => {
            (cfg.service_instance, AnswerSet::ServiceInstance)
        }
        DNS_TYPE_PTR if name.eq_ignore_ascii_case(meta) => (META_SERVICE_TYPE, AnswerSet::Meta),
        DNS_TYPE_ANY if name.eq_ignore_ascii_case(host) => (cfg.host_fqdn, AnswerSet::Host),
        DNS_TYPE_ANY if name.eq_ignore_ascii_case(service_instance) => {
            (cfg.service_instance, AnswerSet::ServiceInstance)
        }
        DNS_TYPE_ANY if name.eq_ignore_ascii_case(service_type) => {
            (cfg.service_type, AnswerSet::ServiceType)
        }
        DNS_TYPE_ANY if name.eq_ignore_ascii_case(meta) => (META_SERVICE_TYPE, AnswerSet::Meta),
        _ if name.eq_ignore_ascii_case(host) => (cfg.host_fqdn, AnswerSet::Host),
        _ if name.eq_ignore_ascii_case(service_instance) => {
            (cfg.service_instance, AnswerSet::ServiceInstance)
        }
        _ if name.eq_ignore_ascii_case(service_type) => (cfg.service_type, AnswerSet::ServiceType),
        _ if name.eq_ignore_ascii_case(meta) => (META_SERVICE_TYPE, AnswerSet::Meta),
        _ => return None,
    };

    Some((
        ParsedQuestion {
            name: canonical_name,
            qtype,
            qclass,
        },
        answers,
    ))
}

fn decode_name(packet: &[u8], cursor: &mut usize, out: &mut [u8]) -> Option<usize> {
    let mut out_len = 0;
    loop {
        let len = *packet.get(*cursor)? as usize;
        *cursor += 1;
        if len == 0 {
            break;
        }
        if len & 0xc0 != 0 {
            return None;
        }
        if out_len + usize::from(out_len != 0) + len > out.len() {
            return None;
        }
        if out_len != 0 {
            out[out_len] = b'.';
            out_len += 1;
        }
        let label = packet.get(*cursor..(*cursor + len))?;
        for &byte in label {
            out[out_len] = byte.to_ascii_lowercase();
            out_len += 1;
        }
        *cursor += len;
    }
    Some(out_len)
}

fn read_u16(packet: &[u8], cursor: &mut usize) -> Option<u16> {
    let bytes = packet.get(*cursor..(*cursor + 2))?;
    *cursor += 2;
    Some(u16::from_be_bytes([bytes[0], bytes[1]]))
}

fn mdns_endpoint() -> IpEndpoint {
    IpEndpoint::from((
        to_embassy_ipv4(board::MDNS_MULTICAST_IPV4),
        board::MDNS_PORT,
    ))
}

fn response_endpoint(peer: UdpMetadata) -> IpEndpoint {
    if peer.endpoint.port == board::MDNS_PORT {
        mdns_endpoint()
    } else {
        peer.endpoint
    }
}

fn to_embassy_ipv4(ip: Ipv4Addr) -> Ipv4Address {
    let [a, b, c, d] = ip.octets();
    Ipv4Address::new(a, b, c, d)
}

struct DnsWriter<'a> {
    buf: &'a mut [u8],
    len: usize,
}

impl<'a> DnsWriter<'a> {
    fn new(buf: &'a mut [u8]) -> Self {
        Self { buf, len: 0 }
    }

    fn header(
        &mut self,
        id: u16,
        flags: u16,
        qdcount: u16,
        ancount: u16,
        nscount: u16,
        arcount: u16,
    ) -> Option<()> {
        self.u16(id)?;
        self.u16(flags)?;
        self.u16(qdcount)?;
        self.u16(ancount)?;
        self.u16(nscount)?;
        self.u16(arcount)?;
        Some(())
    }

    fn question(&mut self, name: &str, qtype: u16, qclass: u16) -> Option<()> {
        self.name(name)?;
        self.u16(qtype)?;
        self.u16(qclass)?;
        Some(())
    }

    fn a_record(&mut self, name: &str, ip: Ipv4Addr, unique: bool, ttl: u32) -> Option<()> {
        self.name(name)?;
        self.u16(DNS_TYPE_A)?;
        self.u16(if unique {
            DNS_CLASS_FLUSH
        } else {
            DNS_CLASS_IN
        })?;
        self.u32(ttl)?;
        self.u16(4)?;
        self.bytes(&ip.octets())?;
        Some(())
    }

    fn ptr_record(&mut self, name: &str, target: &str, unique: bool, ttl: u32) -> Option<()> {
        let rdata_len = encoded_name_len(target)?;
        self.name(name)?;
        self.u16(DNS_TYPE_PTR)?;
        self.u16(if unique {
            DNS_CLASS_FLUSH
        } else {
            DNS_CLASS_IN
        })?;
        self.u32(ttl)?;
        self.u16(rdata_len as u16)?;
        self.name(target)?;
        Some(())
    }

    fn srv_record(
        &mut self,
        name: &str,
        target: &str,
        port: u16,
        unique: bool,
        ttl: u32,
    ) -> Option<()> {
        let rdata_len = 6 + encoded_name_len(target)?;
        self.name(name)?;
        self.u16(DNS_TYPE_SRV)?;
        self.u16(if unique {
            DNS_CLASS_FLUSH
        } else {
            DNS_CLASS_IN
        })?;
        self.u32(ttl)?;
        self.u16(rdata_len as u16)?;
        self.u16(0)?;
        self.u16(0)?;
        self.u16(port)?;
        self.name(target)?;
        Some(())
    }

    fn txt_record(&mut self, name: &str, txt: &[u8], unique: bool, ttl: u32) -> Option<()> {
        self.name(name)?;
        self.u16(DNS_TYPE_TXT)?;
        self.u16(if unique {
            DNS_CLASS_FLUSH
        } else {
            DNS_CLASS_IN
        })?;
        self.u32(ttl)?;
        self.u16(txt.len() as u16)?;
        self.bytes(txt)?;
        Some(())
    }

    fn name(&mut self, name: &str) -> Option<()> {
        for label in name.trim_end_matches('.').split('.') {
            let len = u8::try_from(label.len()).ok()?;
            self.u8(len)?;
            self.bytes(label.as_bytes())?;
        }
        self.u8(0)?;
        Some(())
    }

    fn u8(&mut self, value: u8) -> Option<()> {
        self.bytes(&[value])
    }

    fn u16(&mut self, value: u16) -> Option<()> {
        self.bytes(&value.to_be_bytes())
    }

    fn u32(&mut self, value: u32) -> Option<()> {
        self.bytes(&value.to_be_bytes())
    }

    fn bytes(&mut self, bytes: &[u8]) -> Option<()> {
        let end = self.len.checked_add(bytes.len())?;
        if end > self.buf.len() {
            return None;
        }
        self.buf[self.len..end].copy_from_slice(bytes);
        self.len = end;
        Some(())
    }

    fn finish(self) -> usize {
        self.len
    }
}

fn encoded_name_len(name: &str) -> Option<usize> {
    let mut len: usize = 1;
    for label in name.trim_end_matches('.').split('.') {
        len = len.checked_add(label.len() + 1)?;
    }
    Some(len)
}
