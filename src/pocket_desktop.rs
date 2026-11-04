//! MicroTS view adapter. Existing page state owns input and business actions;
//! PocketJS owns control layout/rasterization. Images, icons and CJK overlays
//! keep the same asset path as the existing desktop.
use alloc::{string::ToString, vec, vec::Vec};
use embedded_graphics::{pixelcolor::Rgb565, prelude::*, primitives::Rectangle};
use microts::{Input, Ui};
use pocket_drivers::DamageRect;
use tiny_ui::{Scene, Widget};

#[path = "../assets/ui/pocket/gen/mod.rs"]
mod generated;
use generated::{Color, DesktopApp, DesktopProps, DesktopViewModel, Item, Px};

pub const HEAP_SIZE: usize = 4 * 1024 * 1024;
#[cfg(all(target_os = "none", not(feature = "slint-ui")))]
#[global_allocator]
static HEAP: linked_list_allocator::LockedHeap = linked_list_allocator::LockedHeap::empty();

/// Called once on core 1, after PSRAM initialization and before any allocation.
#[cfg(target_os = "none")]
pub unsafe fn init_heap(ptr: *mut u8) {
    #[cfg(feature = "slint-ui")]
    unsafe {
        crate::slint_backend::HEAP.lock().init(ptr, HEAP_SIZE);
    }
    #[cfg(not(feature = "slint-ui"))]
    unsafe {
        HEAP.lock().init(ptr, HEAP_SIZE);
    }
}

#[derive(Default)]
struct Model {
    items: Vec<Item>,
}
impl DesktopViewModel for Model {
    fn items(&self) -> &[Item] {
        &self.items
    }
}

