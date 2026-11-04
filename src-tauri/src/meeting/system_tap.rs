//! System-audio half of a meeting recording (macOS 14.2+ only).
//!
//! The Swift side (`swift/system_audio_tap.swift`) creates a Core Audio process
//! tap on the default output. Its IO callback runs on a real-time thread, so
//! the callback here only downmixes to mono and pushes into a lock-free ring
//! buffer. A separate writer thread drains the buffer into a WAV file.

use super::wav::MonoWavWriter;
use anyhow::{bail, Result};
use std::ffi::{c_char, c_void, CStr};
use std::path::PathBuf;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// About 10 seconds of mono audio at 48 kHz. Long enough to ride out a slow
/// disk without blocking the real-time thread.
const RING_CAPACITY: usize = 48_000 * 10;

mod ffi {
    use std::os::raw::{c_char, c_void};

    pub type TapCallback = extern "C" fn(*mut c_void, *const f32, usize, u32, f64);

    extern "C" {
        pub fn handy_tap_supported() -> bool;
        pub fn handy_tap_start(
            callback: TapCallback,
            user: *mut c_void,
            out_error: *mut *mut c_char,
        ) -> *mut c_void;
        pub fn handy_tap_stop(handle: *mut c_void);
        pub fn handy_tap_free_error(error: *mut c_char);
    }
}

/// State the IO thread writes into. Boxed so its address stays fixed while the
/// tap holds a pointer to it.
struct TapShared {
    producer: rtrb::Producer<f32>,
    /// Sample rate, set by the first callback.
    sample_rate: Arc<OnceLock<u32>>,
    dropped_samples: u64,
}

/// Called on Core Audio's IO thread. Must not block or allocate per call.
extern "C" fn on_samples(
    user: *mut c_void,
    samples: *const f32,
    frames: usize,
    channels: u32,
    sample_rate: f64,
) {
    if user.is_null() || samples.is_null() || channels == 0 {
        return;
    }
    // SAFETY: `user` points at the `TapShared` box owned by `SystemAudioCapture`.
    // The tap calls this only between `handy_tap_start` and `handy_tap_stop`,
    // and the box outlives both.
    let shared = unsafe { &mut *(user as *mut TapShared) };
    let _ = shared.sample_rate.set(sample_rate.round() as u32);

    let channels = channels as usize;
    // SAFETY: Swift guarantees `frames * channels` valid interleaved f32 samples.
    let input = unsafe { std::slice::from_raw_parts(samples, frames * channels) };
    for frame in input.chunks_exact(channels) {
        let mono = frame.iter().sum::<f32>() / channels as f32;
        if shared.producer.push(mono).is_err() {
            shared.dropped_samples += 1;
        }
    }
}

/// Drains the ring buffer into a mono WAV file until told to stop and empty.
fn write_loop(
    mut consumer: rtrb::Consumer<f32>,
    sample_rate: Arc<OnceLock<u32>>,
    path: PathBuf,
    stop: Arc<AtomicBool>,
) -> Result<Option<u64>> {
    // Wait for the first callback to tell us the rate. Give up if stopped first.
    let rate = loop {
        if let Some(rate) = sample_rate.get() {
            break *rate;
        }
        if stop.load(Ordering::Acquire) {
            // The tap ran but never called back: usually the system audio permission.
            log::warn!("System audio: no audio callbacks before stop; no track written");
            return Ok(None);
        }
        thread::sleep(Duration::from_millis(10));
    };
    log::info!("System audio: first callback at {rate} Hz, writing {}", path.display());

    let mut writer = MonoWavWriter::create(&path, rate)?;
    let mut buffer = vec![0f32; 4096];
    loop {
        let available = consumer.slots();
        if available == 0 {
            // The tap is stopped before `stop` is set, so once both hold, no more
            // samples can arrive.
            if stop.load(Ordering::Acquire) && consumer.slots() == 0 {
                break;
            }
            thread::sleep(Duration::from_millis(10));
            continue;
        }
        let n = available.min(buffer.len());
        for slot in buffer.iter_mut().take(n) {
            *slot = consumer.pop().unwrap_or(0.0);
        }
        writer.write_interleaved(&buffer[..n], 1)?;
    }
    Ok(Some(writer.finish()?))
}

/// Result of stopping the system-audio half.
#[derive(Debug)]
pub struct SystemRecording {
    /// `None` when the tap never delivered audio, so no file was written.
    pub path: Option<PathBuf>,
    pub frames: u64,
    pub sample_rate: Option<u32>,
    /// Samples lost because the writer fell behind.
    pub dropped_samples: u64,
}

pub struct SystemAudioCapture {
    handle: *mut c_void,
    /// Owned via `Box::into_raw`; freed only after the tap has stopped.
    shared: *mut TapShared,
    stop: Arc<AtomicBool>,
    sample_rate: Arc<OnceLock<u32>>,
    writer: Option<JoinHandle<Result<Option<u64>>>>,
    path: PathBuf,
}

