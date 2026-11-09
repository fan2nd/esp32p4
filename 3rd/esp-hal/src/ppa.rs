//! Experimental ESP32-P4 blocking PPA scaling/rotation/mirroring.
//!
//! Adapted from esp-rs/esp-hal PR #6476, upstream commit
//! e045405e6f036a7d8b2ef22bc877457dc3e6eba2 (MIT OR Apache-2.0).
//! P4-specific clock, DMA2D register access, 16x16 blocks and cache maintenance.
//! Destination buffers must be 128-byte aligned and have a 128-byte multiple length
//! even in internal SRAM: P4 DMA bypasses the CPU caches. No heap allocation.
//! Only RGB formats are enabled initially; YUV/GRAY require additional silicon validation.

use core::sync::atomic::{AtomicBool, Ordering, compiler_fence};

use crate::{
    pac,
    soc::{is_slice_in_dram, is_slice_in_psram},
    time::Instant,
};

const ALIGNMENT: usize = 128;
const MAX_DIMENSION: usize = 0x3fff;
// SRM output dimensions are limited to 13 bits.
const MAX_SRM_DIMENSION: usize = 0x1fff;
const OUTPUT_BLOCK_SIZE: Size = Size {
    width: 2,
    height: 2,
};

/// Image dimensions in pixels.
///
/// Both dimensions must be nonzero and no greater than `0x3fff`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[instability::unstable]
pub struct Size {
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
}

impl Size {
    fn is_valid(self, max_dimension: usize) -> bool {
        self.width != 0
            && self.height != 0
            && self.width <= max_dimension
            && self.height <= max_dimension
    }
}

/// Position within a destination image, in pixels.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[instability::unstable]
pub struct Offset {
    /// Horizontal offset in pixels.
    pub x: usize,
    /// Vertical offset in pixels.
    pub y: usize,
}

/// Pixel formats supported by the PPA SRM operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
#[instability::unstable]
pub enum PixelFormat {
    /// Packed UYVY 4:2:2, using limited-range BT.601 color conversion.
    ///
    /// Input widths must be even. Output widths, horizontal offsets, and transformed region
    /// widths must be even.
    Uyvy422,
    /// RGB565 in native byte order.
    ///
    /// Conversion to 8-bit channels fills the least significant bits with zeros.
    Rgb565,
    /// RGB888, stored as B, G, R bytes.
    Rgb888,
    /// ARGB8888 in native byte order, stored as B, G, R, A bytes.
    ///
    /// Input alpha is preserved. Inputs without alpha are treated as opaque.
    Argb8888,
    /// One byte per pixel, using `(85 * R + 86 * G + 85 * B) >> 8` for RGB input.
    Gray8,
}

impl PixelFormat {
    /// Returns the number of bytes per pixel.
    #[instability::unstable]
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Gray8 => 1,
            Self::Rgb565 | Self::Uyvy422 => 2,
            Self::Rgb888 => 3,
            Self::Argb8888 => 4,
        }
    }

    fn dma_pixel_size(self) -> u32 {
        match self {
            Self::Gray8 => 1,
            Self::Rgb565 | Self::Uyvy422 => 3,
            Self::Rgb888 => 4,
            Self::Argb8888 => 5,
        }
    }

    fn color_mode(self) -> u8 {
        match self {
            Self::Uyvy422 => 9,
            Self::Rgb565 => 2,
            Self::Rgb888 => 1,
            Self::Argb8888 => 0,
            Self::Gray8 => 12,
        }
    }
}

/// Counterclockwise image rotation.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
#[instability::unstable]
pub enum Rotation {
    /// No rotation.
    #[default]
    Degrees0,
    /// 90 degrees.
    Degrees90,
    /// 180 degrees.
    Degrees180,
    /// 270 degrees.
    Degrees270,
}

impl Rotation {
    fn angle(self) -> u8 {
        match self {
            Self::Degrees0 => 0,
            Self::Degrees90 => 1,
            Self::Degrees180 => 2,
            Self::Degrees270 => 3,
        }
    }

    fn output_size(self, size: Size) -> Size {
        match self {
            Self::Degrees0 | Self::Degrees180 => size,
            Self::Degrees90 | Self::Degrees270 => Size {
                width: size.height,
                height: size.width,
            },
        }
    }
}

