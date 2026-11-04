//! Meeting recording (macOS only): the microphone and system audio are written
//! to two separate WAV files, so each can be transcribed on its own. The
//! microphone is "me" and system audio is "them".
//!
//! Dictation has its own recorder and state, and this module shares none of
//! them. Start and stop both halves together: a meeting is either fully
//! recording or not recording at all.

pub mod mic;
pub mod system_tap;
pub mod wav;

use anyhow::{bail, Context, Result};
use mic::{MicCapture, MicRecording};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use system_tap::{SystemAudioCapture, SystemRecording};
use tauri::{AppHandle, Manager};

use crate::managers::history::HistoryManager;
use crate::managers::transcription_jobs::TranscriptionJobManager;
use crate::notices::{self, Notice, NoticeCode, NoticeSource};

/// History source labels for each track. Stored with the entry, like file names.
const MIC_TRACK_LABEL: &str = "Meeting · Me";
const SYSTEM_TRACK_LABEL: &str = "Meeting · Them";
const MIC_TRACK: &str = "me";
const SYSTEM_TRACK: &str = "them";

/// Files produced by one meeting, both already finalized.
#[derive(Debug)]
pub struct MeetingRecording {
    pub meeting_id: String,
    pub mic: MicRecording,
    pub system: SystemRecording,
}

struct ActiveMeeting {
    meeting_id: String,
    // Stopped in `stop`. Dropping them ends the capture as well.
    system: SystemAudioCapture,
    mic: MicCapture,
}

#[derive(Default)]
pub struct MeetingManager {
    active: Mutex<Option<ActiveMeeting>>,
}

impl MeetingManager {
    /// Starts both halves, writing `<id>-system.wav` and `<id>-mic.wav` into
    /// `dir`. Returns the meeting id. If either half fails, the other is
    /// stopped and its file discarded.
    pub fn start(&self, dir: &Path) -> Result<String> {
        let mut active = self.lock();
        if active.is_some() {
            bail!("A meeting is already being recorded.");
        }
        std::fs::create_dir_all(dir)
            .with_context(|| format!("Creating {}", dir.display()))?;

        let meeting_id = format!("meeting-{}", unix_millis());
        // System audio first: it is the step that can fail on permission.
        let system = SystemAudioCapture::start(dir.join(format!("{meeting_id}-system.wav")))
            .context("Starting system audio capture")?;
        let mic = match MicCapture::start(&dir.join(format!("{meeting_id}-mic.wav"))) {
            Ok(mic) => mic,
            Err(e) => {
                let _ = system.stop();
                return Err(e).context("Starting the microphone");
            }
        };

        *active = Some(ActiveMeeting {
            meeting_id: meeting_id.clone(),
            system,
            mic,
        });
        Ok(meeting_id)
    }

    /// Stops both halves and finalizes their files. Both are stopped even when
    /// one fails; the first error is returned.
    pub fn stop(&self) -> Result<MeetingRecording> {
        let Some(meeting) = self.lock().take() else {
            bail!("No meeting is being recorded.");
        };
        let mic = meeting.mic.stop();
        let system = meeting.system.stop();
        Ok(MeetingRecording {
            meeting_id: meeting.meeting_id,
            mic: mic.context("Finishing the microphone recording")?,
            system: system.context("Finishing the system audio recording")?,
        })
    }

    pub fn is_recording(&self) -> bool {
        self.lock().is_some()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<ActiveMeeting>> {
        // A poisoned lock still holds valid state; keep recording rather than panic.
        self.active.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// Adds each non-empty track to History and queues it for transcription, the
/// microphone first. The track files stay where they were recorded (the
/// recordings folder), so deleting the history entry also deletes the audio.
pub fn queue_transcription(app: &AppHandle, recording: &MeetingRecording) -> Result<()> {
    let history = app.state::<Arc<HistoryManager>>();
    let jobs = app.state::<Arc<TranscriptionJobManager>>();

    let mut tracks: Vec<(PathBuf, &str, &str)> = Vec::new();
    if recording.mic.frames > 0 {
        tracks.push((recording.mic.path.clone(), MIC_TRACK_LABEL, MIC_TRACK));
    }
    match &recording.system.path {
        Some(path) if recording.system.frames > 0 => {
            tracks.push((path.clone(), SYSTEM_TRACK_LABEL, SYSTEM_TRACK));
        }
        _ => log::warn!(
            "Meeting {}: system audio track skipped (no frames captured)",
            recording.meeting_id
        ),
    }

    for (path, label, track) in tracks {
        let Some(file_name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            continue;
        };
        let entry = history.save_meeting_entry(
            file_name,
            label.to_string(),
            recording.meeting_id.clone(),
            track.to_string(),
        )?;
        jobs.enqueue(entry.id);
    }
    Ok(())
}

/// Starts a meeting in the recordings folder. Failures raise a notice and are
/// returned to the caller. Used by the command and the shortcut.
pub fn start_meeting(app: &AppHandle) -> std::result::Result<String, String> {
    let meetings = app.state::<Arc<MeetingManager>>();
    let history = app.state::<Arc<HistoryManager>>();
    match meetings.start(history.recordings_dir()) {
        Ok(meeting_id) => {
            notices::clear(app, NoticeSource::Meeting);
            Ok(meeting_id)
        }
        Err(e) => Err(raise_failure(app, NoticeCode::MeetingStartFailed, &e)),
    }
}

/// Stops the meeting, queues both tracks for transcription and returns the
/// meeting id. Failures raise a notice.
pub fn stop_meeting(app: &AppHandle) -> std::result::Result<String, String> {
    let meetings = app.state::<Arc<MeetingManager>>();
    let recording = match meetings.stop() {
        Ok(recording) => recording,
        Err(e) => return Err(raise_failure(app, NoticeCode::MeetingStopFailed, &e)),
    };
    let meeting_id = recording.meeting_id.clone();
    match queue_transcription(app, &recording) {
        Ok(()) => {
            notices::clear(app, NoticeSource::Meeting);
            Ok(meeting_id)
        }
        Err(e) => Err(raise_failure(app, NoticeCode::MeetingStopFailed, &e)),
    }
}

/// Shortcut entry point: starts a meeting if none is running, else stops it.
pub fn toggle_meeting(app: &AppHandle) {
    let active = app.state::<Arc<MeetingManager>>().is_recording();
    let result = if active {
        stop_meeting(app)
    } else {
        start_meeting(app)
    };
    if let Err(e) = result {
        log::warn!("Meeting toggle failed: {e}");
    }
}

fn raise_failure(app: &AppHandle, code: NoticeCode, error: &anyhow::Error) -> String {
    let detail = format!("{error:#}");
    notices::raise(
        app,
        Notice::new(code, NoticeSource::Meeting).with_detail(detail.clone()),
    );
    detail
}

fn unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}
