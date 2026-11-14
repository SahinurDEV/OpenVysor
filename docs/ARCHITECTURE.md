# OpenVysor Architecture

OpenVysor is one native Rust program in `native/`. The earlier Tauri and React version in
`src-tauri/` and `src/` is kept for reference only and is not shipped.

## Overview

```
Native windows (egui)                Rust threads                              Android device
---------------------                ------------                              --------------
Device list   <---- registry ------- adb watcher (polls `adb devices -l`, 1 s)
Phone window  <---- newest frame --- decoder thread <--- video thread <------- scrcpy-server
                                                                               (H.264 over adb forward)
Mouse/keys    ----- queue ---------> control writer thread -------------------> control socket
Speakers      <---- PCM queue ------ audio thread <---------------------------- audio socket
```

The interface never builds command lines. Serials come from adb's own device list, and
addresses, package names and phone paths are validated or shell-quoted before they reach
`adb` as separate arguments.

## Decisions

| Decision | Choice | Why |
| --- | --- | --- |
| Interface | egui through eframe, OpenGL | One small program, no browser engine |
| Streaming | Official `scrcpy-server` v4.1, own client in Rust | Proven capture and injection, Apache-2.0, video inside our own windows |
| Per-app windows | `new_display=` on the server, one virtual display per app | Several apps side by side while the phone stays free |
| Video decode | Media Foundation on Windows, OpenH264 on macOS and Linux | No FFmpeg to ship |
| Frame delivery | Only the newest decoded picture is kept | A slow window never falls behind live |
| Tunnel | `adb forward tcp:0 localabstract:scrcpy_<scid>` | adb picks a free port, so several sessions never collide |
| Device tracking | Poll `adb devices -l` every second and diff | Survives adb server restarts without stream resync logic |
| Process cleanup | Windows Job Object with kill-on-close; a process group led by a watcher on macOS and Linux | Sessions die with the app, even after a crash |
| adb server | Started outside the job | Otherwise closing OpenVysor would kill an adb server other tools share |
| Windows install | Per-user NSIS installer, plus a portable `.exe` | No administrator rights, and a normal uninstall from Settings |

## scrcpy v4.1 protocol summary

The client and server versions must match exactly, so the bundled server and
`SCRCPY_VERSION` in `native/src/mirroring/protocol.rs` change together.

1. Push the server once per phone to `/data/local/tmp/openvysor-server-<version>.jar`. Each
   session runs a throwaway copy, because a running server deletes its own jar.
2. Forward a local port to `localabstract:scrcpy_<scid as 8 hex digits>`.
3. Run `adb -s S shell CLASSPATH=... app_process / com.genymobile.scrcpy.Server 4.1 key=value...`.
4. Connect the video socket, then the audio socket if wanted, then the control socket. The
   server writes one dummy byte on the first socket, then a 64-byte NUL-padded device name.
5. Video socket: `u32` codec id (0 = disabled, 1 = configuration error), then 12-byte headers.
   - MSB set: session packet with width and height at bytes 4 and 8. A new one follows rotation.
   - MSB clear: `u64` PTS and flags (bit 62 config, bit 61 key frame), `u32` size, payload.
6. Control socket: big-endian messages, layouts in `mirroring/protocol.rs`. Device messages
   (clipboard, acks) come back on the same socket and are always read, so the server never blocks.

A config packet (SPS/PPS) is merged into the next frame, so every key frame can be decoded
on its own.

## Modules (`native/src`)

- `adb/`: binary discovery, process wrapper with timeouts, device parsing, the watcher,
  wireless pairing and connecting, one-click ADB setup (pinned version and SHA-256), notifications.
- `mirroring/`: wire protocol, session lifecycle, job object, app listing.
- `video.rs`, `decoder/`: decoding on a background thread, keeping only the newest picture.
- `phone.rs`: a phone window, its toolbar, input mapping and key mapping.
- `audio.rs`: plays raw 48 kHz PCM through the default output with `cpal`, whole-phone sessions only.
- `recorder.rs`: writes the phone's own H.264 stream to MP4 without re-encoding.
- `clipboard.rs`: one thread owns the clipboard and polls it every 800 ms. A tracker stops
  text that came from the phone being sent straight back.
- `transfer/`: installs dropped APKs, copies files to Download, and the Files page's
  browse, download and upload.
- `state.rs`, `profiles.rs`: settings, workspaces, per-app quality and positions.
- `app.rs`, `theme.rs`, `tray.rs`: the main window, its look, and the tray icon and shortcut.

## Licensing

| Component | License | How it is used |
| --- | --- | --- |
| scrcpy-server | Apache-2.0 | Bundled unmodified, with its license and a NOTICE entry |
| egui, eframe, cpal, mp4 and other crates | MIT / Apache-2.0 | Normal dependencies |
| OpenH264 (macOS and Linux) | BSD-2-Clause | Built in, license shipped alongside |
| adb (Google platform-tools) | Google's SDK terms | **Not bundled.** Found on the machine, or downloaded by the user's choice |
| OpenVysor | Apache-2.0 | Project license |

No Vysor code, assets or implementation details are used. Features come from public Android
and scrcpy documentation.