/// Configuration for a PPA SRM operation.
///
/// Scaling uses the source axes and bilinear interpolation. Factors must be at least `1/16`
/// and less than `256`, and are truncated to multiples of `1/16`.
/// Rotation follows scaling. Mirroring uses the rotated image's axes.
/// Each transformed dimension must be no greater than `0x1fff`.
#[derive(Debug, Clone, Copy, PartialEq, procmacros::BuilderLite)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
#[instability::unstable]
pub struct Config {
    /// Rotation applied to the source image.
    rotation: Rotation,
    /// Horizontal scaling factor. See [`Config`] for the range and precision.
    scale_x: f32,
    /// Vertical scaling factor. See [`Config`] for the range and precision.
    scale_y: f32,
    /// Horizontal mirroring.
    mirror_x: bool,
    /// Vertical mirroring.
    mirror_y: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            rotation: Rotation::default(),
            scale_x: 1.0,
            scale_y: 1.0,
            mirror_x: false,
            mirror_y: false,
        }
    }
}

impl Config {
    /// Returns the transformed image dimensions.
    ///
    /// Scaled dimensions are rounded down to whole pixels and exchanged for 90-degree and
    /// 270-degree rotations.
    ///
    /// # Errors
    ///
    /// - [`Error::InvalidScale`] when a scaling factor is invalid.
    /// - [`Error::InvalidSize`] when the source or transformed dimensions are invalid.
    #[instability::unstable]
    pub fn output_size(&self, size: Size) -> Result<Size, Error> {
        if !size.is_valid(MAX_DIMENSION) {
            return Err(Error::InvalidSize);
        }
        if !(1.0 / 16.0..256.0).contains(&self.scale_x)
            || !(1.0 / 16.0..256.0).contains(&self.scale_y)
        {
            return Err(Error::InvalidScale);
        }
        let size = self.rotation.output_size(Size {
            width: size.width * (self.scale_x * 16.0) as usize / 16,
            height: size.height * (self.scale_y * 16.0) as usize / 16,
        });
        if !size.is_valid(MAX_SRM_DIMENSION) {
            return Err(Error::InvalidSize);
        }
        Ok(size)
    }
}

/// Source image for an SRM operation.
#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[instability::unstable]
pub struct Input<'a> {
    /// Source pixels. See the [buffer requirements][self].
    pub data: &'a [u8],
    /// Source image dimensions.
    pub size: Size,
    /// Pixel format.
    pub format: PixelFormat,
}

/// Destination image for an SRM operation.
///
/// The transformed image must fit within [`Self::size`] at [`Self::offset`].
#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[instability::unstable]
pub struct Output<'a> {
    /// Destination pixels. See the [buffer requirements][self].
    pub data: &'a mut [u8],
    /// Full destination image dimensions.
    pub size: Size,
    /// Pixel format.
    pub format: PixelFormat,
    /// Position of the converted image within the destination, in pixels.
    pub offset: Offset,
}

/// Errors returned by the PPA SRM operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[non_exhaustive]
#[instability::unstable]
pub enum Error {
    /// Invalid image dimensions, destination offset, or buffer length.
    InvalidSize,
    /// This format has not been enabled for this P4 implementation.
    UnsupportedFormat,
    /// A scaling factor is outside the supported range or is NaN.
    InvalidScale,
    /// A destination address or length is not 128-byte aligned.
    UnalignedBuffer,
    /// A buffer is outside internal RAM or initialized PSRAM.
    UnsupportedMemoryRegion,
    /// A 2D-DMA or PPA operation error.
    Hardware,
    /// Completion deadline expired; channels were drained before returning.
    Timeout,
}

impl core::error::Error for Error {}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnsupportedFormat => f.write_str("unsupported P4 pixel format"),
            Self::InvalidSize => f.write_str("invalid image dimensions, offset, or buffer length"),
            Self::InvalidScale => f.write_str("invalid scaling factor"),
            Self::UnalignedBuffer => {
                f.write_str("destination address or length is not 128-byte aligned")
            }
            Self::UnsupportedMemoryRegion => {
                f.write_str("image buffer is outside internal RAM or initialized PSRAM")
            }
            Self::Hardware => f.write_str("SRM hardware error"),
            Self::Timeout => f.write_str("SRM completion timed out"),
        }
    }
}

/// PPA SRM driver.
///
/// The driver owns the PPA and the entire 2D-DMA controller.
#[derive(Debug)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[instability::unstable]
pub struct Srm {
    _private: (),
}
static TAKEN: AtomicBool = AtomicBool::new(false);

