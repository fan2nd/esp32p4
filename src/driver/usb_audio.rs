//! UAC1 duplex, mono PCM16 at 16 kHz. One AC and two AS interfaces.
//! No hardware volume controls are advertised; adjust gain in the host mixer.
use core::cell::{Cell, RefCell};
use embassy_usb::{
    Builder, Handler,
    control::{InResponse, OutResponse, Recipient, Request, RequestType},
    descriptor::{SynchronizationType, UsageType},
    driver::{Driver, Endpoint, EndpointType},
    types::InterfaceNumber,
};
use heapless::Deque;

pub const RATE: u32 = 16_000;
pub const MAX_PACKET: usize = 34; // Full-speed: 16 samples + one for clock drift.
pub const TARGET: usize = 64; // 4 ms elastic buffering, independent of USB speed.

pub struct AudioState {
    pub playback: RefCell<Deque<i16, 256>>,
    pub capture: RefCell<Deque<i16, 256>>,
    pub playing: Cell<bool>,
    pub recording: Cell<bool>,
    pub suspended: Cell<bool>,
}
impl Default for AudioState {
    fn default() -> Self {
        Self {
            playback: RefCell::new(Deque::new()),
            capture: RefCell::new(Deque::new()),
            playing: Cell::new(false),
            recording: Cell::new(false),
            suspended: Cell::new(false),
        }
    }
}
impl AudioState {
    fn reset(&self) {
        self.playing.set(false);
        self.recording.set(false);
        self.suspended.set(false);
        self.playback.borrow_mut().clear();
        self.capture.borrow_mut().clear();
    }
}
pub struct Control<'a> {
    pub state: &'a AudioState,
    speaker_interface: u8,
    mic_interface: u8,
    speaker_ep: u8,
    mic_ep: u8,
}
impl<'a> Control<'a> {
    pub fn new(state: &'a AudioState) -> Self {
        Self {
            state,
            speaker_interface: 0,
            mic_interface: 0,
            speaker_ep: 0,
            mic_ep: 0,
        }
    }
    fn owns(&self, r: &Request) -> bool {
        r.request_type == RequestType::Class
            && r.recipient == Recipient::Endpoint
            && (r.index == self.speaker_ep as u16 || r.index == self.mic_ep as u16)
    }
}
impl Handler for Control<'_> {
    fn reset(&mut self) {
        #[cfg(target_arch = "riscv32")]
        crate::diagnostic!("usb_audio: USB bus reset");
        self.state.reset();
    }
    fn configured(&mut self, configured: bool) {
        #[cfg(target_arch = "riscv32")]
        crate::diagnostic!("usb_audio: USB configured={}", configured);
        if !configured {
            self.state.reset();
        }
    }
    fn suspended(&mut self, suspended: bool) {
        self.state.suspended.set(suspended);
        self.state.playback.borrow_mut().clear();
        self.state.capture.borrow_mut().clear();
    }
    fn set_alternate_setting(&mut self, interface: InterfaceNumber, alt: u8) {
        #[cfg(target_arch = "riscv32")]
        crate::diagnostic!("usb_audio: interface={} alt={}", u8::from(interface), alt);
        if u8::from(interface) == self.speaker_interface {
            self.state.playing.set(alt == 1);
            self.state.playback.borrow_mut().clear();
        } else if u8::from(interface) == self.mic_interface {
            self.state.recording.set(alt == 1);
            self.state.capture.borrow_mut().clear();
        }
    }
    fn control_out(&mut self, r: Request, data: &[u8]) -> Option<OutResponse> {
        if !self.owns(&r) {
            return None;
        }
        Some(
            if r.request == 1
                && r.value == 0x100
                && r.length == 3
                && data == &RATE.to_le_bytes()[..3]
            {
                OutResponse::Accepted
            } else {
                OutResponse::Rejected
            },
        )
    }
    fn control_in<'a>(&'a mut self, r: Request, buf: &'a mut [u8]) -> Option<InResponse<'a>> {
        if !self.owns(&r) {
            return None;
        }
        if !matches!(r.request, 0x81..=0x84) || r.value != 0x100 || r.length < 3 || buf.len() < 3 {
            return Some(InResponse::Rejected);
        }
        let rate = if r.request == 0x84 { 0 } else { RATE };
        buf[..3].copy_from_slice(&rate.to_le_bytes()[..3]);
        Some(InResponse::Accepted(&buf[..3]))
    }
}

pub struct Streams<'d, D: Driver<'d>> {
    pub speaker: D::EndpointOut,
    pub mic: D::EndpointIn,
    pub feedback: D::EndpointIn,
}