pub struct Painter {
    app: DesktopApp<Model>,
    pixels: Vec<u16>,
    words: Vec<u32>,
}
fn color(c: Rgb565) -> Color {
    let r = u32::from(c.r()) * 255 / 31;
    let g = u32::from(c.g()) * 255 / 63;
    let b = u32::from(c.b()) * 255 / 31;
    Color(0xff000000 | b << 16 | g << 8 | r)
}
impl Painter {
    pub fn new() -> Self {
        let mut ui = Ui::new();
        assert!(ui.load_styles(include_bytes!("../assets/ui/pocket/gen/styles.bin")));
        ui.core_mut().set_viewport(360., 640.);
        assert!(
            ui.core_mut()
                .load_font_atlas(include_bytes!("../assets/ui/pocket/inter16.bin"))
        );
        Self {
            app: DesktopApp::new(ui, DesktopProps {}, Model::default()),
            pixels: vec![0; 360 * 640],
            words: Vec::new(),
        }
    }
    pub fn render<const N: usize, D: DrawTarget<Color = Rgb565>>(
        &mut self,
        scene: &Scene<N>,
        target: &mut D,
        damage: Rectangle,
    ) -> Result<(), D::Error> {
        // Stable IDs let MicroTS retain nodes across presses. Reuse allocation
        // capacity; the host never sends passwords through this control model.
        let items = &mut self.app.model.items;
        items.clear();
        for widget in scene.widgets() {
            let base = widget.base();
            if !base.visible {
                continue;
            }
            let b = base.bounds;
            let style = base.style;
            let id = i32::from(base.id) * 8;
            let (label, background) = match widget {
                Widget::Button(button) => (
                    button.text,
                    if button.is_pressed() {
                        style.pressed
                    } else {
                        style.background
                    },
                ),
                _ => ("", style.background),
            };
            items.push(Item {
                id,
                x: Px(b.top_left.x as f32),
                y: Px(b.top_left.y as f32),
                width: Px(b.size.width as f32),
                height: Px(b.size.height as f32),
                label: label.to_string(),
                background: color(background),
                border: color(style.border),
                stroke: Px(style.border_width as f32),
                radius: Px(style.radius as f32 / 2.),
            });
            match widget {
                Widget::Slider(slider) => {
                    let inset = 6.;
                    let travel = b.size.width.saturating_sub(13) as f32;
                    let lo = slider.min.min(slider.max);
                    let hi = slider.min.max(slider.max);
                    let fraction = if hi == lo {
                        0.
                    } else {
                        (slider.value.clamp(lo, hi) - lo) as f32 / (hi - lo) as f32
                    };
                    let x = b.top_left.x as f32 + inset;
                    let y = b.top_left.y as f32 + b.size.height as f32 / 2.;
                    for (part, px, py, w, h, fill, radius) in [
                        (1, x, y - 2., travel + 1., 4., style.border, 0.),
                        (2, x, y - 2., travel * fraction, 4., style.accent, 0.),
                        (
                            3,
                            x + travel * fraction - 6.,
                            y - 6.,
                            12.,
                            12.,
                            style.accent,
                            6.,
                        ),
                    ] {
                        items.push(Item {
                            id: id + part,
                            x: Px(px),
                            y: Px(py),
                            width: Px(w),
                            height: Px(h),
                            label: alloc::string::String::new(),
                            background: color(fill),
                            border: color(fill),
                            stroke: Px(0.),
                            radius: Px(radius),
                        });
                    }
                }
                Widget::ListView(list) if list.max_scroll() != 0 => {
                    let width = style.border_width.max(4).min(b.size.width / 2);
                    let inset = style.radius.div_ceil(2).max(width).min(b.size.height / 2);
                    let height = b.size.height - 2 * inset;
                    let total = b.size.height as u64 + list.max_scroll() as u64;
                    let thumb = ((height as u64 * b.size.height as u64 / total) as u32)
                        .max(18)
                        .min(height);
                    let offset = (u64::from(height - thumb)
                        * u64::from(list.scroll.min(list.max_scroll()))
                        / u64::from(list.max_scroll())) as u32;
                    let fill = Rgb565::new(
                        ((u16::from(style.border.r()) + u16::from(style.accent.r())) / 2) as u8,
                        ((u16::from(style.border.g()) + u16::from(style.accent.g())) / 2) as u8,
                        ((u16::from(style.border.b()) + u16::from(style.accent.b())) / 2) as u8,
                    );
                    items.push(Item {
                        id: id + 1,
                        x: Px(b.top_left.x as f32 + (b.size.width - width) as f32),
                        y: Px(b.top_left.y as f32 + (inset + offset) as f32),
                        width: Px(width as f32),
                        height: Px(thumb as f32),
                        label: alloc::string::String::new(),
                        background: color(fill),
                        border: color(fill),
                        stroke: Px(0.),
                        radius: Px(width as f32 / 2.),
                    });
                }
                _ => {}
            }
        }
        self.app.invalidate();
        self.app.frame(&Input::default());
        let core = self.app.host.core_mut();
        self.words.clear();
        self.words.extend_from_slice(&core.draw().words);
        let end = damage.top_left + damage.size;
        let region = DamageRect::new(damage.top_left.x, damage.top_left.y, end.x, end.y);
        pocket_raster::render_scaled_rgb565_regions(
            &*core,
            &self.words,
            &mut self.pixels,
            1,
            &[region],
        );
        // Only changed spans cross into scanout. Panel performs aligned 32-bit
        // stores and 2x expansion, preserving the established PSRAM write path.
        for y in damage.top_left.y..end.y {
            let row = Rectangle::new(
                Point::new(damage.top_left.x, y),
                Size::new(damage.size.width, 1),
            );
            let start = y as usize * 360 + damage.top_left.x as usize;
            target.fill_contiguous(
                &row,
                self.pixels[start..start + damage.size.width as usize]
                    .iter()
                    .map(|&p| Rgb565::new((p >> 11) as u8, ((p >> 5) & 63) as u8, (p & 31) as u8)),
            )?;
        }
        Ok(())
    }
}