impl Srm {
    /// Exclusively claims PPA and the whole DMA2D controller. The current PAC has no
    /// DMA2D singleton; this cross-core atomic prevents two safe drivers owning it.
    pub fn take() -> Option<Self> {
        TAKEN
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()?;
        critical_section::with(|_| {
            let clock = unsafe { &*pac::HP_SYS_CLKRST::ptr() };
            clock.soc_clk_ctrl1().modify(|_, w| {
                w.ppa_sys_clk_en().set_bit();
                w.dma2d_sys_clk_en().set_bit()
            });
            clock.hp_rst_en1().modify(|_, w| w.rst_en_ppa().set_bit());
            clock.hp_rst_en1().modify(|_, w| w.rst_en_ppa().clear_bit());
            clock.hp_rst_en0().modify(|_, w| w.rst_en_dma2d().set_bit());
            clock
                .hp_rst_en0()
                .modify(|_, w| w.rst_en_dma2d().clear_bit());
        });
        Reg(0xa04).write(7); // DMA2D clock on, reset both AXI interfaces.
        Reg(0xa04).write(4);
        // Connect SRM before waiting for RESET_AVAIL: an unconnected channel may
        // never assert it (Espressif issue #19108).
        Reg(0x38).write(1);
        Reg(0x53c).write(1);
        Some(Self { _private: () })
    }