// SAFETY: `handle` is an opaque Swift object used only through the C API, and
// its callback only touches `shared` while the tap is running. Methods take
// `&mut self` or consume `self`, and `stop` ends the tap before anything is
// dropped.
unsafe impl Send for SystemAudioCapture {}

/// True when this macOS can capture system audio (14.2+).
pub fn is_supported() -> bool {
    // SAFETY: a plain query with no arguments.
    unsafe { ffi::handy_tap_supported() }
}

impl SystemAudioCapture {
    /// Starts the tap and a writer thread that streams into `path`.
    /// Fails when the OS is too old or the system-audio permission is denied.
    pub fn start(path: PathBuf) -> Result<Self> {
        if !is_supported() {
            bail!("System audio capture needs macOS 14.2 or later.");
        }

        let (producer, consumer) = rtrb::RingBuffer::<f32>::new(RING_CAPACITY);
        let sample_rate = Arc::new(OnceLock::new());
        let shared = Box::into_raw(Box::new(TapShared {
            producer,
            sample_rate: Arc::clone(&sample_rate),
            dropped_samples: 0,
        }));

        let user = shared.cast::<c_void>();
        let mut error: *mut c_char = ptr::null_mut();
        // SAFETY: `user` stays valid until the tap is stopped and `shared` freed.
        let handle = unsafe { ffi::handy_tap_start(on_samples, user, &mut error) };
        if handle.is_null() {
            // SAFETY: the tap never started, so nothing else holds `shared`.
            unsafe { drop(Box::from_raw(shared)) };
            let message = take_error(error);
            log::warn!("System audio tap failed to start: {message}");
            bail!("{message}");
        }
        log::info!("System audio tap started");

        let stop = Arc::new(AtomicBool::new(false));
        let writer = {
            let stop = Arc::clone(&stop);
            let sample_rate = Arc::clone(&sample_rate);
            let path = path.clone();
            thread::Builder::new()
                .name("meeting-system-writer".into())
                .spawn(move || write_loop(consumer, sample_rate, path, stop))
        };
        let writer = match writer {
            Ok(writer) => writer,
            Err(e) => {
                // SAFETY: `handle` came from a successful `handy_tap_start`.
                unsafe {
                    ffi::handy_tap_stop(handle);
                    drop(Box::from_raw(shared));
                }
                bail!("Starting the system audio writer failed: {e}");
            }
        };

        Ok(Self {
            handle,
            shared,
            stop,
            sample_rate,
            writer: Some(writer),
            path,
        })
    }

    /// Stops the tap, drains the buffer and finalizes the file.
    pub fn stop(mut self) -> Result<SystemRecording> {
        // SAFETY: `handle` is valid until this call; the tap has no callbacks
        // once `handy_tap_stop` returns.
        unsafe { ffi::handy_tap_stop(self.handle) };
        self.handle = ptr::null_mut();
        self.stop.store(true, Ordering::Release);

        // SAFETY: the tap has stopped, so the callback no longer touches `shared`.
        let dropped_samples = unsafe { (*self.shared).dropped_samples };
        let writer = self.writer.take().expect("writer is joined only once");
        let frames = match writer.join() {
            Ok(result) => result?,
            Err(_) => bail!("The system audio writer thread panicked"),
        };
        self.free_shared();
        log::info!(
            "System audio stopped: {} frames, {} dropped samples",
            frames.unwrap_or(0),
            dropped_samples
        );

        Ok(SystemRecording {
            path: frames.map(|_| self.path.clone()),
            frames: frames.unwrap_or(0),
            sample_rate: self.sample_rate.get().copied(),
            dropped_samples,
        })
    }
}

impl SystemAudioCapture {
    /// Frees `shared`. Only call once the tap has stopped.
    fn free_shared(&mut self) {
        if !self.shared.is_null() {
            // SAFETY: created by `Box::into_raw` in `start`; the tap is stopped.
            unsafe { drop(Box::from_raw(self.shared)) };
            self.shared = ptr::null_mut();
        }
    }
}

impl Drop for SystemAudioCapture {
    fn drop(&mut self) {
        // Not stopped explicitly (for example, the app is shutting down). End
        // the tap before freeing `shared`, so the callback never sees freed
        // memory. The writer thread is left to finish on its own.
        if !self.handle.is_null() {
            // SAFETY: as in `stop`.
            unsafe { ffi::handy_tap_stop(self.handle) };
            self.handle = ptr::null_mut();
            self.stop.store(true, Ordering::Release);
        }
        self.free_shared();
    }
}

/// Reads and frees the Swift error string.
fn take_error(error: *mut c_char) -> String {
    if error.is_null() {
        return "System audio capture failed.".to_string();
    }
    // SAFETY: Swift returns a valid NUL-terminated string from `strdup`.
    let message = unsafe { CStr::from_ptr(error) }
        .to_string_lossy()
        .into_owned();
    // SAFETY: the string came from Swift and is freed exactly once.
    unsafe { ffi::handy_tap_free_error(error) };
    message
}
