# AudioFocus

<img width="1774" height="887" alt="ChatGPT Image May 9, 2026, 11_02_33 PM" src="https://github.com/user-attachments/assets/01ed2ecf-303c-4ae4-afc9-89a8d07dc618" />


**Smart audio coordination for Windows.** AudioFocus automatically manages your music and videos so you never have to manually pause one app to hear another.

## 🎧 What does AudioFocus do?

Windows is great, but it has one annoying flaw: it lets every app play sound at the same time. If you're listening to Spotify and click a YouTube link, both will blast audio at you until you manually pause one.

**AudioFocus fixes this.** It works in the background to ensure only one "active" app is playing at a time.

### Key Benefits
- **Auto-Pause:** Start a video in VLC or Edge, and your background Spotify music pauses instantly.
- **Smart Hand-back:** Close a temporary video or clip, and your music **automatically resumes** where you left off.
- **Set & Forget:** No complex setup. Just run it, and it lives silently in your system tray.
- **Native Experience:** Built specifically for Windows 10 and 11 using high-performance, lightweight technology.

---

## 🚀 Getting Started

1.  **Download:** Grab the latest `audiofocus.exe`.
2.  **Run:** Double-click the file. You'll see a small notification and a headphone icon in your system tray.
3.  **Enjoy:** Start playing any audio. The app handles the rest!

### Tray Options (Right-Click)
- **Active:** Toggle this to temporarily disable automatic pausing.
- **Auto-Resume Recently Paused:** If enabled (default), your music will come back to life when you stop watching other videos (within 5 minutes).
- **Restart:** Refreshes all background services if Windows audio acting up.
- **Open Logs Folder:** See exactly how the app is making decisions.

---

## 🛠️ Performance & Privacy
- **Ultra Lightweight:** Uses almost zero CPU (0.1% or less) and very little memory (<15MB).
- **Privacy First:** No internet connection required. No data is ever sent to the cloud. All coordination happens locally on your PC.
- **Stability:** Built-in "Self-Healing" logic detects if Windows audio drivers crash and automatically reconnects to keep things running smoothly.

---

## 🏗️ How it's Built (For Developers)

AudioFocus is a professional-grade Rust application that bridges the gap between modern WinRT APIs and legacy Win32 systems:
- **Universal Monitoring:** Combines SMTC (modern apps) and WASAPI (legacy apps) into a single event stream.
- **Identity Tracking:** Uses process creation timestamps to uniquely identify apps, preventing "ghost" sessions even if Windows reuses Process IDs.
- **Arbitration Brain:** A custom state machine with debouncing and loop-protection to prevent "pause-wars" between apps.

### Build from Source
Requires [Rust](https://rustup.rs/) (Stable MSVC) and Windows 10/11 SDK.
```powershell
cargo build --release
```

## License
MIT
