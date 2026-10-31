//! Compatibility between Hosted's owned xarxa packets and embassy-net 0.9 tokens.
//! A bounded static pool owns all buffers; no allocator or packet lifetime leaks.
use core::{
    cell::UnsafeCell,
    mem::MaybeUninit,
    ptr::NonNull,
    sync::atomic::{AtomicBool, Ordering},
    task::Context,
};
use embassy_net_driver as old;
use xarxa_driver::{Driver, PacketBuf, RawPacketBuf};
struct Slot {
    used: AtomicBool,
    raw: UnsafeCell<MaybeUninit<RawPacketBuf>>,
    data: UnsafeCell<[u8; 1514]>,
}
// A successful CAS transfers exclusive access until PacketBuf's free callback.
unsafe impl Sync for Slot {}
static POOL: [Slot; 12] = [const {
    Slot {
        used: AtomicBool::new(false),
        raw: UnsafeCell::new(MaybeUninit::uninit()),
        data: UnsafeCell::new([0; 1514]),
    }
}; 12];
unsafe fn release(raw: NonNull<RawPacketBuf>) {
    let slot = unsafe { &*(raw.as_ref().pool.cast::<Slot>()) };
    slot.used.store(false, Ordering::Release);
}
fn allocate() -> Option<PacketBuf> {
    for slot in &POOL {
        if slot
            .used
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            // Slot and storage are static, uniquely owned until release().
            let raw = unsafe {
                (&mut *slot.raw.get()).write(RawPacketBuf {
                    free: release,
                    pool: (slot as *const Slot).cast(),
                    data: NonNull::new(slot.data.get().cast()).unwrap(),
                    capacity: 1514,
                    headroom: 0,
                    len: 0,
                    meta: Default::default(),
                })
            };
            return Some(unsafe { PacketBuf::from_raw(NonNull::from(raw)) });
        }
    }
    None
}
pub struct HostedNet(pub embassy_net_esp_hosted::NetDriver<'static>);
pub struct Rx(PacketBuf);
pub struct Tx<'a> {
    device: &'a mut embassy_net_esp_hosted::NetDriver<'static>,
    buffer: PacketBuf,
}
impl old::RxToken for Rx {
    fn consume<R, F: FnOnce(&mut [u8]) -> R>(mut self, f: F) -> R {
        f(&mut self.0)
    }
}
impl old::TxToken for Tx<'_> {
    fn consume<R, F: FnOnce(&mut [u8]) -> R>(mut self, len: usize, f: F) -> R {
        self.buffer.set_len(len);
        let result = f(&mut self.buffer);
        assert!(self.device.transmit(self.buffer).is_ok());
        result
    }
}
impl HostedNet {
    fn prepare(&mut self, cx: &mut Context<'_>) {
        let _ = self.0.register_waker(cx.waker());
        while self.0.rx_wanted() > 0 {
            if let Some(buffer) = allocate() {
                self.0.rx_give(buffer);
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn packet_pool_exhaustion_and_cross_thread_return_preserve_ownership() {
        let mut packets = std::vec::Vec::new();
        for i in 0..12 {
            let mut p = allocate().expect("slot available");
            p.set_len(1514);
            p.fill(i);
            assert!(
                packets
                    .iter()
                    .all(|other: &PacketBuf| other.as_ptr() != p.as_ptr())
            );
            packets.push(p);
        }
        assert!(allocate().is_none());
        let returned = packets.pop().unwrap();
        std::thread::spawn(move || drop(returned)).join().unwrap();
        let mut reused = allocate().expect("cross-thread release publishes slot");
        reused.set_len(1514);
        reused.fill(255);
        for (i, p) in packets.iter().enumerate() {
            assert!(p.iter().all(|b| *b == i as u8));
        }
        drop(reused);
        drop(packets);
        let all: std::vec::Vec<_> = (0..12).map(|_| allocate().unwrap()).collect();
        assert!(allocate().is_none());
        drop(all);
    }
}
impl old::Driver for HostedNet {
    type RxToken<'a>
        = Rx
    where
        Self: 'a;
    type TxToken<'a>
        = Tx<'a>
    where
        Self: 'a;
    fn receive(&mut self, cx: &mut Context<'_>) -> Option<(Rx, Tx<'_>)> {
        self.prepare(cx);
        if !self.0.can_transmit() {
            return None;
        }
        let buffer = allocate()?;
        let received = self.0.receive()?;
        Some((
            Rx(received),
            Tx {
                device: &mut self.0,
                buffer,
            },
        ))
    }
    fn transmit(&mut self, cx: &mut Context<'_>) -> Option<Tx<'_>> {
        self.prepare(cx);
        if !self.0.can_transmit() {
            return None;
        }
        let buffer = allocate()?;
        Some(Tx {
            device: &mut self.0,
            buffer,
        })
    }
    fn link_state(&mut self, cx: &mut Context<'_>) -> old::LinkState {
        self.prepare(cx);
        match self.0.link_state() {
            xarxa_driver::LinkState::Up => old::LinkState::Up,
            _ => old::LinkState::Down,
        }
    }
    fn capabilities(&self) -> old::Capabilities {
        let mut c = old::Capabilities::default();
        c.max_transmission_unit = 1514;
        c.max_burst_size = Some(4);
        c
    }
    fn hardware_address(&self) -> old::HardwareAddress {
        match self.0.hardware_address() {
            xarxa_driver::HardwareAddress::Ethernet(mac) => old::HardwareAddress::Ethernet(mac),
            _ => panic!("Hosted must be initialized before constructing Ethernet stack"),
        }
    }
}
