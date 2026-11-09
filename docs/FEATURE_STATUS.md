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
| P1 | Full-screen mode | Written, **not yet verified** |
| P1 | Several devices at once | Backend supports it; UI shows one session; **not verified** |
| P1 | Wireless ADB | Not started (detected in parsing only) |
| P1 | APK installation | Not started (shown as "Soon") |
| P1 | File transfer | Not started (shown as "Soon") |
| P2 | Resolution, bitrate and FPS controls | Written, **not yet verified on a device** |
| P2 | Advanced device information | Written, **not yet verified on a device** |
| P2 | Clipboard sync, recording, audio | Not started |
| P3 | Profiles, history, shortcuts | Recent devices written; the rest not started |

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