pub fn build<'d, D: Driver<'d>>(
    builder: &mut Builder<'d, D>,
    control: &'d mut Control<'d>,
) -> Streams<'d, D> {
    let mut function = builder.function(1, 1, 0);
    let mut ac = function.interface();
    let ac_number = u8::from(ac.interface_number());
    control.speaker_interface = ac_number + 1;
    control.mic_interface = ac_number + 2;
    let mut alt = ac.alt_setting(1, 1, 0, None);
    // Header (10) + two input terminals (12 each) + two output terminals (9 each).
    alt.descriptor(0x24, &[1, 0, 1, 52, 0, 2, ac_number + 1, ac_number + 2]);
    alt.descriptor(0x24, &[2, 1, 1, 1, 0, 1, 0, 0, 0, 0]); // USB -> speaker
    alt.descriptor(0x24, &[3, 2, 1, 3, 0, 1, 0]);
    alt.descriptor(0x24, &[2, 3, 1, 2, 0, 1, 0, 0, 0, 0]); // microphone -> USB
    alt.descriptor(0x24, &[3, 4, 1, 1, 0, 3, 0]);

    let mut spk = function.interface();
    spk.alt_setting(1, 2, 0, None);
    let mut alt = spk.alt_setting(1, 2, 0, None);
    alt.descriptor(0x24, &[1, 1, 0, 1, 0]);
    alt.descriptor(0x24, &[2, 1, 1, 2, 16, 1, 0x80, 0x3e, 0]);
    let speaker = alt.alloc_endpoint_out(EndpointType::Isochronous, None, MAX_PACKET as u16, 1);
    // UAC1 feedback is three-byte 10.14; do not advertise UAC2's four-byte format.
    let feedback = alt.alloc_endpoint_in(EndpointType::Isochronous, None, 3, 1);
    alt.endpoint_descriptor(
        speaker.info(),
        SynchronizationType::Asynchronous,
        UsageType::DataEndpoint,
        &[0, feedback.info().addr.into()],
    );
    alt.descriptor(0x25, &[1, 1, 0, 0, 0]);
    alt.endpoint_descriptor(
        feedback.info(),
        SynchronizationType::NoSynchronization,
        UsageType::FeedbackEndpoint,
        &[1, 0],
    );

    let mut mic_if = function.interface();
    mic_if.alt_setting(1, 2, 0, None);
    let mut alt = mic_if.alt_setting(1, 2, 0, None);
    alt.descriptor(0x24, &[1, 4, 0, 1, 0]);
    alt.descriptor(0x24, &[2, 1, 1, 2, 16, 1, 0x80, 0x3e, 0]);
    let mic = alt.alloc_endpoint_in(EndpointType::Isochronous, None, MAX_PACKET as u16, 1);
    alt.endpoint_descriptor(
        mic.info(),
        SynchronizationType::Asynchronous,
        UsageType::DataEndpoint,
        &[0, 0],
    );
    alt.descriptor(0x25, &[1, 1, 0, 0, 0]);
    control.speaker_ep = speaker.info().addr.into();
    control.mic_ep = mic.info().addr.into();
    drop(function);
    builder.handler(control);
    Streams {
        speaker,
        mic,
        feedback,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embassy_usb::driver::Direction;

    fn request() -> Request {
        Request {
            direction: Direction::Out,
            request_type: RequestType::Class,
            recipient: Recipient::Endpoint,
            request: 1,
            value: 0x100,
            index: 1,
            length: 3,
        }
    }
    #[test]
    fn only_declared_rate_and_well_formed_requests_are_accepted() {
        let state = AudioState::default();
        let mut c = Control::new(&state);
        c.speaker_ep = 1;
        c.mic_ep = 0x82;
        assert_eq!(
            c.control_out(request(), &[0x80, 0x3e, 0]),
            Some(OutResponse::Accepted)
        );
        assert_eq!(
            c.control_out(request(), &[0x80, 0xbb, 0]),
            Some(OutResponse::Rejected)
        );
        for len in 0..3 {
            assert_eq!(
                c.control_out(request(), &[0; 3][..len]),
                Some(OutResponse::Rejected)
            );
        }
        let mut r = request();
        r.index = 0x83;
        assert_eq!(c.control_out(r, &[0x80, 0x3e, 0]), None);
        r = request();
        r.value = 0x101;
        assert_eq!(
            c.control_out(r, &[0x80, 0x3e, 0]),
            Some(OutResponse::Rejected)
        );
        r = request();
        r.direction = Direction::In;
        r.request = 0x81;
        r.index = 0x82;
        assert_eq!(
            c.control_in(r, &mut [0; 3]),
            Some(InResponse::Accepted(&[0x80, 0x3e, 0]))
        );
        assert_eq!(c.control_in(r, &mut [0; 2]), Some(InResponse::Rejected));
    }
    #[test]
    fn streams_are_independent_and_resume_preserves_alt_settings() {
        let state = AudioState::default();
        let mut c = Control::new(&state);
        c.speaker_interface = 1;
        c.mic_interface = 2;
        c.set_alternate_setting(InterfaceNumber(1), 1);
        c.set_alternate_setting(InterfaceNumber(2), 1);
        state.playback.borrow_mut().push_back(123).unwrap();
        c.suspended(true);
        assert!(state.playback.borrow().is_empty());
        assert!(state.suspended.get());
        c.suspended(false);
        assert!(state.playing.get() && state.recording.get());
        c.set_alternate_setting(InterfaceNumber(1), 0);
        assert!(!state.playing.get() && state.recording.get());
        c.reset();
        assert!(!state.playing.get() && !state.recording.get());
    }
}