    /// Scales, rotates, mirrors, and converts a source image into a destination region.
    ///
    /// The requirements for images and transformations are in [`Input`], [`Output`], and
    /// [`Config`]. Buffers must also meet the [module-level requirements][self].
    ///
    /// On success, pixels outside the destination region are unchanged.
    ///
    /// # Errors
    ///
    /// - [`Error::InvalidSize`] when dimensions, offsets, or buffer lengths are invalid.
    /// - [`Error::InvalidScale`] when a scaling factor is invalid.
    /// - [`Error::UnalignedBuffer`] when a destination address or length is not 128-byte
    ///   aligned.
    /// - [`Error::UnsupportedMemoryRegion`] when a buffer is outside the supported memory regions.
    /// - [`Error::Hardware`] on a peripheral error.
    ///
    /// The destination contents are unspecified after a hardware error.
    #[instability::unstable]
    pub fn run(
        &mut self,
        input: Input<'_>,
        output: Output<'_>,
        config: Config,
    ) -> Result<(), Error> {
        if matches!(input.format, PixelFormat::Uyvy422 | PixelFormat::Gray8)
            || matches!(output.format, PixelFormat::Uyvy422 | PixelFormat::Gray8)
        {
            return Err(Error::UnsupportedFormat);
        }
        let transformed = config.output_size(input.size)?;
        if !output.size.is_valid(MAX_DIMENSION)
            || input.data.len()
                < input.size.width * input.size.height * input.format.bytes_per_pixel()
            || output.data.len()
                < output.size.width * output.size.height * output.format.bytes_per_pixel()
        {
            return Err(Error::InvalidSize);
        }
        if input.format == PixelFormat::Uyvy422 && !input.size.width.is_multiple_of(2) {
            return Err(Error::InvalidSize);
        }
        if output.format == PixelFormat::Uyvy422
            && (!output.size.width.is_multiple_of(2)
                || !output.offset.x.is_multiple_of(2)
                || !transformed.width.is_multiple_of(2))
        {
            return Err(Error::InvalidSize);
        }
        if output.offset.x > output.size.width
            || output.offset.y > output.size.height
            || transformed.width > output.size.width - output.offset.x
            || transformed.height > output.size.height - output.offset.y
        {
            return Err(Error::InvalidSize);
        }
        if !(is_slice_in_dram(input.data) || is_slice_in_psram(input.data))
            || !(is_slice_in_dram(output.data) || is_slice_in_psram(output.data))
        {
            return Err(Error::UnsupportedMemoryRegion);
        }
        if !(output.data.as_ptr() as usize).is_multiple_of(ALIGNMENT)
            || !output.data.len().is_multiple_of(ALIGNMENT)
        {
            return Err(Error::UnalignedBuffer);
        }

        let mut tx = Descriptor::new(
            input.data.as_ptr(),
            input.size,
            input.size,
            Offset::default(),
            input.format,
        );
        let mut rx = Descriptor::new(
            output.data.as_mut_ptr(),
            output.size,
            OUTPUT_BLOCK_SIZE,
            output.offset,
            output.format,
        );
        let tx_address = (&raw mut tx) as u32;
        let rx_address = (&raw mut rx) as u32;
        let ppa = unsafe { &*pac::PPA::ptr() };
        self.stop_channels();
        // Reset the channel/FIFO while no memory transactions are in flight.
        let channel_config = (1 << 2) | (4 << 6) | (3 << 9) | (1 << 11) | (1 << 12);
        Reg(0x00).write(channel_config | 1);
        Reg(0x500).write(channel_config | 1);
        Reg(0x00).write(channel_config);
        Reg(0x500).write(channel_config);
        ppa.sr_scal_rotate()
            .modify(|_, w| w.scal_rotate_rst().set_bit());
        ppa.sr_scal_rotate()
            .modify(|_, w| w.scal_rotate_rst().clear_bit());
        // 16x16 macroblocks plus a two-pixel interpolation border, valid on P4.
        Reg(0x6c).write(18 | (18 << 14));
        Reg(0x10).write(u32::MAX);
        Reg(0x510).write(u32::MAX);

        ppa.sr_byte_order().write(|w| w.sr_bk_size_sel().set_bit());
        ppa.sr_color_mode().write(|w| unsafe {
            w.sr_rx_cm().bits(input.format.color_mode());
            w.sr_tx_cm().bits(output.format.color_mode());
            w.yuv_rx_range().clear_bit();
            w.yuv2rgb_protocal().clear_bit();
            w.yuv_tx_range().clear_bit();
            w.rgb2yuv_protocal().clear_bit();
            w.yuv422_rx_byte_order().bits(0)
        });
        ppa.sr_fix_alpha().reset();
        ppa.rgb2gray().reset();
        let scale_x = (config.scale_x * 16.0) as u16;
        let scale_y = (config.scale_y * 16.0) as u16;
        // P4 DIG-734: a partial last transfer unit below the FIFO depth can
        // suppress EOF. Match ESP-IDF ppa_srm_transaction_on_picked's workaround.
        let (height, vertical_scale) = match config.rotation {
            Rotation::Degrees90 | Rotation::Degrees270 => (input.size.width, scale_x as usize),
            _ => (input.size.height, scale_y as usize),
        };
        let divisor = if output.format == PixelFormat::Rgb565 {
            64
        } else {
            32
        };
        let tail_width = (transformed.width - 1) % divisor + 1;
        let tail_height = ((height - 1) % 16 + 1) * vertical_scale / 16;
        let bypass = (transformed.width > divisor || height > 16)
            && tail_width * tail_height * output.format.bytes_per_pixel() * 8 < 12 * 128;
        ppa.sr_byte_order()
            .modify(|_, w| w.sr_macro_bk_ro_bypass().bit(bypass));
        ppa.sr_scal_rotate().write(|w| unsafe {
            w.sr_scal_x_int().bits((scale_x >> 4) as u8);
            w.sr_scal_x_frag().bits((scale_x & 15) as u8);
            w.sr_scal_y_int().bits((scale_y >> 4) as u8);
            w.sr_scal_y_frag().bits((scale_y & 15) as u8);
            w.sr_rotate_angle().bits(config.rotation.angle());
            w.sr_mirror_x().bit(config.mirror_x);
            w.sr_mirror_y().bit(config.mirror_y)
        });

        writeback(input.data);
        // Maintain only rows touched by the destination rectangle, including boundary
        // cache lines. The entire output slice is exclusively borrowed, so adjacent
        // pixels in these lines cannot be modified concurrently and are preserved.
        let row_bytes = output.size.width * output.format.bytes_per_pixel();
        let first = output.offset.y * row_bytes + output.offset.x * output.format.bytes_per_pixel();
        let last = (output.offset.y + transformed.height - 1) * row_bytes
            + (output.offset.x + transformed.width) * output.format.bytes_per_pixel();
        let cache_start = first & !(ALIGNMENT - 1);
        let cache_end = last.next_multiple_of(ALIGNMENT);
        writeback(&output.data[cache_start..cache_end]);
        // The caller's stack may be in PSRAM.
        writeback(core::slice::from_ref(&tx));
        writeback(core::slice::from_ref(&rx));
        compiler_fence(Ordering::SeqCst);
        Reg(0x20).write(tx_address);
        Reg(0x520).write(rx_address);
        Reg(0x1c).write(1 << 21);
        Reg(0x51c).write(1 << 22);
        ppa.sr_scal_rotate()
            .modify(|_, w| w.scal_rotate_start().set_bit());

        let result = self.wait_for_completion();
        self.stop_channels();
        // Invalidate even after failure: DMA may have partially written the image.
        compiler_fence(Ordering::SeqCst);
        unsafe {
            crate::soc::cache_invalidate_addr(
                output.data.as_ptr() as u32 + cache_start as u32,
                (cache_end - cache_start) as u32,
            );
        }
        compiler_fence(Ordering::SeqCst);
        result
    }

