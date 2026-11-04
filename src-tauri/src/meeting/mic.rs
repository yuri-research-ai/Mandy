//! Microphone half of a meeting recording.
//!
//! This uses its own `handy_recorder::Recorder`, separate from the one dictation
//! uses, so starting or stopping a meeting never touches dictation's state.
//! Audio is written straight to disk through `MonoWavWriter`.
//!
//! When the microphone is lost (unplugged, sleep, ...) the meeting keeps going:
//! a supervisor thread moves capture to the built-in microphone, and moves back
//! to the preferred microphone once it returns. The file is the same throughout;
//! the audio simply has a short gap where the device changed.

use super::wav::MonoWavWriter;
use anyhow::{anyhow, Context, Result};
use handy_recorder::{AudioChunk, InputDevice, Recorder, RecorderConfig, Sink};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

/// How often the supervisor checks for a lost device or a returned preferred one.
const SUPERVISE_TICK: Duration = Duration::from_secs(1);

/// Receives microphone chunks from handy-recorder and writes them to disk.
/// Runs on the recorder's delivery thread, so it only does file I/O.
pub struct MicSink {
    writer: Option<MonoWavWriter>,
    error: Option<String>,
}

impl Sink for MicSink {
    fn process_chunk(&mut self, chunk: AudioChunk<'_>) {
        let Some(writer) = self.writer.as_mut() else {
            return;
        };
        let real = chunk.valid_frames * chunk.channels as usize;
        if let Err(err) = writer.write_interleaved(&chunk.samples[..real], chunk.channels as usize)
        {
            // Stop writing, keep the error for `stop` to report.
            self.error.get_or_insert_with(|| format!("{err:#}"));
            self.writer = None;
        }
    }
}

/// Result of stopping the microphone half.
#[derive(Debug)]
pub struct MicRecording {
    pub path: PathBuf,
    pub frames: u64,
    pub sample_rate: u32,
    /// Frames lost because the sink fell behind (usually disk pressure).
    pub dropped_frames: u64,
    /// The microphone failed or the writer hit an error while recording.
    pub error: Option<String>,
}

/// State shared between the meeting, its supervisor thread and failure handlers.
struct Inner {
    /// The device currently open, if any. `None` between two devices.
    recorder: Option<Recorder<MicSink>>,
    /// The writer, held here only while no recorder owns it.
    writer: Option<MonoWavWriter>,
    /// Name of the device `recorder` is open on.
    device_name: Option<String>,
    /// Errors from recorders that ended, reported by `stop` if nothing else is.
    error: Option<String>,
    dropped_frames: u64,
}

/// What a failure handler needs to report a lost device.
#[derive(Clone)]
struct Hooks {
    tx: Sender<()>,
    /// Sticky: the microphone has failed at least once in this meeting.
    failed: Arc<AtomicBool>,
    /// Set on each loss and cleared by the supervisor when it reacts.
    lost: Arc<AtomicBool>,
}

pub struct MicCapture {
    inner: Arc<Mutex<Inner>>,
    path: PathBuf,
    sample_rate: u32,
    hooks: Hooks,
    stop_flag: Arc<AtomicBool>,
    supervisor: Option<JoinHandle<()>>,
}

impl MicCapture {
    /// Opens the default microphone and starts writing to `path`. The default
    /// microphone at this moment is the preferred one; the built-in microphone
    /// is used whenever it is unavailable.
    pub fn start(path: &Path) -> Result<Self> {
        let devices = handy_recorder::list_input_devices().unwrap_or_default();
        let preferred_name = devices.iter().find(|d| d.is_default).map(|d| d.name.clone());
        let device = pick_device(&devices, preferred_name.as_deref());

        let (tx, rx) = mpsc::channel();
        let hooks = Hooks {
            tx: tx.clone(),
            failed: Arc::new(AtomicBool::new(false)),
            lost: Arc::new(AtomicBool::new(false)),
        };

        let recorder = open_on(device.as_ref(), &hooks)?;
        let sample_rate = recorder.info().format.sample_rate;
        let writer = MonoWavWriter::create(path, sample_rate)?;
        recorder
            .start(MicSink {
                writer: Some(writer),
                error: None,
            })
            .map_err(|e| anyhow!("Starting the microphone failed: {}", e.error))?;

        let inner = Arc::new(Mutex::new(Inner {
            recorder: Some(recorder),
            writer: None,
            device_name: device.as_ref().map(|d| d.name.clone()),
            error: None,
            dropped_frames: 0,
        }));
        let stop_flag = Arc::new(AtomicBool::new(false));

        let supervisor = Supervisor {
            inner: Arc::clone(&inner),
            stop_flag: Arc::clone(&stop_flag),
            hooks: hooks.clone(),
            preferred_name,
        };
        let handle = std::thread::Builder::new()
            .name("meeting-mic-supervisor".into())
            .spawn(move || supervisor.run(rx))
            .context("Starting the microphone supervisor")?;

        Ok(Self {
            inner,
            path: path.to_path_buf(),
            sample_rate,
            hooks: Hooks { tx, ..hooks },
            stop_flag,
            supervisor: Some(handle),
        })
    }

    /// True once the microphone has failed at least once (unplugged, permission revoked, ...).
    pub fn has_failed(&self) -> bool {
        self.hooks.failed.load(Ordering::Acquire)
    }

