<p align="center">
  <img src="src-tauri/icons/logo.png" alt="Mandy" width="128" height="128">
</p>

<h1 align="center">Mandy</h1>

<p align="center">
  <strong>Offline speech-to-text for dictation and meetings. Nothing leaves your computer.</strong>
</p>

Mandy is a desktop app that turns speech into text locally. Hold a shortcut and talk, and the words land in whatever text field has focus. On macOS it can also record a meeting: your microphone and the sound coming out of your speakers are captured as two separate tracks and transcribed on their own, so you get a clean "Me" / "Them" transcript of a Slack, Zoom or Google Meet call.

Mandy is a fork of [Handy](https://github.com/cjpais/Handy) by CJ Pais. Everything Handy does, Mandy does too. This README covers what is different and what you need to get going. For the long tail of platform notes and troubleshooting, the [upstream README](https://github.com/cjpais/Handy#readme) still applies.

## What's in the box

- **Dictation.** Press a shortcut, speak, release. Silence is trimmed with Silero VAD, the audio goes through Whisper or Parakeet on your GPU/CPU, and the text is pasted into the active app.
- **Meeting recording (macOS 14.2+).** One shortcut starts and stops a two-track recording. Each track is transcribed separately and shown side by side on the **Meetings** page.
- **Local models.** Whisper (Small, Medium, Turbo, Large) with Metal, Vulkan or CPU acceleration, and Parakeet for fast CPU transcription with automatic language detection. Models are downloaded once and stored on disk.
- **History, dictionary, post-processing.** Every dictation is kept with its audio. Custom words improve recognition, and an optional LLM step can rewrite text before it is pasted.
- **Cross-platform.** macOS, Windows and Linux for dictation. Meeting recording is macOS only, because it relies on Core Audio process taps.

## Meeting recording

### How it works

1. Press the meeting shortcut (default **⌥⇧M** on macOS) or click **Start recording** on the Meetings page.
2. Mandy opens two captures at once: your microphone, and a system-wide audio tap on the default output device. Mandy's own feedback sounds are excluded from the tap.
3. Both streams are written straight to disk as mono 16-bit FLAC. Memory stays flat no matter how long the meeting runs, and silent stretches cost almost nothing on disk.
4. Press the shortcut again to stop. Both tracks are added to History and queued for transcription, microphone first.
5. The Meetings page groups the two tracks under one timestamp. **Copy meeting** puts both transcripts on the clipboard, labelled "Me:" and "Them:". **Delete meeting** removes both entries and their audio.

A meeting is either fully recording or not recording at all. If either half fails to start, the other is stopped and its file discarded.

### What happens when the microphone disappears

Unplugging a headset or waking from sleep does not end the meeting. A supervisor thread moves capture to the built-in microphone and moves it back to the preferred one once it returns. The track stays one file with a short gap where the device changed.

### Permissions

Meeting recording needs two permissions, both granted in **System Settings > Privacy & Security**:

- **Microphone**
- **System Audio Recording** (under "Screen & System Audio Recording")

macOS prompts for them the first time you start a meeting. If a meeting refuses to start, the notice in the footer and tray tells you which one is missing. System audio capture requires macOS 14.2 or later. On older systems the Meetings page is still there, but recording fails with an explanation.

Tell everyone in the meeting that you are recording.

### Where the files live

Recordings sit next to dictation audio in the app data folder:

```
~/Library/Application Support/com.yuri-research-ai.mandy/recordings/
├── meeting-<unix-ms>-mic.flac      # Me
└── meeting-<unix-ms>-system.flac   # Them
```

A file with a `.partial` suffix is still being written. Once a track is finalized it loses the suffix, so any file without it is complete. Deleting a meeting from the Meetings page removes both files.

### Dictation during a meeting

Dictation keeps working while a meeting is recording. Normally dictation mutes system audio while you hold the shortcut; during a meeting it leaves the audio alone so the "Them" track stays intact.

## Installation

Mandy has no update feed yet, so there is no in-app updater. The quickest way to install or update is the script in the repository root:

```bash
curl -fsSL https://raw.githubusercontent.com/yuri-research-ai/Mandy/mandy/install.sh | bash
```

It downloads the DMG for your Mac, checks its SHA-256 against the value in the script, copies `Mandy.app` to `/Applications`, strips the quarantine flag and, on an update, clears the stale Accessibility grant so Mandy asks for it again. No sudo. Set `MANDY_INSTALL_DIR` to install somewhere else. Read the script first if you prefer; it is short.

Builds are also committed to the repository in the [`releases/`](releases/) folder, one `.dmg` per version and architecture, for a manual install. Take the newest one for your Mac, or build it yourself (see below).

| Mac                       | File                                   |
| ------------------------- | -------------------------------------- |
| Apple Silicon (M1 and up) | `releases/Mandy_<version>_aarch64.dmg` |
| Intel                     | `releases/Mandy_<version>_x64.dmg`     |

Manual install:

1. Open the `.dmg` and drag **Mandy** to Applications.
2. The build is ad-hoc signed and not notarized, so the first launch stops with "Apple could not verify Mandy is free of malware". Click **Done**, open **System Settings > Privacy & Security**, scroll down to the message about Mandy and click **Open Anyway**. You only need to do this once. The equivalent in a terminal, after the app is in Applications:

   ```bash
   xattr -d com.apple.quarantine /Applications/Mandy.app
   ```

   On macOS 14 and earlier, right-clicking the app and choosing **Open** works as well.

3. Grant **Accessibility** (for the global shortcut and pasting) and **Microphone** when asked. Grant **System Audio Recording** the first time you record a meeting.
4. Pick a model on the Models page. Parakeet is the fastest choice on Apple Silicon; Whisper Turbo gives the best quality for non-English speech.
5. Set your shortcuts on the General page. Defaults:

| Action            | macOS  | Windows      | Linux          |
| ----------------- | ------ | ------------ | -------------- |
| Dictation         | ⌥Space | Ctrl+Space   | Ctrl+Alt+Space |
| Meeting recording | ⌥⇧M    | Ctrl+Shift+M | Ctrl+Shift+M   |

Windows and Linux builds work for dictation but have not been exercised with this fork. The Meetings page is hidden on those platforms.

### Updating to a new version

Every build is ad-hoc signed, so each version has a different code identity as far as macOS is concerned. Permissions are tied to that identity, so after you replace `Mandy.app` with a newer version, **Accessibility**, **Microphone** and **System Audio Recording** have to be granted again. Accessibility is the one that misleads: the old entry in **System Settings > Privacy & Security > Accessibility** still shows as enabled, but it no longer covers the new build. The shortcut stays on "Waiting..." and nothing is pasted.

The install script does this for you. By hand, after installing the new version:

1. Quit Mandy.
2. Open **System Settings > Privacy & Security > Accessibility**, select **Mandy** and remove it with the **−** button.
3. Launch Mandy and grant Accessibility again when it asks.
4. Approve Microphone when you first dictate, and System Audio Recording when you first record a meeting. macOS prompts for both again.

The same from a terminal:

```bash
osascript -e 'tell application id "com.yuri-research-ai.mandy" to quit' || true
tccutil reset Accessibility com.yuri-research-ai.mandy
open /Applications/Mandy.app
```

Settings, history and downloaded models survive the update.

## Building from source

### Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- [Bun](https://bun.sh/)
- **macOS:** Xcode Command Line Tools with a macOS 14.2 SDK or newer. The build compiles a small Swift bridge for system audio taps. If the SDK lacks `CATapDescription.h`, the build still succeeds but prints a warning and swaps in a stub, and meeting recording reports itself as unavailable.
- **Linux / Windows:** see [BUILD.md](BUILD.md) for the platform toolchains (Vulkan SDK, OpenBLAS, gtk-layer-shell).

### Steps

```bash
bun install

# The VAD model ships with the app and is not in git.
mkdir -p src-tauri/resources/models
curl -L -o src-tauri/resources/models/silero-vad-v6.2-F32.gguf \
  https://huggingface.co/handy-computer/silero-vad-v6.2-gguf/resolve/main/silero-vad-v6.2-F32.gguf

# Run in development
bun run tauri dev
# If cmake complains on macOS:
CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri dev

# Build the release bundle
bun run tauri build
```

The macOS bundle ends up in `src-tauri/target/release/bundle/`: `macos/Mandy.app` and `dmg/Mandy_<version>_aarch64.dmg`. For an Intel build from an Apple Silicon Mac, add the target once and build for it:

```bash
rustup target add x86_64-apple-darwin
bun run tauri build --target x86_64-apple-darwin
```

The result lands in `src-tauri/target/x86_64-apple-darwin/release/bundle/` as `Mandy_<version>_x64.dmg`. The Swift bridge for system audio is compiled without the Swift runtime compatibility archives, because the Command Line Tools ship them for arm64 only.

Before committing, run the linters and formatters:

```bash
bun run lint
bun run format
bun run check:translations
```

### Versioning

Mandy's version is Handy's base version plus a build suffix: `0.10.0+m1`, `0.10.0+m2`, and so on. The suffix goes up by one for every delivered change and is never reused. It has to be identical in four places: `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json` and the `handy` package entry in `src-tauri/Cargo.lock`. The DMG file name follows it.

## Controlling Mandy from outside

The binary inside the bundle is still called `handy`. All of Handy's CLI flags work:

```bash
MANDY=/Applications/Mandy.app/Contents/MacOS/handy

$MANDY --toggle-transcription   # start or stop dictation
$MANDY --toggle-post-process    # dictation with LLM post-processing
$MANDY --cancel                 # cancel the current operation
$MANDY --start-hidden           # launch to the tray without a window
$MANDY --no-tray                # closing the window quits the app
$MANDY --debug                  # trace-level logging
```

The remote-control flags talk to the running instance and exit. There is no CLI flag for meeting recording yet. Use the shortcut or the Meetings page.

**Transcription hook.** An executable at `hooks/transcription` inside the app data folder can rewrite dictated text right before it is pasted. Mandy writes the text to its stdin and pastes its stdout. See the [upstream description](https://github.com/cjpais/Handy#transcription-hook) for details and an example. The folder is `~/Library/Application Support/com.yuri-research-ai.mandy/` on macOS and `~/.local/share/com.yuri-research-ai.mandy/` on Linux.

**Debug menu.** `Cmd+Shift+D` (macOS) or `Ctrl+Shift+D` (Windows/Linux) reveals the Debug page with paste delays, the app data path and log level.

## Differences from Handy

| Area              | Handy                           | Mandy                                                                 |
| ----------------- | ------------------------------- | --------------------------------------------------------------------- |
| Branding          | Handy name, hand mascot         | Mandy name, new mark and wordmark, new icons                          |
| Bundle identifier | `com.pais.handy`                | `com.yuri-research-ai.mandy` (separate settings, history and models)  |
| Meeting recording | none                            | Two-track FLAC recording with per-track transcription (macOS 14.2+)   |
| Shortcuts         | dictation                       | dictation plus a `meeting` binding (⌥⇧M)                              |
| History schema    | one entry per dictation         | entries carry `meeting_id` and `meeting_track` so tracks stay grouped |
| Updater           | signed releases, in-app updates | disabled: no feed, no signing key, updater artifacts are not built    |
| Version           | `0.10.0`                        | `0.10.0+mN`                                                           |

Because the bundle identifier changed, Mandy does not see settings or history from an existing Handy installation. Both apps can be installed side by side. Models are not shared either, except for GGUF files that already sit in the Hugging Face cache at `~/.cache/huggingface/hub`.

## Troubleshooting

**Meeting starts, but the "Them" track is missing.** The system audio tap received no callbacks. Check that System Audio Recording is granted for Mandy, that a default output device exists, and that something was actually playing. The log notes when the tap starts, when the first callback arrives and when it stops, so a silent tap is visible there. Open the Debug page to find the log folder.

**Only the "Me" track transcribes, "Them" says "No speech detected."** The tap worked but recorded silence. The tap follows the default output device. Switching output in the middle of a call, for example to AirPods, moves the tap with it, but audio routed through a virtual device or an aggregate device may not pass through.

**Shortcut stays on "Waiting..." after an update, nothing is pasted.** The Accessibility grant belongs to the previous build. Remove Mandy from Accessibility and grant it again, see [Updating to a new version](#updating-to-a-new-version).

**Meeting refuses to start on macOS 13 or earlier.** Process taps are a macOS 14.2 feature. Dictation still works.

**Microphone recording moved to the built-in mic.** The preferred device went away for a moment. Mandy switches back automatically once it returns. If the switch was unwanted, pick the device explicitly on the General page.

**Previous clipboard content gets pasted, pasting fails on Linux, Wayland shortcuts, WebKit rendering, Vulkan layers on Windows.** These are unchanged from Handy. See the upstream [Troubleshooting](https://github.com/cjpais/Handy#troubleshooting) and [Linux notes](https://github.com/cjpais/Handy#linux-notes). Environment variables such as `HANDY_NO_GTK_LAYER_SHELL` and `HANDY_KEEP_VULKAN_IMPLICIT_LAYERS` keep their names.

**Manual model installation.** Put `.bin` (Whisper GGML) or `.gguf` files into `models/` inside the app data folder and restart. Download links are listed in the [upstream README](https://github.com/cjpais/Handy#manual-model-installation-for-proxy-users-or-network-restrictions).

## Project layout

```
src-tauri/src/
├── meeting/              # Meeting recording (macOS only)
│   ├── mod.rs            #   MeetingManager: start/stop both halves, queue transcription
│   ├── mic.rs            #   Microphone half with device-loss supervisor
│   ├── system_tap.rs     #   System audio half: ring buffer + writer thread over the Swift tap
│   └── flac.rs           #   Streaming mono FLAC writer (.partial → final rename)
├── commands/meeting.rs   # Tauri commands: start/stop/is_recording
├── managers/             # Audio, model, transcription, history, transcription jobs
├── actions.rs            # Shortcut actions, including MeetingAction
└── ...
src-tauri/swift/
├── system_audio_tap.swift       # Core Audio process tap (CATapDescription)
└── system_audio_tap_stub.swift  # Used when the SDK lacks process taps
src/components/settings/meetings/MeetingsSettings.tsx   # Meetings page
```

[AGENTS.md](AGENTS.md) has the full architecture overview and conventions for anyone, human or otherwise, working on the code.

## Contributing

Issues and pull requests are welcome at [yuri-research-ai/Mandy](https://github.com/yuri-research-ai/Mandy). Changes that are not specific to meeting recording or Mandy's branding are usually better sent upstream to [Handy](https://github.com/cjpais/Handy), where they reach more people. Handy's [contributing guide](CONTRIBUTING.md) and [translation guide](CONTRIBUTING_TRANSLATIONS.md) apply here as well.

## License and credits

MIT. See [LICENSE](LICENSE). Mandy is built on [Handy](https://github.com/cjpais/Handy), copyright CJ Pais and contributors. The Handy name, logo and mascot are not open source and are not used by Mandy; the Mandy name and mark are this project's own.

Thanks to OpenAI (Whisper), NVIDIA (Parakeet), ggml, Silero, the Tauri team, and everyone who contributed to Handy.
