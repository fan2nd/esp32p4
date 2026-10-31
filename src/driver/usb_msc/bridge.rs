//! Owns all bus operations in the USB task; the MSC task never touches registers.
use core::{
    cell::{Cell, RefCell},
    future::poll_fn,
    task::{Poll, Waker},
};
use embassy_futures::select::{Either, select};
use embassy_usb::driver::{
    self, EndpointAddress, EndpointAllocError, EndpointType, Event, Unsupported,
};

pub const IN: u8 = 1;
pub const OUT: u8 = 2;
const ABORT: u8 = 4;

pub struct Link {
    endpoints: Cell<Option<(EndpointAddress, EndpointAddress)>>,
    pending: Cell<u8>,
    pub epoch: Cell<u32>,
    pub media_epoch: Cell<u32>,
    pub configured: Cell<bool>,
    pub cleared: Cell<u8>,
    pub halted: Cell<u8>,
    pub locked: Cell<bool>,
    bus_waker: RefCell<Option<Waker>>,
    task_waker: RefCell<Option<Waker>>,
}
impl Link {
    pub fn new() -> Self {
        Self {
            endpoints: Cell::new(None),
            pending: Cell::new(0),
            epoch: Cell::new(0),
            media_epoch: Cell::new(0),
            configured: Cell::new(false),
            cleared: Cell::new(3),
            halted: Cell::new(0),
            locked: Cell::new(false),
            bus_waker: RefCell::new(None),
            task_waker: RefCell::new(None),
        }
    }
    pub fn endpoints(&self, input: EndpointAddress, output: EndpointAddress) {
        self.endpoints.set(Some((input, output)));
    }
    fn wake(&self) {
        if let Some(w) = self.task_waker.borrow_mut().take() {
            w.wake();
        }
    }
    fn request(&self, bits: u8) {
        self.pending.set(self.pending.get() | bits);
        if let Some(w) = self.bus_waker.borrow_mut().take() {
            w.wake();
        }
    }
    pub fn class_reset(&self) {
        self.locked.set(false);
        self.cleared.set(0);
        self.epoch.set(self.epoch.get().wrapping_add(1));
        self.request(ABORT);
        self.wake();
    }
    pub fn configure(&self, on: bool) {
        self.configured.set(on);
        if on {
            self.media_epoch.set(self.media_epoch.get().wrapping_add(1));
        }
        self.locked.set(false);
        self.cleared.set(3);
        self.halted.set(0);
        self.pending.set(0);
        self.epoch.set(self.epoch.get().wrapping_add(1));
        self.wake();
    }
    pub async fn wait(&self, ready: impl Fn() -> bool) {
        poll_fn(|cx| {
            *self.task_waker.borrow_mut() = Some(cx.waker().clone());
            if ready() {
                Poll::Ready(())
            } else {
                Poll::Pending
            }
        })
        .await
    }
    pub async fn halt(&self, bits: u8) {
        self.request(bits);
        self.wait(|| self.pending.get() == 0).await;
    }
    pub async fn ready(&self) {
        self.wait(|| {
            self.configured.get()
                && self.pending.get() == 0
                && self.cleared.get() == 3
                && self.halted.get() == 0
        })
        .await;
    }
    fn bit(&self, address: EndpointAddress) -> u8 {
        match self.endpoints.get() {
            Some((i, _)) if address == i => IN,
            Some((_, o)) if address == o => OUT,
            _ => 0,
        }
    }
}

