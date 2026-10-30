#![no_std]
#![no_main]

use core::cell::RefCell;
use embassy_futures::{block_on, join::join, yield_now};
use embassy_usb::driver::{Endpoint, EndpointIn, EndpointOut};
use esp_hal::{
    delay::Delay,
    gpio::{Level, Output, OutputConfig},
    i2c::master::{Config as I2cConfig, I2c},
    i2s::master::{Channels, DataFormat, I2s, TdmConfig},
    time::{Duration, Instant, Rate},
    usb::otg::{
        Usb,
        embassy_usb_device::{Config as DriverConfig, Driver},
    },
};
use panic_halt as _;
use riscv_rt::entry;
use waveshare_p4_eth::driver::{
    audio::Es8311,
    usb_audio::{self, AudioState, Control, MAX_PACKET, TARGET},
};

esp_bootloader_esp_idf::esp_app_desc!();

#[entry]
fn main() -> ! {
    let p = esp_hal::init(esp_hal::Config::default());
    let mut amplifier = Output::new(p.GPIO53, Level::Low, OutputConfig::default());
    let mut i2c = I2c::new(p.I2C1, I2cConfig::default())
        .unwrap()
        .with_sda(p.GPIO7)
        .with_scl(p.GPIO8);
    let i2s = I2s::new(
        p.I2S0,
        p.DMA_CH0,
        TdmConfig::new_tdm_philips()
            .with_sample_rate(Rate::from_hz(16_000))
            .with_data_format(DataFormat::Data16Channel16)
            .with_channels(Channels::STEREO)
            .with_signal_loopback(true),
    )
    .unwrap()
    .with_mclk(p.GPIO13);
    // TX is the sole clock master; RX uses its BCK/WS internally.
    let tx = i2s
        .i2s_tx
        .with_bclk(p.GPIO12)
        .with_ws(p.GPIO10)
        .with_dout(p.GPIO9)
        .build();
    let rx = i2s.i2s_rx.with_din(p.GPIO11).build();
    Es8311::init(&mut i2c).expect("ES8311 init");
    let mut tx_buffer = esp_hal::dma_tx_stream_buffer!(1024, 128);
    tx_buffer.push(&[0; 1024]);
    let rx_buffer = esp_hal::dma_rx_stream_buffer!(4096, 128);
    let mut tx = tx.write(tx_buffer).unwrap();
    let mut rx = rx.read(rx_buffer).unwrap();
    Delay::new().delay_millis(1);

    let state = AudioState::default();
    let mut control = Control::new(&state);
    let mut ep_buffer = [0; 256];
    let mut usb_config = DriverConfig::default();
    // UAC1 uses 1 ms full-speed frames; HS playback is not validated on Windows.
    // The same J2 HS controller/PHY remains in use, limited to 12 Mbit/s.
    usb_config.force_full_speed = true;
    let driver = Driver::new(Usb::new_hs(p.USB_HS), &mut ep_buffer, usb_config);
    // Laboratory VID/PID only. Replace with an allocated pair for distribution.
    let mut config = embassy_usb::Config::new(0xcafe, 0x4011);
    config.manufacturer = Some("P4 Lab");
    config.product = Some("P4 USB Microphone + Speaker");
    config.serial_number = Some("P4-AUDIO-001");
    config.self_powered = true;
    config.max_power = 0;
    let mut descriptors = [0; 512];
    let mut bos = [0; 64];
    let mut msos = [0; 64];
    let mut control_buffer = [0; 64];
    let mut builder = embassy_usb::Builder::new(
        driver,
        config,
        &mut descriptors,
        &mut bos,
        &mut msos,
        &mut control_buffer,
    );
    let usb_audio::Streams {
        mut speaker,
        mut mic,
        mut feedback,
    } = usb_audio::build(&mut builder, &mut control);
    let mut usb = builder.build();
    // All futures run cooperatively on this one thread. RefCell borrows never
    // cross an await; the USB ISR only touches the HAL endpoint state.
    let last_packet = RefCell::new(Instant::now());
    esp_println::println!(
        "usb_audio: UAC1 mono PCM16 16000 Hz; USB-A HS controller, full-speed UAC1"
    );

    let playback = async {
        let mut packet = [0; MAX_PACKET];
        loop {
            speaker.wait_enabled().await;
            while let Ok(n) = speaker.read(&mut packet).await {
                if state.playing.get() && !state.suspended.get() && n % 2 == 0 {
                    *last_packet.borrow_mut() = Instant::now();
                    let mut q = state.playback.borrow_mut();
                    for bytes in packet[..n].as_chunks::<2>().0 {
                        if q.is_full() {
                            q.pop_front();
                        }
                        q.push_back(i16::from_le_bytes([bytes[0], bytes[1]]))
                            .unwrap();
                    }
                }
            }
        }
    };
    let capture = async {
        let mut packet = [0; MAX_PACKET];
        loop {
            mic.wait_enabled().await;
            let nominal = 16; // One full-speed packet per millisecond.
            let mut primed = false;
            loop {
                let n = {
                    let mut q = state.capture.borrow_mut();
                    if q.len() >= TARGET {
                        primed = true;
                    }
                    let n = if !primed {
                        nominal
                    } else if q.len() > TARGET + 16 {
                        nominal + 1
                    } else if q.len() < TARGET - 16 {
                        nominal - 1
                    } else {
                        nominal
                    };
                    for sample in packet[..n * 2].as_chunks_mut::<2>().0 {
                        let value = if primed {
                            q.pop_front().unwrap_or(0)
                        } else {
                            0i16
                        };
                        sample.copy_from_slice(&value.to_le_bytes());
                    }
                    n * 2
                };
                if mic.write(&packet[..n]).await.is_err() {
                    break;
                }
            }
        }
    };
    let clock_feedback = async {
        loop {
            feedback.wait_enabled().await;

            loop {
                // Occupancy servo: +/- 0.5% around 16 kHz. The host adjusts
                // packet sizes to match the independently clocked codec.
                let error = TARGET as i32 - state.playback.borrow().len() as i32;
                let correction = (error * 8).clamp(-1310, 1310);
                let value = (16 * 16384 + correction) as u32; // UAC1 10.14 samples/frame.
                if feedback.write(&value.to_le_bytes()[..3]).await.is_err() {
                    break;
                }
            }
        }
    };
    let audio = async {
        let mut input = [0; 128];
        let mut output = [0; 128];
        let mut primed = false;
        loop {
            let n = rx.pop(&mut input);
            if state.recording.get() && !state.suspended.get() {
                let mut q = state.capture.borrow_mut();
                for frame in input[..n].as_chunks::<4>().0 {
                    if q.is_full() {
                        q.pop_front();
                    }
                    // ES8311 ADC occupies left I2S slot.
                    q.push_back(i16::from_le_bytes([frame[0], frame[1]]))
                        .unwrap();
                }
            }
            // HAL RX EOF is finite even with a circular DMA descriptor chain.
            if rx.is_done() && rx.available_bytes() == 0 {
                let (receiver, buffer) = rx.stop();
                rx = receiver.read(buffer).unwrap();
            }
            let active = state.playing.get()
                && !state.suspended.get()
                && last_packet.borrow().elapsed() < Duration::from_millis(100);
            if !active {
                amplifier.set_low();
                state.playback.borrow_mut().clear();
                primed = false;
            }
            let available = tx.available_bytes().min(output.len()) / 4 * 4;
            if available > 0 {
                let mut q = state.playback.borrow_mut();
                if active && q.len() >= TARGET {
                    primed = true;
                }
                if active && primed {
                    amplifier.set_high();
                }
                for frame in output[..available].as_chunks_mut::<4>().0 {
                    let sample = if active && primed {
                        q.pop_front().unwrap_or(0)
                    } else {
                        0i16
                    };
                    frame[..2].copy_from_slice(&sample.to_le_bytes());
                    frame[2..].copy_from_slice(&sample.to_le_bytes());
                }
                tx.push(&output[..available]);
            }
            if tx.is_done() {
                amplifier.set_low();
                esp_println::println!("usb_audio: TX DMA underrun; restarting");
                let (sender, buffer) = tx.stop();
                let (descriptors, bytes) = buffer.split();
                let mut buffer = esp_hal::dma::DmaTxStreamBuf::new(descriptors, bytes).unwrap();
                buffer.push(&[0; 1024]);
                tx = sender.write(buffer).unwrap();
                primed = false;
            }
            yield_now().await;
        }
    };
    // This bare-metal binary uses block_on rather than an executor, so it
    // must enable machine interrupts for the USB and DMA handlers itself.
    unsafe { riscv::interrupt::enable() };
    esp_println::println!(
        "usb_audio: interrupts enabled={}",
        riscv::register::mstatus::read().mie()
    );
    block_on(join(
        usb.run(),
        join(audio, join(playback, join(capture, clock_feedback))),
    ));
    loop {
        core::hint::spin_loop();
    }
}
