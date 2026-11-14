# Feature Status

Last updated 2026-11-11. "Device verified" means tested against a real phone (Nothing A059,
Android 16). See [TESTING.md](TESTING.md) for evidence.

| Priority | Feature | Status |
| --- | --- | --- |
| P0 | ADB detection and setup guidance | Backend device verified; missing-ADB guide not exercised |
| P0 | Device discovery and parsing | Device verified in the packaged app, including noticing a replugged phone |
| P0 | H.264 stream from the device | Device verified at backend level |
| P0 | Rendering in the app (WebCodecs, canvas) | Device verified. Smoothness and latency not measured under motion |
| P0 | Mouse control | Click device verified through the UI; drag and wheel **not yet verified** |
| P0 | Keyboard control | Single key device verified through the UI; typing into a field **not yet verified** |
| P0 | Disconnect and tunnel/server cleanup | Device verified through the UI and on app exit |
| P0 | Reconnect | Manual reconnect device verified; automatic resume after an unplug **not yet verified** |
| P1 | Screenshot | Device verified |
| New | Per-app phone windows on virtual displays | Device verified with Calculator and Clock, two at once |
| New | Detach whole phone to a window | Device verified, numbers only |
| New | Arrange windows side by side | Device verified |
| New | Switch windows (backend command) | Device verified against the Windows foreground window |
| New | Alt + Left/Right shortcut | **Did not switch windows in a synthetic key test; cause unknown** |
| New | App list with search | Device verified, 122 apps listed |
| P1 | Full-screen mode | Written, **not yet verified** |
| P1 | Several devices at once | Backend supports it; UI shows one session; **not verified** |
| P1 | Wireless ADB (QR code, pairing code with automatic connect, IP address, Use over Wi-Fi from USB, reconnect at startup) | Use over Wi-Fi device verified to fail cleanly when the phone is unreachable. **A successful connection is not yet verified on a device** |
| P1 | APK installation by drag and drop | Written, **not yet verified on a device** |
| P1 | File transfer by drag and drop (to Download) | Written, **not yet verified on a device** |
| P2 | Resolution, bitrate and FPS controls | Written, **not yet verified on a device** |
| P2 | Advanced device information | Written, **not yet verified on a device** |
| P2 | Clipboard sync, both directions | Device verified both ways, including text copied before a window opened |
| P2 | Screen recording | Device verified in the native app |
| P2 | Phone audio on the PC (whole-phone sessions only) | Written, **not yet verified on a device** |
| P3 | Profiles, history, shortcuts | Recent devices written; the rest not started |

## Native Rust app (`native/`)

The same app with no WebView, React or TypeScript: egui for the interface, the Windows
H.264 decoder for video. `adb` and scrcpy-server remain separate programs.

| Feature | Status |
| --- | --- |
| Live video from the phone | Device verified with Calculator on a virtual display |
| Touch input | Device verified: a tap changed the Calculator display |
| Mouse reaching the phone area | Observed in the app's log during manual use |
| Phone window, drawn toolbar, full screen, close | Observed working in the running app |
| Clean exit, no tunnel left behind | Verified |
| Whole-phone mirroring | Picture device verified; mouse input in that window **not yet verified** |
| Drag, wheel, typing, rotation, auto-resume | Written, **not yet verified** |
| Clipboard sync | Device verified both ways |
| Drag and drop, wireless, sound | Written, **not yet verified** |
| Screenshot | Device verified: saved to `Pictures\OpenVysor` |
| Recording to MP4 | Device verified: stopped with the button (1 min 24 s file) and by closing the window (1 min 52 s file). Video only, no sound |
| Opening speed | Measured on a real phone: window in 245–367 ms, first picture about 0.95 s once the server is on the phone |
| End-to-end latency | **Not measured** |
| Hardware video decoding | Not done; decoding runs on the CPU |
| Installer | Windows installer and uninstaller verified silently on Windows 11. Linux Flatpak manifest in `packaging/flatpak/` |
| Workspaces (save, delete) | Save verified through the real UI against a mock adb on a virtual display. **Reopening not verified on a phone** |
| Per-app quality and window position | Saving verified through the real UI against a mock adb. **Effect on a real stream not verified** |
| Key mapping | Adding and removing a binding verified through the real UI on a sample picture. **Touches not verified on a phone** |
| File browser | Listing with sizes verified on a phone (Android 16), including odd names and a 276-item folder. Browsing and downloading verified through the real UI against a mock adb. Filter unit tested. **Upload and download not verified on a phone** |
| Notifications | Parser verified against real `dumpsys` output from a Nothing Phone (3a), Android 16: all four notifications read, with text over several lines. **Page not verified on a phone; other Android versions may differ** |
| Tray icon and global shortcut | Window restore verified on Windows. **The key press and tray clicks not verified; macOS not verified** |
| Mirror all | Button verified with two mock phones. **Not verified with two real phones** |
| One-click ADB setup | Real download and install verified on Windows, and a failed download recovers. **Not verified on macOS** |
| Quality presets, open windows list, page shortcuts, search keys | Unit tested and checked in screenshots, **not yet verified with a phone** |

## Platforms (native app)

| Platform | Builds and unit tests | App opens | Mirroring on a real phone |
| --- | --- | --- | --- |
| Windows 10/11 x64 | Passes | Verified | Verified for app windows, see above |
| Linux x64 | Passes in a Debian container | Verified on a virtual X11 display | **Not verified** |
| macOS arm64 and x64 | Passes on GitHub Actions | **Not verified** | **Not verified** |

macOS and Linux decode video in software with OpenH264, assuming the BT.709 color
matrix. Wayland does not let apps position windows, so "Arrange windows" may do
nothing there. Builds are not signed, so Windows and macOS show a security warning.
Setup is in [PLATFORM_SETUP.md](PLATFORM_SETUP.md).

## Windows release

| Item | Status |
| --- | --- |
| Release build | Passes |
| NSIS installer | Builds (2.17 MiB) |
| Packaged release app runs the full mirroring flow | Verified from the build folder |
| Install and uninstall | Not tested |
| Clean machine test | Not tested |
| Code signing | Not done; SmartScreen will warn |

REA was not configured, so no REA investigation was performed.
