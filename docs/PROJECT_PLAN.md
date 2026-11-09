# OpenVysor Project Plan

First milestone: a Windows desktop app that detects an Android phone over USB and
shows its live screen with working mouse and keyboard control.

| Phase | Scope |
| --- | --- |
| 0 Research | Environment, scrcpy protocol, licenses, architecture |
| 1 Foundation | Tauri 2, React, navigation, theme system, dev build |
| 2 ADB engine | Detection, device parsing, connection states, real-device test |
| 3 Mirroring | Server launch, video decode, input, disconnect and reconnect |
| 4 Device management | Wireless ADB, multiple devices, APK install, file transfer, screenshots |
| 5 Polish | Performance, logging, recording, clipboard, audio, automated tests |
| 6 Release | Installer, clean-machine test, user docs, GitHub release |

Linux and macOS are out of scope until the Windows release ships.

## Biggest technical risks

1. **Video path latency.** Tauri channels are not designed for video rates. Measure on real
   hardware; the fallback is a token-protected loopback socket.
2. **WebCodecs in WebView2.** H.264 decode depends on the runtime and GPU. Needs a real test,
   and a clear error if the codec is unsupported.
3. **Input on OEM builds.** Some vendors block injection until a developer option is enabled
   (for example Xiaomi's "USB debugging (Security settings)"). Surface this as guidance.
4. **Version lock.** The server must match the protocol code exactly, so upgrades are deliberate.
5. **Orphaned processes.** Handled with a Job Object and cleanup on every exit path.
6. **Unsigned installer.** Windows SmartScreen will warn until the binaries are code-signed.
7. **Project name.** "OpenVysor" contains another product's name. Check trademark risk before
   a public release.
