# Installing the Native App

Download the package for your computer from the
[Releases page](https://github.com/SahinurDEV/OpenVysor/releases). Every platform also
needs Android's `adb` tool and a phone with USB debugging turned on.

Real-device status for each platform is in [FEATURE_STATUS.md](FEATURE_STATUS.md).

## Windows 10 and 11

1. Install adb: `winget install Google.PlatformTools`
2. Unzip `OpenVysor-<version>-windows-x64.zip` and run `OpenVysor.exe`.

The app is not signed, so SmartScreen may warn. Choose **More info**, then **Run anyway**.

## macOS 11 or later

1. Install adb: `brew install android-platform-tools`
2. Unzip `macos-arm64` (Apple Silicon) or `macos-x64` (Intel), and move `OpenVysor.app` to Applications.
3. The app is not notarized, so macOS blocks the first launch. Allow it once with:

   ```sh
   xattr -dr com.apple.quarantine /Applications/OpenVysor.app
   ```

## Linux x64

1. Install adb. On Debian and Ubuntu: `sudo apt install adb`
2. Allow USB access to phones. On Debian and Ubuntu: `sudo apt install android-sdk-platform-tools-common`
3. Install the app:

   ```sh
   tar -xzf OpenVysor-*-linux-x64.tar.gz
   cd OpenVysor-*-linux-x64
   install -Dm755 openvysor ~/.local/bin/openvysor
   install -Dm644 openvysor.png ~/.local/share/icons/hicolor/32x32/apps/openvysor.png
   install -Dm644 openvysor.desktop ~/.local/share/applications/openvysor.desktop
   ```

The package needs glibc 2.35 or later, X11 or Wayland, and OpenGL.

## Building from source

Install [Rust](https://rustup.rs), then run `cargo build --release` in `native/`. On Linux,
first install `libasound2-dev`, `nasm` and `pkg-config`. Installing `nasm` on Linux and on
Intel Macs makes video decoding faster.

## Publishing a release

Set `version` in `native/Cargo.toml`, commit it, then push a matching tag:

```sh
git tag v0.3.0
git push origin v0.3.0
```

GitHub Actions builds every platform and publishes the release with a `SHA256SUMS.txt`.
A tag with a suffix, such as `v0.3.0-rc.1`, is published as a pre-release.
