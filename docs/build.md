# Building AudioFocus

AudioFocus is a standard Rust project built to run natively on Windows 10 and 11. It utilizes the `windows-rs` crate to interop with native COM and WinRT APIs.

## Prerequisites

1.  **Rust Toolchain**: Install via [rustup.rs](https://rustup.rs/). The default stable `x86_64-pc-windows-msvc` toolchain is required.
2.  **Windows SDK**: Ensure you have the Visual Studio C++ Build Tools installed (specifically the Windows 10/11 SDK). This is typically installed automatically with rustup if you opt into the MSVC prerequisites.

## Compiling for Release

AudioFocus is configured with an optimized release profile in `Cargo.toml`. To build the final, single executable:

```powershell
cargo build --release
```

The output executable will be located at:
`target\release\audiofocus.exe`

### Optimization Details
The release build applies the following optimizations:
- `opt-level = "z"`: Aggressively optimizes for binary size.
- `lto = true`: Link-Time Optimization is enabled to inline dependencies and reduce overhead.
- `codegen-units = 1`: Improves optimization quality by analyzing the entire crate as a single unit.
- `panic = "abort"`: Prevents unwinding bloat in fatal scenarios (monitored worker panics are caught explicitly via `catch_unwind` where safe).
- `strip = true`: Strips debug symbols from the final executable to further reduce size.

## Running the Application

Double-click `audiofocus.exe`.

Because the application is built with `#![windows_subsystem = "windows"]`, it will launch directly into the system tray without opening a visible command prompt. 

### Diagnostics & Logs
You can monitor the runtime by checking the JSON-structured logs. To access the logs:
1. Right-click the AudioFocus tray icon.
2. Select "Open Logs Folder".

By default, logs are written to a `logs` directory immediately alongside the `audiofocus.exe` location.

## Optional: Auto-Start Registration

To launch AudioFocus automatically when Windows starts, you can add a shortcut to the Windows Startup folder:
1. Press `Win + R`, type `shell:startup`, and hit Enter.
2. Right-click in the folder, choose `New -> Shortcut`.
3. Point the shortcut to the absolute path of your compiled `audiofocus.exe`.
