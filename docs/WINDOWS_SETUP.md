# Windows Setup

## Using OpenVysor

You need Windows 10 or 11 x64, and ADB.

1. Run `OpenVysor-windows-x64-setup.exe`. It installs for your user only, with no administrator
   rights, and adds a Start menu shortcut. To remove it later, use **Settings → Apps →
   Installed apps**. Details are in [PLATFORM_SETUP.md](PLATFORM_SETUP.md#windows-10-and-11).
2. Install ADB, if you don't already have it: choose **Install ADB** in the app, or download
   [SDK Platform-Tools](https://developer.android.com/tools/releases/platform-tools) and unzip
   it into `%LOCALAPPDATA%\OpenVysor`.
3. On the phone, enable Developer options and turn on **USB debugging**.
4. Connect the phone with a USB data cable and tap **Allow** on the prompt.
5. In OpenVysor, choose **Mirror screen** or **Open an app**.

OpenVysor looks for `adb.exe` in this order: the `OPENVYSOR_ADB` environment variable,
`PATH`, `ANDROID_HOME` / `ANDROID_SDK_ROOT`, the default Android Studio folder, WinGet
packages, then `%LOCALAPPDATA%\OpenVysor\platform-tools` and the folders next to it.

Some manufacturers need an extra developer option before input works. On Xiaomi devices,
enable **USB debugging (Security settings)**.

## Building from source

| Tool | Version used |
| --- | --- |
| Rust (MSVC toolchain) | 1.99 |
| Visual Studio Build Tools with the C++ workload | 2019 or later |
| NSIS, only for the installer | 3.10 |

```powershell
cd native
cargo run             # development
cargo test            # unit tests
cargo build --release # native\target\release\openvysor-native.exe
```

To build the installer and the portable packages as the release does, run this from the
repository root in Git Bash, after the release build:

```sh
bash scripts/package-native.sh windows-x64
```

The packages are written to `dist-release/`. If `makensis` is not on the `PATH` or in
`C:\Program Files (x86)\NSIS`, set `MAKENSIS` to its full path.

## Upgrading scrcpy-server

The client and server versions must match exactly. To upgrade, replace
`native/resources/scrcpy-server`, change `SCRCPY_VERSION` in
`native/src/mirroring/protocol.rs`, and check the wire format against that release's source.

Bundled v4.1 SHA-256, matching the official GitHub release:
`deacb991ed2509715160ffdc7907e47b4160eb30d1566217e9047fd5b8850cae`
