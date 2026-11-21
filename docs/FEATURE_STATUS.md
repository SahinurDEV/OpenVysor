# Feature Status

Last updated 2026-11-21. "Device verified" means tested against a real phone: a Nothing A059
or a Samsung Galaxy A54 (SM-A546E), both on Android 16. See [TESTING.md](TESTING.md) for evidence.

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
| Whole-phone mirroring | Device verified on the Galaxy A54 through the window's own input code: a click on Calculator, typing and Backspace in a text field, right-click Back, middle-click Home, and the Recent apps and Home buttons |
| Drag, wheel, typing, rotation, auto-resume | Device verified on the Galaxy A54: a drag and the wheel scrolled Settings both ways; the picture turned landscape and a tap still landed on the right key; after `adb reconnect` the window came back by itself in about 2 s |
| Clipboard sync | Device verified both ways |
| Drag and drop | Device verified on the Galaxy A54: a text file reached Download and an APK installed, including a test-only build from Android Studio. Fixed on the way: dropping anything froze the window |
| `Alt` + `Left`/`Right` | Verified with two real windows: each sends Focus to the next or previous phone window, wrapping around |
| Wireless | Written. **A successful connection is not verified**: the computer and phone tested were on separate networks |
| Phone sound | Device verified that sound streams to the computer: 1.7 MB of audio arrived in a few seconds. **Whether it is audible is not verified**, because another running copy of OpenVysor held the phone's audio capture |
| Screenshot | Device verified: saved to `Pictures\OpenVysor` |
| Recording to MP4 | Device verified: stopped with the button (1 min 24 s file) and by closing the window (1 min 52 s file). Video only, no sound |
| Opening speed | Measured on a real phone: window in 245–367 ms, first picture about 0.95 s once the server is on the phone |
| End-to-end latency | **Not measured** |
| Hardware video decoding | Not done; decoding runs on the CPU |
| Installer | Windows installer and uninstaller verified silently on Windows 11. Linux Flatpak manifest in `packaging/flatpak/` |
| Workspaces (save, delete) | Device verified on the Galaxy A54: a workspace with Calculator and Clock was saved, both closed, and reopening it brought both back live |
| Per-app quality and window position | Device verified on the Galaxy A54: Smooth gave a 1280-pixel stream and the window opened at its saved position; Sharp gave the phone's native 1600 |
| Key mapping | Device verified on the Galaxy A54: `G` bound to Calculator's 7 typed "77" when pressed twice |
| File browser | Device verified on the Galaxy A54: a file dropped on the Files page uploaded to Download, the filter box found it in a 111-item folder, and its Download button saved it to the computer. Listing also verified on the Nothing phone, including odd names and a 276-item folder |
| Notifications | Device verified on the Galaxy A54: after **Turn on**, a new notification appeared on the page within one refresh. Parser also verified against real `dumpsys` output from the Nothing phone |
| Tray icon and global shortcut | On Windows, the tray icon is created and `Ctrl+Shift+O` registers; a running copy of the app already owned the shortcut. **The key press and tray clicks not verified; macOS not verified** |
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
