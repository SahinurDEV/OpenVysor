# Testing

> **Warning:** the real-device and end-to-end tests send a real tap to the top-center of the
> screen, and the end-to-end test saves a real screenshot. On an unlocked phone the tap hits
> whatever is there. Lock the phone or close private apps first.

## Automated tests (no hardware)

```powershell
pnpm exec tsc --noEmit
pnpm exec vitest run            # 19 tests: input mapping, H.264 config, stores
cd src-tauri; cargo test --lib  # 22 tests: parsing, wire format, options, Job Object
```

These use mocks or pure functions, and say nothing about real-device behaviour.

## Real-device test (Rust backend)

```powershell
cd src-tauri
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
