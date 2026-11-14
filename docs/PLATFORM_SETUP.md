# Installing the Native App

Download the package for your computer from the [website](https://sahinurdev.github.io/OpenVysor/#install),
which detects it for you, or from the [Releases page](https://github.com/SahinurDEV/OpenVysor/releases), which keeps every version. Every platform also
needs a phone with USB debugging turned on, and Android's `adb` tool. On Windows and macOS, the
app offers an **Install ADB** button that downloads a fixed version from Google and checks it.

Real-device status for each platform is in [FEATURE_STATUS.md](FEATURE_STATUS.md).

## Windows 10 and 11

1. Run `OpenVysor-windows-x64-setup.exe` and follow the steps. It installs for your user only,
   so it needs no administrator rights, into `%LOCALAPPDATA%\Programs\OpenVysor`. It adds a
   Start menu shortcut and, if you tick it, a desktop shortcut.

   If you'd rather not install anything, `OpenVysor-windows-x64.exe` is the same app as a
   single portable file. The `.zip` holds that file with its license files.
2. If the app asks, choose **Install ADB**. To install it yourself instead, download
   [SDK Platform-Tools for Windows](https://developer.android.com/tools/releases/platform-tools)
   and unzip it into `%LOCALAPPDATA%\OpenVysor`, so that `adb.exe` ends up in
   `%LOCALAPPDATA%\OpenVysor\platform-tools`. The winget package `Google.PlatformTools` currently
   fails with a 404, because it points at a file Google no longer serves.

The app is not signed, so SmartScreen may warn. Choose **More info**, then **Run anyway**.

### Uninstalling on Windows

Open **Settings → Apps → Installed apps**, find **OpenVysor**, and choose **Uninstall**. You can
also run **Uninstall OpenVysor** in the install folder. The uninstaller closes the app first.

Your settings, workspaces, key mappings and the ADB copy the app downloaded are kept, so a
later reinstall picks up where you left off. Tick **Settings and downloaded ADB** in the
uninstaller to delete them too. Screenshots and recordings in your Pictures and Videos folders
are never deleted.

For a silent uninstall, run `"%LOCALAPPDATA%\Programs\OpenVysor\Uninstall OpenVysor.exe" /S`.
For the portable `.exe`, delete the file, and `%LOCALAPPDATA%\OpenVysor` if you want the
settings gone too.

## macOS 11 or later

1. Open `OpenVysor-macos-arm64.dmg` (Apple Silicon) or `OpenVysor-macos-x64.dmg` (Intel), and drag `OpenVysor.app` to Applications. A `.zip` of the app is also available.
2. The app is not notarized, so macOS blocks the first launch. Try opening it once, then go to
   **System Settings → Privacy & Security** and choose **Open Anyway**. Or run this in Terminal:

   ```sh
   xattr -dr com.apple.quarantine /Applications/OpenVysor.app
   ```

3. If the app asks, choose **Install ADB**. To install it yourself instead:
   `brew install android-platform-tools`

## Linux x64

On Debian and Ubuntu, install the package. This also brings in adb and its USB access rules:

```sh
sudo apt install ./OpenVysor-linux-x64.deb
```

On other distributions, install adb and your distribution's Android udev rules, then:

```sh
tar -xzf OpenVysor-linux-x64.tar.gz
cd OpenVysor-linux-x64
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
git tag v0.3.1
git push origin v0.3.1
```

GitHub Actions builds every platform and publishes the release with a `SHA256SUMS.txt`.
A tag with a suffix, such as `v0.3.1-rc.1`, is published as a pre-release.
