# Audio verification

Run `cargo run --release --bin verify_audio`. Wait for the ready log, then
press BOOT (active-low GPIO35). After 30 ms debounce, capture 192,000 bytes
at 16 kHz, stereo 16-bit (three seconds), then play the captured buffer.
Recording and playback each have start and completion logs.

After playback, release BOOT before the next press. Verify two consecutive
record/playback cycles and a long press: holding BOOT must not retrigger.
DMA buffers are reused, with the TX pre-fill cursor reset between cycles.
This standalone bin does not enable Ethernet, which shares GPIO35.

Build verification: `cargo build --release --bin verify_audio`.
Actual recording and playback require hardware verification.

Playback fixes: GPIO53 enables the NS4150B amplifier only during playback.
GPIO12/GPIO10 select RX BCLK/WS for capture and TX BCLK/WS for playback;
the GPIO matrix can route only one output signal to each pin at a time.
Previously RX setup replaced the TX routes, leaving the codec without TX
clocks after RX stopped. PCM min/max per channel is logged before playback
to distinguish constant input data from varying recorded samples.

Codec coefficients now match HAL's 4.096 MHz MCLK (16 kHz * 256): REG02=0,
REG06=3. ADC/DAC equalizer bypass and microphone ADC gain follow
[Espressif's ES8311 driver](https://github.com/espressif/esp-bsp/blob/master/components/es8311/es8311.c).
Verify audible playback and two successive BOOT record/playback cycles on hardware.

Recording diagnostics: `RX DMA started` distinguishes returning from HAL
startup from waiting for samples. Every second the bin reports received bytes
and RX completion. It drains and restarts RX on EOF until 192,000 bytes have
been captured. HAL 1.2.0 programs a 16-bit RX EOF count; whether this explains
the observed stall still requires hardware logs. Capture and playback waits
time out after five seconds. Incomplete recordings are not played.
