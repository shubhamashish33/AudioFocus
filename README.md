# AudioFocus

A production-grade Windows background utility that provides intelligent audio focus orchestration. AudioFocus automatically pauses background media when a new media source starts playing, bringing mobile-like audio focus behavior to Windows 10 and 11.

## The Problem
Windows allows multiple applications to play audio simultaneously without coordination. While some modern apps use the System Media Transport Controls (SMTC), many legacy applications (VLC, MPC-HC, web browsers) do not participate in a unified focus system. This results in "audio overlapping" where users must manually pause one app to hear another.

## Key Features
- **Intelligent Arbitration:** Automatically pauses the "losing" media source when a new one starts.
- **Universal Support:** Works with modern SMTC-enabled apps (Spotify, Netflix, YouTube PWA) and legacy desktop apps (VLC, Foobar2000, MPC-HC).
- **Stable Identity Tracking:** Tracks applications across audio session recreations, process restarts, and PID reuse.
- **Zero-Config Tray App:** Runs silently in the tray with minimal CPU/Memory footprint.
- **Production Hardened:** Built-in watchdog, event storm protection, and automatic subsystem recovery.

## Architecture

### 1. Monitoring Layer (The Eyes)
AudioFocus employs a dual-monitoring strategy to capture all media activity:
- **SMTC Watcher:** Uses the Windows Runtime (WinRT) `GlobalSystemMediaTransportControlsSessionManager` to receive native events from modern apps. It provides rich metadata (Title, Artist) and accurate playback state.
- **WASAPI Monitor:** Connects to the default audio render endpoint via the Windows Audio Session API. It monitors `IAudioSessionControl2` for session state and uses `IAudioMeterInformation` to detect actual audio peak activity in legacy apps that don't report playback state correctly.

### 2. Identity System (The Memory)
Because PIDs and HWNDs are transient, AudioFocus uses a robust identity abstraction:
- **`MediaSourceId`:** A deterministic, stable string generated from executable paths or Appx package names.
- **Process Lifetime Tracking:** Uses `GetProcessTimes` (Creation Time) to ensure that if a PID is reused by Windows, the old identity is safely expired and not confused with the new process.
- **Session Reconciliation:** Automatically merges WASAPI and SMTC streams belonging to the same process into a single logical "Hybrid" source.

### 3. Arbitration Engine (The Brain)
The engine processes a stream of media events and maintains a state machine of playback ownership:
- **Debouncing:** Prevents rapid-fire "MediaStarted" events from causing flickery pauses.
- **Loop Guard:** Detects and breaks infinite "App A pauses App B -> App B resumes -> App B pauses App A" cycles.
- **Decision Matrix:** Computes commands (`Promote`, `Switch`, `Reject`) based on source priority and recency.

### 4. Transport Layer (The Hands)
Executes the engine's decisions:
- **SMTC Controller:** Sends `TryPauseAsync` commands to WinRT sessions.
- **Non-SMTC Controller:** Uses `EnumWindows` and `GetWindowThreadProcessId` to find the target app's window and injects a synthesized `WM_APPCOMMAND` (APPCOMMAND_MEDIA_PAUSE) message.

### 5. Reliability & Hardening
- **Watchdog:** Monitors worker thread heartbeats; stalls trigger an automatic subsystem restart.
- **Event Storm Protection:** A sliding-window rate limiter prevents event floods from consuming CPU or causing recursion.
- **Panic Containment:** All background tasks are wrapped in `catch_unwind` to prevent a single component failure from crashing the entire utility.

## Production Observations
- **CPU Usage:** Under 0.1% idle. Process scanning is throttled to a 5-second maintenance timer to minimize wakeups.
- **Memory:** Fixed-size history buffers and proactive registry pruning ensure a stable memory footprint (typically < 15MB).
- **Latency:** Switch latency is typically < 200ms, depending on how quickly the target application reacts to `WM_APPCOMMAND`.

## Building from Source

### Prerequisites
- [Rust](https://rustup.rs/) (Stable MSVC toolchain)
- Windows 10 or 11 SDK

### Build
```powershell
cargo build --release
```
The binary will be generated at `target/release/audiofocus.exe`.

## Usage
- **Launch:** Run `audiofocus.exe`. It will appear in your system tray.
- **Toggle:** Double-click the tray icon to enable/disable automatic arbitration.
- **Logs:** Right-click -> "Open Logs Folder" to see structured JSON diagnostics.

## License
MIT
