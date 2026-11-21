# Testing

> **Warning:** the real-device and end-to-end tests send a real tap to the top-center of the
> screen, and the end-to-end test saves a real screenshot. On an unlocked phone the tap hits
> whatever is there. Lock the phone or close private apps first.

## Automated tests (no hardware)

```powershell
cd native
cargo test     # 101 tests: parsing, wire format, options, Job Object, clipboard, recording, audio
cargo clippy --all-targets
```

These use mocks or pure functions, and say nothing about real-device behaviour. CI runs
`cargo test` on Windows, Linux and macOS.

## Real-device test (native app)

```powershell
cd native
cargo test --test real_device -- --ignored --nocapture --test-threads=1
```

This needs one authorized device. It briefly sleeps and wakes the screen and taps the clock
area once, and it works on a locked phone. It checks the video stream, key and touch
injection, and that the server and tunnel are gone afterwards.

Last result, 2026-11-09, Nothing A059 on Android 16: **passed**.

```
sessions=1 configs=1 keys=1 size=(722, 1600)
wakefulness: Dozing -> Dozing -> Awake
recent motion events: 2, age=582ms and 574ms
```

An earlier 5-second run measured 156 delta frames, about 31 FPS, on an idle lock screen.
Android only encodes when the screen changes, so this is not a maximum frame rate.

## Window tests on a real phone (native app)

`native/tests/ui_device.rs` runs the real phone window code without showing it. It sends the
same mouse, wheel, keyboard and file-drop events a person would, through `Phone::show`, finds
toolbar buttons by their accessible names, and checks the result on the phone with
`uiautomator` and `dumpsys`. Settings go to a temporary folder, never your own.

```sh
cd native
OPENVYSOR_TEST_SERIAL=<serial> cargo test --release --test ui_device -- --ignored --nocapture --test-threads=1
```

The phone must be awake and unlocked. The tests open Calculator, Clock and Settings, type
"wifi" into the Settings search box, rotate the screen and restore it, briefly drop the USB
connection with `adb reconnect`, reinstall the phone's smallest single-APK app from its own copy,
and post and remove one test notification. Files they send are deleted afterwards. Other phones'
notifications are never printed.

Last result, 2026-11-21, Samsung Galaxy A54 (SM-A546E) on Android 16 over USB: **11 passed**.

| Test | Result |
| --- | --- |
| Click, typing, Backspace, right-click Back, middle-click Home, Recent apps and Home buttons | Calculator showed 7; the search box read "wifi", then "wif" |
| Drag and wheel | Settings scrolled down by drag and wheel, and back up |
| Rotation | The video went from 738×1600 to 1600×738, a tap still hit 7, and the settings were restored |
| Automatic reconnection | Disconnected after 0.26 s; live again by itself after 2.1 s |
| Dropping a file and an APK | The file reached Download; the APK, a test-only build, installed |
| Per-app quality and position | Smooth streamed at 1280 and opened at the saved position; Sharp at the native 1600 |
| Workspace | Calculator and Clock saved, closed, and both reopened live |
| Key mapping | `G` on Calculator's 7, pressed twice, typed "77" |
| File browser | Upload by drop, the filter box in a 111-item folder, and Download to the computer |
| Notifications page | A new notification appeared after **Turn on** |
| `Alt` + `Left`/`Right` | Each sent Focus to the other phone window |
| Nothing left behind | No new server or tunnel on the phone after the window closed |

Bugs found and fixed:
- Dropping any file on a phone window froze it, because the window's state was locked twice.
- APKs built by Android Studio's Run button were refused, as they are marked test-only. The
  install now passes `-t`.
- Custom buttons and file rows had no accessible names, so screen readers could not announce them.

Not covered:
- **Sound.** The sound test received 1.7 MB of audio, but it was silent: another running copy of
  OpenVysor held the phone's audio capture. Rerun `sound_button_streams_phone_audio` with no
  other copy mirroring that phone. It plays a 440 Hz tone (`tests/fixtures/tone.mp4`) in the
  phone's own video player and expects it to arrive.
- **Wi-Fi.** The computer and the phone were on separate networks.
- **Tray and global shortcut.** `cargo test --test desktop -- --ignored` creates the tray icon
  and registers `Ctrl+Shift+O`. It failed only because the installed app already held the
  shortcut. Pressing the shortcut and clicking the tray were not tested.

## Release check for 0.5.0 (native app)

Run on 2026-11-14 with a Nothing A059 on Android 16 over USB.

