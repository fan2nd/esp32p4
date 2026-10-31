//! Bounded, nonblocking logging for the dual-core desktop. Only core 0 drains UART.
use core::{
    cell::UnsafeCell,
    fmt::{self, Write},
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
};
use heapless::{Deque, String};

static ENABLED: AtomicBool = AtomicBool::new(false);
static DROPPED: AtomicU32 = AtomicU32::new(0);
struct Queue {
    busy: AtomicBool,
    records: UnsafeCell<Deque<String<256>, 32>>,
}
// Only a successful busy CAS grants access; a producer never waits on another CPU
// or an interrupted producer. UART output and formatting happen outside the lock.
unsafe impl Sync for Queue {}
static QUEUE: Queue = Queue {
    busy: AtomicBool::new(false),
    records: UnsafeCell::new(Deque::new()),
};
static LOGGER: Logger = Logger;
struct Logger;
impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Info
    }
    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            write(format_args!("{} - {}", record.level(), record.args()));
        }
    }
    fn flush(&self) {}
}

/// Call on core 0 before starting the secondary core or any application tasks.
pub fn init() {
    log::set_logger(&LOGGER).unwrap();
    log::set_max_level(log::LevelFilter::Info);
    ENABLED.store(true, Ordering::Release);
}

pub fn write(args: fmt::Arguments<'_>) {
    if !ENABLED.load(Ordering::Acquire) {
        // Preserve standalone binaries that use display::init without this logger.
        esp_println::println!("{}", args);
        return;
    }
    let mut line = String::<256>::new();
    let _ = write!(line, "[{:?}] {}", esp_hal::system::Cpu::current(), args);
    if QUEUE
        .busy
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        DROPPED.fetch_add(1, Ordering::Relaxed);
        return;
    }
    let full = unsafe { (&mut *QUEUE.records.get()).push_back(line).is_err() };
    QUEUE.busy.store(false, Ordering::Release);
    if full {
        DROPPED.fetch_add(1, Ordering::Relaxed);
    }
}

#[macro_export]
macro_rules! diagnostic {
    ($($arg:tt)*) => { $crate::diagnostics::write(core::format_args!($($arg)*)) };
}

#[embassy_executor::task]
pub async fn run() {
    assert_eq!(
        esp_hal::system::Cpu::current(),
        esp_hal::system::Cpu::ProCpu
    );
    loop {
        let record = if QUEUE
            .busy
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            let record = unsafe { (&mut *QUEUE.records.get()).pop_front() };
            QUEUE.busy.store(false, Ordering::Release);
            record
        } else {
            None
        };
        if let Some(record) = record {
            // A full synchronous UART line can starve USB audio on this
            // executor. Yield between <=4-byte UTF-8-safe segments: a 16-byte
            // write already exceeds the USB audio endpoint's 1 ms deadline.
            let mut remaining = record.as_str();
            while !remaining.is_empty() {
                let mut end = remaining.len().min(4);
                while !remaining.is_char_boundary(end) {
                    end -= 1;
                }
                esp_println::print!("{}", &remaining[..end]);
                remaining = &remaining[end..];
                embassy_futures::yield_now().await;
            }
            esp_println::println!();
        }
        let dropped = DROPPED.swap(0, Ordering::Relaxed);
        if dropped != 0 {
            esp_println::println!("log: dropped {} records", dropped);
        }
        // Yield after each UART line so audio/USB and the radio runner can progress.
        embassy_futures::yield_now().await;
    }
}
