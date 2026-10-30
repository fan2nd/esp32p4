use esp_hal::Blocking;
use esp_hal::delay::Delay;
use esp_hal::i2c::master::I2c;

pub const ES8311_ADDR: u8 = 0x18;
pub const SAMPLE_RATE: u32 = 16_000;
pub const SAMPLE_BITS: u8 = 16;

pub struct Es8311;

impl Es8311 {
    pub fn init(i2c: &mut I2c<'static, Blocking>) -> Result<Self, ()> {
        // Reset, enable clocks/analog blocks, and select 16-bit I2S.
        for (index, (register, value)) in [
            (0x00, 0x1F),
            (0x00, 0x00),
            (0x00, 0x80),
            (0x01, 0x3F),
            // HAL supplies MCLK = 16 kHz * 256 = 4.096 MHz: no pre-divider.
            // Espressif es8311.c coefficient row: 4096000 Hz / 16000 Hz.
            (0x02, 0x00),
            (0x03, 0x10),
            (0x04, 0x10),
            (0x05, 0x00),
            (0x06, 0x03),
            (0x07, 0x00),
            (0x08, 0xFF),
            (0x09, 0x0C),
            (0x0A, 0x0C),
            (0x0D, 0x01),
            (0x0E, 0x02),
            (0x12, 0x00),
            (0x13, 0x10),
            // Official init: bypass ADC/DAC equalizers and cancel ADC DC offset.
            (0x1C, 0x6A),
            (0x37, 0x08),
            (0x17, 0xC8), // Analog microphone ADC gain from microphone_config.
            (0x14, 0x1A),
            // DAC volume: -95.5 dB + register * 0.5 dB. 0xBF is unity;
            // 0xFF adds +32 dB and clips ordinary USB PCM at low host volume.
            (0x32, 0xBF),
        ]
        .into_iter()
        .enumerate()
        {
            i2c.write(ES8311_ADDR, &[register, value]).map_err(|_| ())?;
            if index == 0 {
                Delay::new().delay_millis(20);
            }
        }
        Ok(Self)
    }
}
