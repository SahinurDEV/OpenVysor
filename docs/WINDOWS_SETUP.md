# Windows Setup

## Using OpenVysor

You need Windows 10 or 11 x64, and ADB.

1. Install ADB, if you don't already have it: choose **Install ADB** in the app, or download
   [SDK Platform-Tools](https://developer.android.com/tools/releases/platform-tools) and unzip
   it into `%LOCALAPPDATA%\OpenVysor`.
2. On the phone, enable Developer options and turn on **USB debugging**.
3. Connect the phone with a USB data cable and tap **Allow** on the prompt.
4. Open OpenVysor, go to **Devices** and choose **Mirror screen**.

OpenVysor looks for `adb.exe` in this order: the `OPENVYSOR_ADB` environment variable,
`PATH`, `ANDROID_HOME` / `ANDROID_SDK_ROOT`, the default Android Studio folder, then WinGet
packages.

Some manufacturers need an extra developer option before input works. On Xiaomi devices,
enable **USB debugging (Security settings)**.

## Building from source

| Tool | Version used |
| --- | --- |
| Node.js | 22.17 |
| pnpm | 9.15 |
| Rust (MSVC toolchain) | 1.99 |
| Visual Studio Build Tools with the C++ workload | 2019 or later |
| WebView2 Runtime | Included in Windows 11 |

```powershell
pnpm install
pnpm tauri dev      # development
pnpm tauri build    # release build and NSIS installer
```

The installer is written to `src-tauri/target/release/bundle/nsis/`.

## Upgrading scrcpy-server

The client and server versions must match exactly. To upgrade, replace
`src-tauri/resources/scrcpy-server`, change `SCRCPY_VERSION` in
`src-tauri/src/mirroring/protocol.rs`, and check the wire format against that release's source.

Bundled v4.1 SHA-256, matching the official GitHub release:
`deacb991ed2509715160ffdc7907e47b4160eb30d1566217e9047fd5b8850cae`
