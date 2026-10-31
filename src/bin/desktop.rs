#![no_std]
#![no_main]

use core::cell::{Cell, RefCell};
use embassy_executor::{Executor, Spawner};
use embassy_futures::{join::join, yield_now};
use embassy_usb::class::hid::{Config as HidConfig, HidReaderWriter, HidWriter, State};
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
use esp_hal::{
    sdmmc::{Config as SdConfig, SdHostController, SlotConfig},
    system::{CpuControl, Stack},
};
use esp_storage::FlashStorage;
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    waveshare_p4_eth::diagnostic!(
        "desktop PANIC core={:?}: {}",
        esp_hal::system::Cpu::current(),
        info
    );
    loop {
        core::hint::spin_loop();
    }
}
use riscv_rt::entry;
// Report CPU exceptions as well as Rust panics during board bring-up.
#[unsafe(no_mangle)]
unsafe extern "C" fn ExceptionHandler(_frame: *mut u32) -> ! {
    waveshare_p4_eth::diagnostic!(
        "desktop EXCEPTION core={:?} cause={:#x} pc={:#x} value={:#x}",
        esp_hal::system::Cpu::current(),
        riscv::register::mcause::read().bits(),
        riscv::register::mepc::read(),
        riscv::register::mtval::read()
    );
    loop {
        core::hint::spin_loop();
    }
}
use static_cell::StaticCell;
use waveshare_p4_eth::driver::{
    audio::Es8311,
    usb_audio::{self, AudioState, Control, MAX_PACKET},
};
use waveshare_p4_eth::{desktop, desktop_ui, keyboard::Keymap};

esp_bootloader_esp_idf::esp_app_desc!();
static EXECUTOR: StaticCell<Executor> = StaticCell::new();
static SCREEN_STACK: StaticCell<Stack<65536>> = StaticCell::new();
static SDHOST: StaticCell<SdHostController<'static>> = StaticCell::new();
// Composite UI workloads can delay a poll by 3-5 ms. Keep 8 ms of USB PCM
// queued before consuming it; 4 ms left no margin for a delayed packet batch.
const TARGET: usize = 128;

