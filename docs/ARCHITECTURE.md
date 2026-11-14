# OpenVysor Architecture (Windows first)

## Overview

```
React UI (WebView2)                    Rust backend (Tauri 2)                 Android device
-------------------                    ----------------------                 --------------
Device list  <---- devices-changed --- adb::watcher (polls adb devices)
Canvas       <---- binary Channel ---- mirroring::session (video thread) <--- scrcpy-server
  WebCodecs H.264 decoder                                                     (H.264 over adb forward)
Pointer/keys ----- invoke ---------->  mirroring::control ------------------> control socket
```

The UI never builds command lines. It calls typed Tauri commands, and Rust validates
every serial, host and path before passing them to `adb` as discrete arguments.

## Decisions

| Decision | Choice | Why |
| --- | --- | --- |
| Streaming | Official `scrcpy-server` v4.1, own client in Rust | Proven capture and injection, Apache-2.0, and video renders inside our window instead of a separate scrcpy window |
| Video decode | WebCodecs in WebView2, painted to a canvas | Hardware decode with no FFmpeg to ship |
| Video transport | Tauri binary `Channel` | No open local port. Replace with a token-protected loopback socket only if measured latency requires it |
| Tunnel | `adb forward tcp:0 localabstract:scrcpy_<scid>` | adb picks a free port, so several devices never collide |
| Device tracking | Poll `adb devices -l` every 2 s and diff | Survives adb server restarts without stream resync logic |
| Process cleanup | Windows Job Object with kill-on-close for session processes | Sessions die with the app, even after a crash |
| adb server | Started outside the job | Otherwise closing OpenVysor would kill an adb server other tools share |

## scrcpy v4.1 protocol summary

The client and server versions must match exactly, so the bundled server and
`SCRCPY_VERSION` in `mirroring/protocol.rs` change together.

1. Push the server to `/data/local/tmp/scrcpy-server.jar`.
2. Forward a local port to `localabstract:scrcpy_<scid as 8 hex digits>`.
3. Run `adb -s S shell CLASSPATH=... app_process / com.genymobile.scrcpy.Server 4.1 key=value...`.
4. Connect the video socket, then the control socket. The server writes one dummy byte
   on the first socket, then a 64-byte NUL-padded device name.
5. Video socket: `u32` codec id (0 = disabled, 1 = configuration error), then 12-byte headers.
   - MSB set: session packet with width and height at bytes 4 and 8. A new one follows rotation.
   - MSB clear: `u64` PTS and flags (bit 62 config, bit 61 key frame), `u32` size, payload.
6. Control socket: big-endian messages, layouts in `mirroring/protocol.rs`.

A config packet (SPS/PPS) is merged into the next frame, because a WebCodecs H.264
decoder with no `description` expects Annex B parameter sets in the key frame.

## Backend modules

- `adb/`: binary discovery, process wrapper, device parsing, watcher.
- `mirroring/`: wire protocol, session lifecycle, job object, packet merging.
- `commands/`: the Tauri command surface and input validation.
- `adb/wireless.rs`: pairing, connecting, and the QR pairing flow.
- `transfer/`: installs dropped APKs and copies other files to `/sdcard/Download/`.
- `clipboard.rs`: one thread owns the Windows clipboard and polls it every 800 ms. A tracker
  stops text that came from the phone being sent straight back.

## Audio and recording

Whole-phone sessions can open a third scrcpy socket carrying Opus audio. Packets of kind 4
(config) and 5 (frame) go to `services/audioPlayer.ts`, which decodes with WebCodecs and plays
through an `AudioContext`. Per-app windows have no audio.

`services/recorder.ts` records the canvas plus that audio with `MediaRecorder`, preferring MP4
and falling back to WebM. One-second chunks are written in order to `Videos\OpenVysor`.

## Licensing

| Component | License | How it is used |
| --- | --- | --- |
| scrcpy-server | Apache-2.0 | Bundled unmodified, with its license and a NOTICE entry |
| Tauri, React, Tailwind, Lucide, shadcn/ui patterns | MIT / Apache-2.0 / ISC | Normal dependencies |
| adb (Google platform-tools) | Prebuilt binaries are under Google's SDK terms | **Not bundled.** Found on the machine, or the user is guided to install it |
| OpenVysor | Apache-2.0 | Project license |

No Vysor code, assets or implementation details are used. REA was not configured and
Vysor was not inspected; features come from public Android and scrcpy documentation.
