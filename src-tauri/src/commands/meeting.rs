//! Tauri commands for meeting recording. Meeting recording is macOS only. On
//! other platforms the same commands exist (so the generated bindings match)
//! and report that the feature is unavailable.

#[cfg(target_os = "macos")]
use crate::meeting::{self, MeetingManager};
#[cfg(target_os = "macos")]
use std::sync::Arc;
#[cfg(target_os = "macos")]
use tauri::{AppHandle, State};

#[cfg(target_os = "macos")]
#[tauri::command]
#[specta::specta]
pub fn start_meeting_recording(app: AppHandle) -> Result<String, String> {
    meeting::start_meeting(&app)
}

/// Stops the meeting, adds both tracks to History and queues them for
/// transcription. Returns the meeting id.
#[cfg(target_os = "macos")]
#[tauri::command]
#[specta::specta]
pub fn stop_meeting_recording(app: AppHandle) -> Result<String, String> {
    meeting::stop_meeting(&app)
}

#[cfg(target_os = "macos")]
#[tauri::command]
#[specta::specta]
pub fn is_meeting_recording(meetings: State<'_, Arc<MeetingManager>>) -> bool {
    meetings.is_recording()
}

#[cfg(not(target_os = "macos"))]
const UNAVAILABLE: &str = "Meeting recording is only available on macOS.";

#[cfg(not(target_os = "macos"))]
#[tauri::command]
#[specta::specta]
pub fn start_meeting_recording() -> Result<String, String> {
    Err(UNAVAILABLE.to_string())
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
#[specta::specta]
pub fn stop_meeting_recording() -> Result<String, String> {
    Err(UNAVAILABLE.to_string())
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
#[specta::specta]
pub fn is_meeting_recording() -> bool {
    false
}