#[entry]
fn main() -> ! {
    let p =
        esp_hal::init(esp_hal::Config::default().with_cpu_clock(esp_hal::clock::CpuClock::max()));
    waveshare_p4_eth::diagnostics::init();
    EXECUTOR.init(Executor::new()).run(move |spawner| {
        spawner.spawn(waveshare_p4_eth::diagnostics::run()).unwrap();
        spawner
            .spawn(esp_p4_eth::time_driver::time_polling_task())
            .unwrap();
        spawner.spawn(run(p, spawner)).unwrap();
    })
}
#[embassy_executor::task]
async fn run(p: esp_hal::peripherals::Peripherals, spawner: Spawner) {
    let mut flash = FlashStorage::new(p.FLASH);
    let mut keymap = Keymap::default();
    let mut generation = 0u32;
    let mut active_slot = 1;
    let mut table = [0; 0xc00];
    let writable = flash.read(0x8000, &mut table).is_ok()
        && table.chunks_exact(32).any(|e| {
            e[..4] == [0xaa, 0x50, 1, 2]
                && e[4..8] == 0xff0000u32.to_le_bytes()
                && e[8..12] == 0x2000u32.to_le_bytes()
                && &e[12..19] == b"kbdmap\0"
        });
    if writable {
        for slot in 0..2 {
            let mut record = [0; waveshare_p4_eth::keyboard::RECORD];
            if flash.read(0xff0000 + slot * 4096, &mut record).is_ok() {
                if let Some((map, seq)) = Keymap::restore(&record) {
                    if generation == 0 || (seq.wrapping_sub(generation) as i32) > 0 {
                        keymap = map;
                        generation = seq;
                        active_slot = slot;
                    }
                }
            }
        }
    }
    critical_section::with(|cs| {
        *desktop::SHARED.borrow(cs).borrow_mut() = Some(desktop::Shared::new(keymap))
    });
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
    // Static DMA storage is internal SRAM, not the display's PSRAM arena.
    // 4096 bytes / (16 kHz * two 16-bit slots) = 64 ms of scheduling headroom.
    let tx_buffer = esp_hal::dma_tx_stream_buffer!(4096, 128);
    let (tx_descriptors, tx_bytes) = tx_buffer.split();
    waveshare_p4_eth::diagnostic!("audio: TX SRAM data={:#x} descriptors={:#x} bytes={}",
        tx_bytes.as_ptr() as usize, tx_descriptors.as_ptr() as usize, tx_bytes.len());
    let mut tx_buffer = esp_hal::dma::DmaTxStreamBuf::new(tx_descriptors, tx_bytes).unwrap();
    tx_buffer.push_with(|bytes| { bytes.fill(0); bytes.len() });
    let rx_buffer = esp_hal::dma_rx_stream_buffer!(4096, 128);
    let mut tx = tx.write(tx_buffer).unwrap();
    let mut rx = rx.read(rx_buffer).unwrap();
    Delay::new().delay_millis(1);

    let mut cpu = CpuControl::new(p.CPU_CTRL);
    let _screen_guard = cpu
        .start_app_core(SCREEN_STACK.init(Stack::new()), move || {
            desktop_ui::run(p.MIPI_DSI, p.VDMA_CH0, p.PSRAM, i2c)
        })
        .unwrap();
    let host = SDHOST.init(SdHostController::new(p.SDHOST, SdConfig::default()).unwrap());
    let bus = host
        .slot::<1>(SlotConfig::default())
        .unwrap()
        .with_clk(p.GPIO18)
        .with_cmd(p.GPIO19)
        .with_data0(p.GPIO14)
        .with_data1(p.GPIO15)
        .with_data2(p.GPIO16)
        .with_data3(p.GPIO17)
        .into_async();
    let reset = Output::new(p.GPIO54, Level::Low, OutputConfig::default());
    spawner
        .spawn(waveshare_p4_eth::task::desktop_radio::run(bus, reset))
        .unwrap();
    let state = AudioState::default();
    let mut control = Control::new(&state);
    let mut ep_buffer = [0; 512];
    let mut usb_config = DriverConfig::default();
    // UAC1 uses 1 ms full-speed frames; HS playback is not validated on Windows.
    // The same J2 HS controller/PHY remains in use, limited to 12 Mbit/s.
    usb_config.force_full_speed = true;
    let driver = Driver::new(Usb::new_hs(p.USB_HS), &mut ep_buffer, usb_config);
    // Laboratory VID/PID only. Replace with an allocated pair for distribution.
    let mut config = embassy_usb::Config::new(0xcafe, 0x4014);
    config.manufacturer = Some("P4 Lab");
    config.product = Some("P4 Desktop Keyboard + Audio");
    config.serial_number = Some("P4-DESKTOP-001");
    config.self_powered = true;
    config.max_power = 0;
    let mut descriptors = [0; 1024];
    let mut bos = [0; 64];
    let mut msos = [0; 64];
    let mut control_buffer = [0; 64];
    let mut keyboard_state = State::new();
    let mut consumer_state = State::new();
    let mut raw_state = State::new();
    let mut usb_status = UsbStatus { configured: false };
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
    let mut keyboard = HidWriter::<_, 8>::new(
        &mut builder,
        &mut keyboard_state,
        HidConfig {
            report_descriptor: KEYBOARD_DESC,
            request_handler: None,
            poll_ms: 1,
            max_packet_size: 8,
        },
    );
    let mut consumer = HidWriter::<_, 2>::new(
        &mut builder,
        &mut consumer_state,
        HidConfig {
            report_descriptor: CONSUMER_DESC,
            request_handler: None,
            poll_ms: 1,
            max_packet_size: 2,
        },
    );
    let (mut raw_in, mut raw_out) = HidReaderWriter::<_, 32, 32>::new(
        &mut builder,
        &mut raw_state,
        HidConfig {
            report_descriptor: RAW_DESC,
            request_handler: None,
            poll_ms: 1,
            max_packet_size: 32,
        },
    )
    .split();
    builder.handler(&mut usb_status);
    let mut usb = builder.build();
    // All futures run cooperatively on this one thread. RefCell borrows never
    // cross an await; the USB ISR only touches the HAL endpoint state.
    let last_packet = RefCell::new(Instant::now());
    let received_samples = Cell::new(0u32);
    let discarded_samples = Cell::new(0u32);
    let received_packets = Cell::new(0u32);
    let feedback_sum = Cell::new(0i64);
    let feedback_count = Cell::new(0u32);
    waveshare_p4_eth::diagnostic!(
        "usb_audio: UAC1 mono PCM16 16000 Hz; USB-A HS controller, full-speed UAC1"
    );

    let hid = async {
        loop {
            keyboard.ready().await;
            consumer.ready().await;
            let _ = keyboard.write(&[0; 8]).await;
            let _ = consumer.write(&[0; 2]).await;
            loop {
                let report = desktop::with(|s| s.reports.pop_front());
                if let Some(report) = report {
                    if keyboard.write(&report[..8]).await.is_err()
                        || consumer.write(&report[8..]).await.is_err()
                    {
                        break;
                    }
                }
                if !desktop::with(|s| s.view.usb) {
                    break;
                }
                yield_now().await;
            }
        }
    };
    let via = async {
        let mut packet = [0; 32];
        loop {
            raw_in.ready().await;
            if let Ok(32) = raw_in.read(&mut packet).await {
                desktop::with(|s| {
                    s.keymap.via(
                        &mut packet,
                        Instant::now().duration_since_epoch().as_millis() as u32,
                        s.pressed,
                    )
                });
                let _ = raw_out.write(&packet).await;
            }
            yield_now().await;
        }
    };
    let save = async {
        let mut observed = 0;
        let mut saved = 0;
        let mut changed = Instant::now();
        loop {
            yield_now().await;
            let revision = desktop::with(|s| s.keymap.revision);
            if revision != observed {
                observed = revision;
                changed = Instant::now();
            }
            if !writable
                || revision == saved
                || changed.elapsed() < Duration::from_secs(2)
                || state.playing.get()
                || state.recording.get()
                || desktop::with(|s| {
                    s.pressed != 0 || s.keymap.bytes[waveshare_p4_eth::keyboard::PAYLOAD - 1] != 0
                })
            {
                continue;
            }
            let record = desktop::with(|s| s.keymap.record(generation.wrapping_add(1)));
            desktop::FLASH_REQUEST.store(true, core::sync::atomic::Ordering::Release);
            let timeout = Instant::now();
            while !desktop::FLASH_PAUSED.load(core::sync::atomic::Ordering::Acquire)
                && timeout.elapsed() < Duration::from_secs(2)
            {
                yield_now().await;
            }
            if desktop::FLASH_PAUSED.load(core::sync::atomic::Ordering::Acquire) {
                let slot = 1 - active_slot;
                let mut verify = [0; waveshare_p4_eth::keyboard::RECORD];
                if flash.write(0xff0000 + slot * 4096, &record).is_ok()
                    && flash.read(0xff0000 + slot * 4096, &mut verify).is_ok()
                    && record == verify
                {
                    saved = revision;
                    generation = generation.wrapping_add(1);
                    active_slot = slot;
                }
            }
            desktop::FLASH_REQUEST.store(false, core::sync::atomic::Ordering::Release);
            changed = Instant::now();
        }
    };
    let playback = async {
        let mut packet = [0; MAX_PACKET];
        loop {
            speaker.wait_enabled().await;
            while let Ok(n) = speaker.read(&mut packet).await {
                if state.playing.get() && !state.suspended.get() && n % 2 == 0 {
                    *last_packet.borrow_mut() = Instant::now();
                    received_samples.set(received_samples.get() + (n / 2) as u32);
                    received_packets.set(received_packets.get() + 1);
                    let mut q = state.playback.borrow_mut();
                    for bytes in packet[..n].as_chunks::<2>().0 {
                        if q.is_full() {
                            q.pop_front();
                            discarded_samples.set(discarded_samples.get() + 1);
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
                feedback_sum.set(feedback_sum.get() + correction as i64);
                feedback_count.set(feedback_count.get() + 1);
                let value = (16 * 16384 + correction) as u32; // UAC1 10.14 samples/frame.
                if feedback.write(&value.to_le_bytes()[..3]).await.is_err() {
                    break;
                }
            }
        }
    };
    let audio = async {
        // Drain/refill up to 16 ms per poll to catch up after a delayed poll;
        // the old 2 ms cap could not recover while UART output was active.
        let mut input = [0; 1024];
        let mut output = [0; 1024];
        let mut primed = false;
        let mut stats_at = Instant::now();
        let mut polled_at = stats_at;
        let mut max_gap_us = 0;
        let mut missing = 0u32;
        let mut sent = 0u32;
        let mut restarts = 0u32;
        let mut max_work_us = 0;
        loop {
            let now = Instant::now();
            max_gap_us = max_gap_us.max((now - polled_at).as_micros());
            polled_at = now;
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
                        sent += 1;
                        q.pop_front().unwrap_or_else(|| { missing += 1; 0 })
                    } else {
                        0i16
                    };
                    frame[..2].copy_from_slice(&sample.to_le_bytes());
                    frame[2..].copy_from_slice(&sample.to_le_bytes());
                }
                tx.push(&output[..available]);
            }
            if tx.is_done() {
                restarts += 1;
                amplifier.set_low();
                waveshare_p4_eth::diagnostic!("usb_audio: TX DMA underrun; restarting");
                let (sender, buffer) = tx.stop();
                let (descriptors, bytes) = buffer.split();
                let mut buffer = esp_hal::dma::DmaTxStreamBuf::new(descriptors, bytes).unwrap();
                buffer.push_with(|bytes| { bytes.fill(0); bytes.len() });
                tx = sender.write(buffer).unwrap();
                primed = false;
            }
            // Aggregate instead of logging per packet: distinguish USB sample
            // starvation from a stalled DMA chain or executor scheduling delay.
            if stats_at.elapsed() >= Duration::from_secs(5) {
                waveshare_p4_eth::diagnostic!(
                    "audio stats: rx={} out={} empty={} drop={} restart={} gap_us={}",
                    received_samples.replace(0), sent, missing,
                    discarded_samples.replace(0), restarts, max_gap_us);
                waveshare_p4_eth::diagnostic!("audio detail: packets={} feedback={} work_us={}",
                    received_packets.replace(0),
                    feedback_sum.replace(0) / i64::from(feedback_count.replace(0).max(1)),
                    max_work_us);
                stats_at = now;
                sent = 0; missing = 0; restarts = 0; max_gap_us = 0; max_work_us = 0;
            }
            max_work_us = max_work_us.max((Instant::now() - now).as_micros());
            yield_now().await;
        }
    };
    // USB and DMA handlers must be enabled before polling the joined services.
    unsafe { riscv::interrupt::enable() };
    waveshare_p4_eth::diagnostic!(
        "usb_audio: interrupts enabled={}",
        riscv::register::mstatus::read().mie()
    );
    join(
        usb.run(),
        join(
            audio,
            join(
                playback,
                join(capture, join(clock_feedback, join(hid, join(via, save)))),
            ),
        ),
    )
    .await;
}
struct UsbStatus {
    configured: bool,
}
impl embassy_usb::Handler for UsbStatus {
    fn enabled(&mut self, enabled: bool) {
        if !enabled {
            self.configured(false);
        }
    }
    fn reset(&mut self) {
        self.configured(false);
    }
    fn configured(&mut self, on: bool) {
        self.configured = on;
        desktop::with(|s| {
            s.view.usb = on;
            s.view.revision += 1;
            s.reports.clear();
        });
    }
    fn suspended(&mut self, suspended: bool) {
        // Without VBUS sensing, cable removal may only produce Suspend.
        // Keep configuration separately: Resume does not enumerate again.
        let active = self.configured && !suspended;
        desktop::with(|s| {
            if s.view.usb != active {
                s.view.usb = active;
                s.view.revision += 1;
            }
            if !active { s.reports.clear(); }
        });
        log::info!("desktop: USB suspended={} active={}", suspended, active);
    }
}
const KEYBOARD_DESC: &[u8] = &[
    0x05, 1, 0x09, 6, 0xa1, 1, 0x05, 7, 0x19, 0xe0, 0x29, 0xe7, 0x15, 0, 0x25, 1, 0x75, 1, 0x95, 8,
    0x81, 2, 0x75, 8, 0x95, 1, 0x81, 1, 0x19, 0, 0x29, 0xff, 0x15, 0, 0x26, 0xff, 0, 0x75, 8, 0x95,
    6, 0x81, 0, 0xc0,
];
const CONSUMER_DESC: &[u8] = &[
    0x05, 0x0c, 0x09, 1, 0xa1, 1, 0x15, 0, 0x26, 0xff, 3, 0x19, 0, 0x2a, 0xff, 3, 0x75, 16, 0x95,
    1, 0x81, 0, 0xc0,
];
const RAW_DESC: &[u8] = &[
    0x06, 0x60, 0xff, 0x09, 0x61, 0xa1, 1, 0x09, 0x62, 0x15, 0, 0x26, 0xff, 0, 0x75, 8, 0x95, 32,
    0x81, 2, 0x09, 0x63, 0x95, 32, 0x91, 2, 0xc0,
];