    fn stop_channels(&mut self) {
        Reg(0x1c).modify(|v| v | (1 << 20));
        Reg(0x51c).modify(|v| v | (1 << 21));
        Reg(0x00).modify(|v| v | (1 << 25));
        Reg(0x500).modify(|v| v | (1 << 25));
        // Never return borrowed buffers while DMA could still access them.
        // Fail-stop if the bus cannot drain; a reset cannot safely release the borrow.
        let started = Instant::now();
        while Reg(0x24).read() & (1 << 24) == 0 || Reg(0x524).read() & (1 << 23) == 0 {
            assert!(
                started.elapsed().as_millis() < 1000,
                "PPA DMA2D did not drain"
            );
        }
    }

    fn wait_for_completion(&mut self) -> Result<(), Error> {
        let started = Instant::now();
        loop {
            let rx = Reg(0x504).read();
            let tx = Reg(0x04).read();
            let parameter = unsafe { &*pac::PPA::ptr() }.sr_param_err_st().read().bits();
            if rx & 0x3ffc != 0 || tx & 0x1ff4 != 0 || parameter != 0 {
                return Err(Error::Hardware);
            }
            if rx & (1 << 1) != 0 {
                return Ok(());
            }
            if started.elapsed().as_millis() >= 1000 {
                return Err(Error::Timeout);
            }
        }
    }
}

impl Drop for Srm {
    fn drop(&mut self) {
        self.stop_channels();
        critical_section::with(|_| {
            let clock = unsafe { &*pac::HP_SYS_CLKRST::ptr() };
            clock.soc_clk_ctrl1().modify(|_, w| {
                w.ppa_sys_clk_en().clear_bit();
                w.dma2d_sys_clk_en().clear_bit()
            });
        });
        TAKEN.store(false, Ordering::Release);
    }
}

// DMA2D channel 0 MMIO. P4 PAC 0.3 omits this controller entirely.
// Offsets/fields: ESP-IDF soc/esp32p4/register/hw_ver3/soc/dma2d_reg.h.
// Only an exclusively claimed Srm accesses these registers.
struct Reg(usize);
impl Reg {
    fn read(&self) -> u32 {
        unsafe { ((0x50088000 + self.0) as *const u32).read_volatile() }
    }
    fn write(&self, value: u32) {
        unsafe {
            ((0x50088000 + self.0) as *mut u32).write_volatile(value);
        }
    }
    fn modify(&self, f: impl FnOnce(u32) -> u32) {
        self.write(f(self.read()));
    }
}

fn writeback<T>(data: &[T]) {
    if is_slice_in_psram(data) || is_slice_in_dram(data) {
        let start = data.as_ptr() as usize & !(ALIGNMENT - 1);
        let end = (data.as_ptr_range().end as usize).next_multiple_of(ALIGNMENT);
        unsafe {
            crate::soc::cache_writeback_addr(start as u32, (end - start) as u32);
        }
    }
}

// Descriptor layout and pixel-size encoding:
// https://github.com/espressif/esp-idf/blob/ce2100de51a4d9a372f26578fd2ee763ca4b1279/components/esp_hal_dma/include/hal/dma2d_types.h#L22-L76
#[repr(C, align(128))]
struct Descriptor([u32; 6]);

impl Descriptor {
    fn new(
        buffer: *const u8,
        picture: Size,
        block: Size,
        offset: Offset,
        format: PixelFormat,
    ) -> Self {
        const DMA2D: u32 = 1 << 29;
        const EOF: u32 = 1 << 30;
        const OWNER: u32 = 1 << 31;
        Self([
            block.height as u32 | ((block.width as u32) << 14) | DMA2D | EOF | OWNER,
            picture.height as u32
                | ((picture.width as u32) << 14)
                | (format.dma_pixel_size() << 28),
            offset.y as u32 | ((offset.x as u32) << 14),
            buffer as u32,
            0,
            0,
        ])
    }
}
