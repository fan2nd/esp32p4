#![no_std]
#![no_main]

use esp_hal::delay::Delay;
use esp_hal::gpio::{Input, InputConfig, Level, Output, OutputConfig, OutputSignal, Pull};
use esp_hal::i2c::master::{Config as I2cConfig, I2c};
use esp_hal::i2s::master::{Channels, DataFormat, I2s, TdmConfig};
use esp_hal::psram::{Psram, PsramConfig};
use esp_hal::time::{Duration, Instant, Rate};
use panic_halt as _;
use riscv_rt::entry;
use waveshare_p4_eth::driver::audio::Es8311;

esp_bootloader_esp_idf::esp_app_desc!();

#[entry]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    // NS4150B amplifier is active-high; keep it muted outside playback.
    let mut amplifier = Output::new(peripherals.GPIO53, Level::Low, OutputConfig::default());
    // BOOT is active-low GPIO35; this audio-only bin does not use Ethernet TXD1.
    let boot = Input::new(
        peripherals.GPIO35,
        InputConfig::default().with_pull(Pull::Up),
    );
    let i2s = I2s::new(
        peripherals.I2S0,
        peripherals.DMA_CH0,
        TdmConfig::new_tdm_philips()
            .with_sample_rate(Rate::from_hz(16_000))
            .with_data_format(DataFormat::Data16Channel16)
            .with_channels(Channels::STEREO),
    )
    .expect("I2S init failed")
    .with_mclk(peripherals.GPIO13);
    let bclk = peripherals.GPIO12;
    let ws = peripherals.GPIO10;
    // Keep the pins to switch clock sources between sequential RX and TX.
    // These aliases are configured once here; only one transfer runs at a time.
    let bclk_rx = unsafe { bclk.clone_unchecked() };
    let ws_rx = unsafe { ws.clone_unchecked() };
    let mut i2s_tx = i2s
        .i2s_tx
        .with_bclk(unsafe { bclk.clone_unchecked() })
        .with_ws(unsafe { ws.clone_unchecked() })
        .with_dout(peripherals.GPIO9)
        .build();
    let mut i2s_rx = i2s
        .i2s_rx
        .with_bclk(bclk_rx)
        .with_ws(ws_rx)
        .with_din(peripherals.GPIO11)
        .build();
    esp_println::println!("verify: i2s 16k/16bit ready");
    let psram = Psram::new(peripherals.PSRAM, PsramConfig::default());
    let (psram_ptr, psram_len) = psram.raw_parts();
    let mut i2c = I2c::new(peripherals.I2C1, I2cConfig::default())
        .expect("I2C init failed")
        .with_sda(peripherals.GPIO7)
        .with_scl(peripherals.GPIO8);
    Es8311::init(&mut i2c).expect("ES8311 init failed");
    esp_println::println!("verify: es8311 ready");

    // Audio-only firmware owns PSRAM; no display framebuffer is allocated.
    // Stereo 16-bit transfer: 16,000 frames/s * 4 bytes * 3 seconds.
    const AUDIO_SIZE: usize = 192_000;
    assert!(psram_len >= AUDIO_SIZE);
    // SAFETY: PSRAM is initialized, large enough, and not shared in this bin.
    let audio = unsafe { core::slice::from_raw_parts_mut(psram_ptr, AUDIO_SIZE) };
    // Allocate once and reclaim both DMA buffers after each transfer.
    let mut rx_buffer = esp_hal::dma_rx_stream_buffer!(AUDIO_SIZE, 1024);
    let mut tx_buffer = esp_hal::dma_tx_stream_buffer!(AUDIO_SIZE, 1024);
    let delay = Delay::new();
    loop {
        // Require a stable release before arming, so a held button cannot repeat.
        loop {
            while boot.is_low() {
                delay.delay_millis(10);
            }
            delay.delay_millis(30);
            if boot.is_high() {
                break;
            }
        }
        esp_println::println!("verify_audio: ready; press BOOT to record 3 seconds");
        loop {
            while boot.is_high() {
                delay.delay_millis(10);
            }
            delay.delay_millis(30);
            if boot.is_low() {
                break;
            }
        }
        esp_println::println!("verify_audio: recording start, 3 seconds");
        OutputSignal::I2S0_I_BCK.connect_to(&bclk);
        OutputSignal::I2S0_I_WS.connect_to(&ws);
        let mut rx = i2s_rx.read(rx_buffer).unwrap();
        esp_println::println!("verify_audio: RX DMA started");
        let started = Instant::now();
        let mut reported = started;
        let mut written = 0;
        while written < AUDIO_SIZE {
            if rx.available_bytes() != 0 {
                written += rx.pop(&mut audio[written..]);
            }
            if written == AUDIO_SIZE {
                break;
            }
            if started.elapsed() >= Duration::from_secs(5) {
                esp_println::println!(
                    "verify_audio: recording timeout, {}/{} bytes",
                    written,
                    AUDIO_SIZE
                );
                break;
            }
            if reported.elapsed() >= Duration::from_secs(1) {
                esp_println::println!(
                    "verify_audio: recording {}/{} bytes, rx_done={}",
                    written,
                    AUDIO_SIZE,
                    rx.is_done()
                );
                reported = Instant::now();
            }
            // HAL RX has a finite hardware EOF count even with a stream buffer.
            // Drain the final descriptor before restarting the stopped receiver.
            if rx.is_done() {
                written += rx.pop(&mut audio[written..]);
                let (receiver, buffer) = rx.stop();
                rx = receiver.read(buffer).unwrap();
                esp_println::println!("verify_audio: RX EOF, resumed at {} bytes", written);
            }
            delay.delay_millis(1);
        }
        (i2s_rx, rx_buffer) = rx.stop();
        if written != AUDIO_SIZE {
            esp_println::println!("verify_audio: incomplete recording; playback skipped");
            continue;
        }
        esp_println::println!("verify_audio: recording complete, {} bytes", written);
        let mut minimum = [i16::MAX; 2];
        let mut maximum = [i16::MIN; 2];
        for frame in audio[..written].chunks_exact(4) {
            for channel in 0..2 {
                let sample = i16::from_le_bytes([frame[channel * 2], frame[channel * 2 + 1]]);
                minimum[channel] = minimum[channel].min(sample);
                maximum[channel] = maximum[channel].max(sample);
            }
        }
        esp_println::println!(
            "verify_audio: PCM L min={} max={}, R min={} max={}",
            minimum[0],
            maximum[0],
            minimum[1],
            maximum[1]
        );
        assert_eq!(tx_buffer.push(&audio[..written]), written);
        // RX has stopped: route the running TX unit's clocks to the codec.
        OutputSignal::I2S0_O_BCK.connect_to(&bclk);
        OutputSignal::I2S0_O_WS.connect_to(&ws);
        amplifier.set_high();
        delay.delay_millis(10);
        esp_println::println!("verify_audio: playback start, {} bytes", written);
        let tx = i2s_tx.write(tx_buffer).unwrap();
        let started = Instant::now();
        while !tx.is_done() && started.elapsed() < Duration::from_secs(5) {
            delay.delay_millis(1);
        }
        let completed = tx.is_done();
        amplifier.set_low();
        (i2s_tx, tx_buffer) = tx.stop();
        // Reset the pre-fill cursor while reusing the same DMA memory next time.
        let (descriptors, buffer) = tx_buffer.split();
        tx_buffer = esp_hal::dma::DmaTxStreamBuf::new(descriptors, buffer).unwrap();
        esp_println::println!(
            "verify_audio: playback {}",
            if completed { "done" } else { "timeout" }
        );
    }
}