// Bus polling is cancelled by embassy-usb before control dispatch. Sharing the
// bus through RefCell lets the control status hook finish BOT reset synchronously
// before ACK, without raw register access or a concurrent mutable bus alias.
pub struct Driver<'a, 'hw, D: driver::Driver<'hw>> {
    inner: D,
    link: &'a Link,
    bus: &'a RefCell<Option<D::Bus>>,
}
impl<'a, 'hw, D: driver::Driver<'hw>> Driver<'a, 'hw, D> {
    pub fn new(inner: D, link: &'a Link, bus: &'a RefCell<Option<D::Bus>>) -> Self {
        Self { inner, link, bus }
    }
}
impl<'a, 'hw: 'a, D: driver::Driver<'hw>> driver::Driver<'a> for Driver<'a, 'hw, D> {
    type EndpointIn = D::EndpointIn;
    type EndpointOut = D::EndpointOut;
    type ControlPipe = ControlPipe<'a, D::ControlPipe, D::Bus>;
    type Bus = Bus<'a, D::Bus>;
    fn alloc_endpoint_in(
        &mut self,
        t: EndpointType,
        a: Option<EndpointAddress>,
        n: u16,
        p: u8,
    ) -> Result<Self::EndpointIn, EndpointAllocError> {
        self.inner.alloc_endpoint_in(t, a, n, p)
    }
    fn alloc_endpoint_out(
        &mut self,
        t: EndpointType,
        a: Option<EndpointAddress>,
        n: u16,
        p: u8,
    ) -> Result<Self::EndpointOut, EndpointAllocError> {
        self.inner.alloc_endpoint_out(t, a, n, p)
    }
    fn start(self, n: u16) -> (Self::Bus, Self::ControlPipe) {
        let (bus, control) = self.inner.start(n);
        *self.bus.borrow_mut() = Some(bus);
        (
            Bus {
                inner: self.bus,
                link: self.link,
            },
            ControlPipe {
                inner: control,
                bus: self.bus,
                link: self.link,
            },
        )
    }
}
fn apply<B: driver::Bus>(inner: &mut B, link: &Link) {
    let pending = link.pending.replace(0);
    if let Some((input, output)) = link.endpoints.get() {
        if pending & ABORT != 0 {
            for ep in [input, output] {
                let stalled = inner.endpoint_is_stalled(ep);
                inner.endpoint_set_enabled(ep, false);
                inner.endpoint_set_enabled(ep, true);
                if stalled {
                    inner.endpoint_set_stalled(ep, true);
                }
            }
        }
        for (bit, ep) in [(IN, input), (OUT, output)] {
            if pending & bit != 0 {
                inner.endpoint_set_stalled(ep, true);
                link.halted.set(link.halted.get() | bit);
            }
        }
    }
    link.wake();
}
pub struct ControlPipe<'a, C, B> {
    inner: C,
    bus: &'a RefCell<Option<B>>,
    link: &'a Link,
}
impl<C: driver::ControlPipe, B: driver::Bus> driver::ControlPipe for ControlPipe<'_, C, B> {
    fn max_packet_size(&self) -> usize {
        self.inner.max_packet_size()
    }
    async fn setup(&mut self) -> [u8; 8] {
        self.inner.setup().await
    }
    async fn data_out(
        &mut self,
        b: &mut [u8],
        first: bool,
        last: bool,
    ) -> Result<usize, driver::EndpointError> {
        self.inner.data_out(b, first, last).await
    }
    async fn data_in(
        &mut self,
        b: &[u8],
        first: bool,
        last: bool,
    ) -> Result<(), driver::EndpointError> {
        self.inner.data_in(b, first, last).await
    }
    async fn accept(&mut self) {
        if self.link.pending.get() & ABORT != 0 {
            apply(self.bus.borrow_mut().as_mut().unwrap(), self.link);
        }
        self.inner.accept().await;
    }
    async fn reject(&mut self) {
        self.inner.reject().await;
    }
    async fn accept_set_address(&mut self, a: u8) {
        self.inner.accept_set_address(a).await;
    }
}
pub struct Bus<'a, B> {
    inner: &'a RefCell<Option<B>>,
    link: &'a Link,
}
impl<B: driver::Bus> driver::Bus for Bus<'_, B> {
    async fn poll(&mut self) -> Event {
        loop {
            let mut guard = self.inner.borrow_mut();
            let inner = guard.as_mut().unwrap();
            if self.link.pending.get() != 0 {
                apply(inner, self.link);
            }
            let request = poll_fn(|cx| {
                *self.link.bus_waker.borrow_mut() = Some(cx.waker().clone());
                if self.link.pending.get() != 0 {
                    Poll::Ready(())
                } else {
                    Poll::Pending
                }
            });
            match select(inner.poll(), request).await {
                Either::First(event) => return event,
                Either::Second(()) => apply(inner, self.link),
            }
        }
    }
    async fn enable(&mut self) {
        self.inner.borrow_mut().as_mut().unwrap().enable().await;
    }
    async fn disable(&mut self) {
        self.inner.borrow_mut().as_mut().unwrap().disable().await;
    }
    fn endpoint_set_enabled(&mut self, a: EndpointAddress, on: bool) {
        self.inner
            .borrow_mut()
            .as_mut()
            .unwrap()
            .endpoint_set_enabled(a, on);
    }
    fn endpoint_set_stalled(&mut self, a: EndpointAddress, stalled: bool) {
        let bit = self.link.bit(a);
        let stalled = stalled || (bit != 0 && self.link.locked.get());
        self.inner
            .borrow_mut()
            .as_mut()
            .unwrap()
            .endpoint_set_stalled(a, stalled);
        if bit != 0 {
            if stalled {
                self.link.halted.set(self.link.halted.get() | bit);
            } else {
                self.link.halted.set(self.link.halted.get() & !bit);
                self.link.cleared.set(self.link.cleared.get() | bit);
            }
            self.link.wake();
        }
    }
    fn endpoint_is_stalled(&mut self, a: EndpointAddress) -> bool {
        self.inner
            .borrow_mut()
            .as_mut()
            .unwrap()
            .endpoint_is_stalled(a)
    }
    fn force_reset(&mut self) -> Result<(), Unsupported> {
        self.inner.borrow_mut().as_mut().unwrap().force_reset()
    }
    async fn remote_wakeup(&mut self) -> Result<(), Unsupported> {
        self.inner
            .borrow_mut()
            .as_mut()
            .unwrap()
            .remote_wakeup()
            .await
    }
}
