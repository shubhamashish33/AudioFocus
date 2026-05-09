# AudioFocus Architecture

AudioFocus is a native Windows system tray application built in Rust that monitors and orchestrates media playback across various applications using the Windows API (WASAPI, SMTC, and UI Automation).

## Core Philosophy
- **Zero visible footprint:** Runs entirely in the background as a system tray application.
- **Event-driven:** No busy-waiting. The system reacts purely to OS events (COM callbacks, Windows Messages).
- **Graceful degradation:** Fault-isolated worker threads with automated watchdog recovery.

## Subsystems

### 1. Identity System (`src/identity/`)
Tracks media playback applications across boundaries (session recreation, process restarts).
- **`IdentityManager`:** Computes stable, deterministic IDs for processes.
- **`SourceClassifier`:** Categorizes apps into Browsers, Streaming Apps, Dedicated Players, or System utilities.
- **`SessionReconciler`:** Merges duplicate identities from different APIs (e.g., when a browser triggers both WASAPI and SMTC events) into a single "Hybrid" source.
- **`SourceRegistry`:** Thread-safe, central repository for active `MediaSource` definitions.
- **`StaleSourceCollector`:** Periodically prunes expired processes.

### 2. Monitor Workers
- **SMTC Watcher (`src/smtc/`)**: Subscribes to the `GlobalSystemMediaTransportControlsSessionManager` to capture metadata and playback state of modern Windows apps (e.g., Spotify, Edge, Netflix).
- **WASAPI Monitor (`src/wasapi.rs`)**: Connects to the default audio render endpoint using `IAudioSessionManager2` to detect audio stream creation and volume peak activity for non-SMTC legacy apps (e.g., VLC).

### 3. Arbitration Engine (`src/arbitration/`)
The brain of AudioFocus. It determines which media application is allowed to play at any given time.
- **`ArbitrationState`**: Maintains the history of paused and active sources.
- **`DebounceCoordinator`**: Prevents rapid, redundant events from causing decision storms.
- **`PauseLoopGuard`**: Prevents infinite pause/resume loops between warring applications.
- **Decision Matrix (`decision.rs`)**: Computes `Noop`, `Promote`, `Switch`, or `RejectChallenger` commands based on current activity.

### 4. Transport Controllers
Responsible for executing the arbitration engine's "Pause" decisions.
- **SMTC Controller (`src/smtc/controller.rs`)**: Sends native async `TryPauseAsync` commands via WinRT.
- **Non-SMTC Controller (`src/non_smtc/`)**: Discovers top-level playback windows via `EnumWindows` and sends synthesized `WM_APPCOMMAND` (APPCOMMAND_MEDIA_PAUSE) messages. Implements retry coordination for stubborn applications.

### 5. Runtime Host & Tray (`src/tray/`, `src/app.rs`)
- **`TrayManager`**: Implements the native Win32 message loop (`GetMessageW`), manages the `Shell_NotifyIconW` tray icon, and renders the native popup menu.
- **`RuntimeHost`**: Owns the lifecycle of the workers, bridging the UI state with the background service. Provides `start()`, `stop()`, `restart()`, and toggle actions.
- **`SingleInstance`**: A named Win32 Mutex that prevents overlapping executions of AudioFocus.

### 6. Hardening & Recovery (`src/hardening/`)
- **`Watchdog`**: Background threads emit periodic heartbeats. If a thread stalls (e.g., due to a COM lockup), the watchdog flags it.
- **`RecoveryCoordinator`**: Connected to a 5-second maintenance timer in the tray window loop. It safely shuts down and restarts the runtime upon detecting watchdog failures.
- **`EventStormProtector`**: Limits maximum events processed within a rolling window to prevent infinite arbitration recursion.
- **`spawn_safe`**: `catch_unwind` wrapper for worker threads to prevent full process crashes on unexpected panics.

## Concurrency Model
The application uses standard `std::sync` primitives (`Arc`, `Mutex`, `RwLock`) and multi-producer, single-consumer channels (`std::sync::mpsc`) to pass events from the COM worker threads into the central `ArbitrationWorker`. The main thread owns the Win32 UI message loop and delegates heavy lifting to the async background runtime.
