# Feature Status

Last updated 2026-11-09. "Device verified" means tested against a real phone (Nothing A059,
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
| P1 | Wireless ADB (QR code, pairing code, IP address) | Written, unit tests pass, **not yet verified on a device** |
| P1 | APK installation by drag and drop | Written, **not yet verified on a device** |
| P1 | File transfer by drag and drop (to Download) | Written, **not yet verified on a device** |
| P2 | Resolution, bitrate and FPS controls | Written, **not yet verified on a device** |
| P2 | Advanced device information | Written, **not yet verified on a device** |
| P2 | Clipboard sync, both directions | Written, echo prevention unit tested, **not yet verified on a device** |
| P2 | Screen recording | Written, **not yet verified on a device** |
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
| Whole-phone mirroring | Written, **not yet verified** |
| Drag, wheel, typing, rotation, auto-resume | Written, **not yet verified** |
| Clipboard sync, drag and drop, wireless, sound | Written, **not yet verified** |
| Recording to MP4 | Written and unit tested, **not verified on a device**. Video only, no sound |
| Latency | **Not measured** |
| Hardware video decoding | Not done; decoding runs on the CPU |
| Installer | Not done; the app is one portable `.exe` |

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