    /// Stops the recording, finalizes the file and closes the microphone.
    pub fn stop(mut self) -> Result<MicRecording> {
        self.shutdown_supervisor();
        let mut inner = lock(&self.inner);

        let mut error = inner.error.take();
        if let Some(recorder) = inner.recorder.take() {
            let stopped = recorder.stop();
            // Closing the recorder releases the device. Errors here don't lose audio.
            if let Err(e) = recorder.close() {
                log::warn!("Closing the meeting microphone failed: {e}");
            }
            let stopped = stopped.map_err(|e| anyhow!("Stopping the microphone failed: {e}"))?;
            inner.dropped_frames += stopped.dropped_frames;
            if !matches!(stopped.end_reason, handy_recorder::EndReason::StopCalled) {
                error.get_or_insert(format!("{:?}", stopped.end_reason));
            }
            let mut sink = stopped.sink;
            if let Some(e) = sink.error.take() {
                error.get_or_insert(e);
            }
            inner.writer = sink.writer.take();
        }

        let frames = match inner.writer.take() {
            Some(writer) => writer.finish().context("Finalizing the microphone recording")?,
            // No writer: the file was never created, or the last switch lost it.
            None => 0,
        };

        Ok(MicRecording {
            path: self.path.clone(),
            frames,
            sample_rate: self.sample_rate,
            dropped_frames: inner.dropped_frames,
            error,
        })
    }

    /// Ends the supervisor thread, if still running. Safe to call twice.
    fn shutdown_supervisor(&mut self) {
        self.stop_flag.store(true, Ordering::Release);
        let _ = self.hooks.tx.send(());
        if let Some(handle) = self.supervisor.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for MicCapture {
    fn drop(&mut self) {
        self.shutdown_supervisor();
    }
}

struct Supervisor {
    inner: Arc<Mutex<Inner>>,
    stop_flag: Arc<AtomicBool>,
    hooks: Hooks,
    preferred_name: Option<String>,
}

impl Supervisor {
    fn run(self, rx: Receiver<()>) {
        loop {
            match rx.recv_timeout(SUPERVISE_TICK) {
                Ok(()) | Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            if self.stop_flag.load(Ordering::Acquire) {
                break;
            }
            self.reconcile();
        }
    }

    /// Makes the open device match the one that should be used right now.
    fn reconcile(&self) {
        let lost = self.hooks.lost.swap(false, Ordering::AcqRel);
        let devices = match handy_recorder::list_input_devices() {
            Ok(devices) => devices,
            Err(e) => {
                log::warn!("Listing microphones failed: {e}");
                return;
            }
        };
        let Some(target) = pick_device(&devices, self.preferred_name.as_deref()) else {
            // No microphone at all right now: keep waiting for one.
            return;
        };

        let mut inner = lock(&self.inner);
        let is_open = inner.recorder.is_some();
        if !lost && is_open && inner.device_name.as_deref() == Some(target.name.as_str()) {
            return;
        }
        self.switch_to(&mut inner, &target);
    }

    fn switch_to(&self, inner: &mut Inner, target: &InputDevice) {
        // Release the current recorder (if any) and take back its writer.
        if let Some(recorder) = inner.recorder.take() {
            let stopped = recorder.stop();
            if let Err(e) = recorder.close() {
                log::warn!("Closing the lost microphone failed: {e}");
            }
            match stopped {
                Ok(stopped) => {
                    inner.dropped_frames += stopped.dropped_frames;
                    let mut sink = stopped.sink;
                    if let Some(e) = sink.error.take() {
                        inner.error.get_or_insert(e);
                    }
                    inner.writer = sink.writer.take();
                }
                Err(e) => {
                    // The writer is gone with the recorder; this track stops here.
                    log::warn!("Stopping the lost microphone failed: {e}");
                    inner.error.get_or_insert(format!("{e}"));
                }
            }
        }
        inner.device_name = None;

        let recorder = match open_on(Some(target), &self.hooks) {
            Ok(recorder) => recorder,
            Err(e) => {
                // Keep the writer and try again on the next tick.
                log::warn!("Opening microphone {} failed, retrying: {e:#}", target.name);
                return;
            }
        };
        let Some(writer) = inner.writer.take() else {
            log::warn!("No audio file to continue on microphone {}", target.name);
            let _ = recorder.close();
            return;
        };
        match recorder.start(MicSink {
            writer: Some(writer),
            error: None,
        }) {
            Ok(()) => {
                log::info!("Meeting microphone is now {}", target.name);
                inner.device_name = Some(target.name.clone());
                inner.recorder = Some(recorder);
            }
            Err(e) => {
                log::warn!("Starting microphone {} failed, retrying: {}", target.name, e.error);
                inner.writer = e.sink.writer;
                let _ = recorder.close();
            }
        }
    }
}

/// The microphone to use: the preferred one if present, else the built-in one,
/// else the system default.
fn pick_device(devices: &[InputDevice], preferred_name: Option<&str>) -> Option<InputDevice> {
    preferred_name
        .and_then(|name| devices.iter().find(|d| d.name == name))
        .or_else(|| devices.iter().find(|d| is_built_in(&d.name)))
        .or_else(|| devices.iter().find(|d| d.is_default))
        .cloned()
}

fn is_built_in(name: &str) -> bool {
    name.contains("MacBook") || name.contains("Built-in")
}

fn open_on(device: Option<&InputDevice>, hooks: &Hooks) -> Result<Recorder<MicSink>> {
    let mut config = RecorderConfig::speech();
    config.device = device.map(|d| d.id.clone());
    let hooks = hooks.clone();
    Recorder::<MicSink>::open_with_failure_handler(config, move |err| {
        log::warn!("Meeting microphone failed: {err}");
        hooks.failed.store(true, Ordering::Release);
        hooks.lost.store(true, Ordering::Release);
        let _ = hooks.tx.send(());
    })
    .map_err(|e| anyhow!("Opening the microphone failed: {e}"))
}

/// The state is still valid after a panic elsewhere, so keep going.
fn lock(inner: &Mutex<Inner>) -> MutexGuard<'_, Inner> {
    inner.lock().unwrap_or_else(|e| e.into_inner())
}
