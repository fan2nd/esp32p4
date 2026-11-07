//! UAC1/FS or UAC2/HS duplex, mono PCM16 at 16 kHz. One AC and two AS interfaces.
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
    high_speed: bool,
}
impl<'a> Control<'a> {
    pub fn new(state: &'a AudioState) -> Self {
        Self {
            state,
            speaker_interface: 0,
            mic_interface: 0,
            speaker_ep: 0,
            mic_ep: 0,
            high_speed: false,
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
        if self.high_speed {
            // UAC2 uses an interface/entity clock control, not endpoint sample-rate requests.
            if r.request_type != RequestType::Class
                || r.recipient != Recipient::Interface
                || r.index != (5 << 8 | u16::from(self.speaker_interface - 1))
            {
                return None;
            }
            let len = match (r.request, r.value) {
                (1, 0x100) => {
                    buf[..4].copy_from_slice(&RATE.to_le_bytes());
                    4
                }
                (2, 0x100) => {
                    buf[..2].copy_from_slice(&1u16.to_le_bytes());
                    buf[2..6].copy_from_slice(&RATE.to_le_bytes());
                    buf[6..10].copy_from_slice(&RATE.to_le_bytes());
                    buf[10..14].fill(0); // One fixed rate; no programmable step.
                    14
                }
                (1, 0x200) => {
                    buf[0] = 1; // Codec clock is valid.
                    1
                }
                _ => return Some(InResponse::Rejected),
            };
            return Some(InResponse::Accepted(&buf[..len]));
        }
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
    high_speed: bool,
) -> Streams<'d, D> {
    control.high_speed = high_speed;
    let protocol = if high_speed { 0x20 } else { 0 };
    let packet_size = if high_speed { 6 } else { MAX_PACKET as u16 };
    let mut function = builder.function(1, 1, protocol);
    let mut ac = function.interface();
    let ac_number = u8::from(ac.interface_number());
    control.speaker_interface = ac_number + 1;
    control.mic_interface = ac_number + 2;
    let mut alt = ac.alt_setting(1, 1, protocol, None);
    if high_speed {
        // UAC2: 75-byte AC topology, shared fixed 16 kHz clock source (entity 5).
        alt.descriptor(0x24, &[1, 0, 2, 8, 75, 0, 0]);
        alt.descriptor(0x24, &[0x0a, 5, 1, 5, 0, 0]);
        alt.descriptor(0x24, &[2, 1, 1, 1, 0, 5, 1, 0, 0, 0, 0, 0, 0, 0, 0]);
        alt.descriptor(0x24, &[3, 2, 1, 3, 0, 1, 5, 0, 0, 0]);
        alt.descriptor(0x24, &[2, 3, 1, 2, 0, 5, 1, 0, 0, 0, 0, 0, 0, 0, 0]);
        alt.descriptor(0x24, &[3, 4, 1, 1, 0, 3, 5, 0, 0, 0]);
    } else {
        // Header (10) + two input terminals (12 each) + two output terminals (9 each).
        alt.descriptor(0x24, &[1, 0, 1, 52, 0, 2, ac_number + 1, ac_number + 2]);
        alt.descriptor(0x24, &[2, 1, 1, 1, 0, 1, 0, 0, 0, 0]); // USB -> speaker
        alt.descriptor(0x24, &[3, 2, 1, 3, 0, 1, 0]);
        alt.descriptor(0x24, &[2, 3, 1, 2, 0, 1, 0, 0, 0, 0]); // microphone -> USB
        alt.descriptor(0x24, &[3, 4, 1, 1, 0, 3, 0]);
    }

    let mut spk = function.interface();
    spk.alt_setting(1, 2, protocol, None);
    let mut alt = spk.alt_setting(1, 2, protocol, None);
    if high_speed {
        alt.descriptor(0x24, &[1, 1, 0, 1, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0]);
        alt.descriptor(0x24, &[2, 1, 2, 16]);
    } else {
        alt.descriptor(0x24, &[1, 1, 0, 1, 0]);
        alt.descriptor(0x24, &[2, 1, 1, 2, 16, 1, 0x80, 0x3e, 0]);
    }
    // bInterval=1: one FS frame or HS microframe, with one extra sample for drift.
    let speaker = alt.alloc_endpoint_out(EndpointType::Isochronous, None, packet_size, 1);
    let feedback = alt.alloc_endpoint_in(
        EndpointType::Isochronous,
        None,
        if high_speed { 4 } else { 3 },
        1,
    );
    let sync_address = [0, feedback.info().addr.into()];
    alt.endpoint_descriptor(
        speaker.info(),
        SynchronizationType::Asynchronous,
        UsageType::DataEndpoint,
        if high_speed {
            &[]
        } else {
            &sync_address
        },
    );
    alt.descriptor(
        0x25,
        if high_speed {
            &[1, 0, 0, 0, 0, 0]
        } else {
            &[1, 1, 0, 0, 0]
        },
    );
    alt.endpoint_descriptor(
        feedback.info(),
        SynchronizationType::NoSynchronization,
        UsageType::FeedbackEndpoint,
        if high_speed { &[] } else { &[1, 0] },
    );

    let mut mic_if = function.interface();
    mic_if.alt_setting(1, 2, protocol, None);
    let mut alt = mic_if.alt_setting(1, 2, protocol, None);
    if high_speed {
        alt.descriptor(0x24, &[1, 4, 0, 1, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0]);
        alt.descriptor(0x24, &[2, 1, 2, 16]);
    } else {
        alt.descriptor(0x24, &[1, 4, 0, 1, 0]);
        alt.descriptor(0x24, &[2, 1, 1, 2, 16, 1, 0x80, 0x3e, 0]);
    }
    let mic = alt.alloc_endpoint_in(EndpointType::Isochronous, None, packet_size, 1);
    alt.endpoint_descriptor(
        mic.info(),
        SynchronizationType::Asynchronous,
        UsageType::DataEndpoint,
        if high_speed { &[] } else { &[0, 0] },
    );
    alt.descriptor(
        0x25,
        if high_speed {
            &[1, 0, 0, 0, 0, 0]
        } else {
            &[1, 1, 0, 0, 0]
        },
    );
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