| Check | Result |
| --- | --- |
| `cargo test` and `cargo clippy --all-targets` | 99 passed, no warnings |
| Real-device tests, release build | All 3 passed. New: a folder listed with sizes, including a name with a quote, one with a space and a hidden file; a missing folder reports an error; the 276-item Download folder lists |
| Notification parser on the phone's real `dumpsys` output | All 4 notifications read, against 1 before the fix; the lock screen copy no longer replaces the title |
| Flatpak, in WSL Ubuntu 24.04 | Built offline from `cargo-sources.json`; `appstreamcli validate` and `desktop-file-validate` pass; the app opens through WSLg and offers Install ADB |

Not covered: the Files page's size column and filter box seen in a real window, Install ADB
run inside the Flatpak, and a phone over USB from the Flatpak, as WSL has no USB device.

## Release check for 0.4.0 (native app)

Run on 2026-11-14 with a Nothing A059 on Android 16 over USB.

| Check | Result |
| --- | --- |
| `cargo test` and `cargo clippy --all-targets` | 92 passed, no warnings |
| Real-device tests, release build | Both passed: picture after about 1 s, a tap on 7 changed Calculator, no tunnel or server left on the phone |
| Installer, silent (`/S`) | Installed to `%LOCALAPPDATA%\Programs\OpenVysor` without an administrator prompt; Start menu and desktop shortcuts and the Installed apps entry created; the installed app opened |
| Uninstaller, silent, with the app open | Closed the app; removed the program, shortcuts and Installed apps entry; kept the settings |

Not covered: the installer's pages clicked by hand, the uninstaller's option that deletes
settings, and the phone window's reconnect and "stream ended while starting" paths through the UI.

## Earlier Tauri version

The two sections below tested the earlier Tauri and React app in `src-tauri/` and `src/`. The
scripts drive WebView2, so they do not work with the native app.

## End-to-end test (packaged app)

`scripts/e2e.mjs` drives the release build through WebView2's DevTools protocol. It covers
rendering, mouse and keyboard through the UI, screenshot, disconnect and reconnect.

```powershell
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9223"
Start-Process src-tauri\target\release\openvysor.exe
node scripts\e2e.mjs <folder-for-screenshots>
```

Last result, 2026-11-09, Nothing A059 on Android 16: **passed**, no console errors.

```
canvas: 722x1600, non-black image
stream: 722x1600 at 8 FPS (idle screen)
click: 2 motion events at the dispatcher, about 600 ms old
key: 2 key events at the dispatcher, about 440 ms old
screenshot: saved to Pictures\OpenVysor
disconnect: no tunnel left, no server left on the device
reconnect: streaming again at 722x1600
app exit: no tunnel or server left, shared adb server still running
```

The frame rate reflects an idle screen, because Android only encodes changes. Latency,
smoothness under motion, drag, wheel, typing into a field, rotation, full-screen mode, quality
changes during a session, and automatic resume after an unplug have not been tested.

## Phone windows (packaged app)

`scripts/e2e-windows.mjs` opens Calculator and Clock in their own windows and checks the
whole-phone window by numbers only, so no private screen is saved.

Last result, 2026-11-09, Nothing A059 on Android 16: **passed**, no console errors. Each app
ran on its own virtual display, a click changed the Calculator, the windows lined up side by
side, and closing them left no tunnel or server behind.

Open issue: `Alt + Left/Right` sent with WScript.Shell SendKeys did not change the foreground
window, although calling the same backend command directly did.

## Clipboard and Use over Wi-Fi (native app)

Run by hand on 2026-11-11 with the release build and a Nothing A059 on Android 16 over USB.
The phone's clipboard was read by pasting into the Settings search field, and the computer's
with `Get-Clipboard`.

| Check | Result |
| --- | --- |
| Text copied on the computer while a whole-phone window is open | Pasted on the phone |
| Text copied while no window is open, then a window opened (issue #6) | Pasted on the phone |
| Text copied on the phone | Reached the Windows clipboard within 3 s |
| Text copied before the app started | Not sent, by design |
| Use over Wi-Fi with the computer on another network | Clear error after about 20 s; the phone returned to USB mode (`service.adb.tcp.port` 0) |

| Screenshot button on a whole-phone window | Saved a PNG to `Pictures\OpenVysor` |
| Record button, then stop | Saved an MP4 that Windows reads as 1 min 24 s |
| Record button, then close the window | Saved an MP4 that Windows reads as 1 min 52 s |

Some clicks on the record button sent as posted window messages did not register. The
phone also received a key press and a tap that the test did not send, so the computer may have
been in use, and real cursor movement can move the pointer between a posted press and release.
The cause was not confirmed.

Before this run, an unreachable phone kept the button spinning for over 2.5 minutes, because
each `adb connect` waited about 21 s. A successful switch was not tested, because the
computer was on wired Ethernet on a different network from the phone.
