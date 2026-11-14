# Flatpak

`io.github.SahinurDEV.OpenVysor.yml` builds OpenVysor from source with the freedesktop 26.08
runtime and the Rust SDK extension. It is the first step toward a Flathub listing.

## Build and run locally

Install `flatpak` and `flatpak-builder`, then the runtime, from the repository root:

```sh
flatpak remote-add --if-not-exists --user flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install --user -y flathub org.freedesktop.Platform//26.08 org.freedesktop.Sdk//26.08 \
    org.freedesktop.Sdk.Extension.rust-stable//26.08
flatpak-builder --user --install --force-clean flatpak-build \
    packaging/flatpak/io.github.SahinurDEV.OpenVysor.yml
flatpak run io.github.SahinurDEV.OpenVysor
```

The build is offline. `cargo-sources.json` lists every crate with the checksum from
`native/Cargo.lock`. After changing dependencies, write it again:

```sh
python3 packaging/flatpak/cargo-sources.py
```

## ADB inside the sandbox

The sandbox cannot see an `adb` installed on the computer, and Google's platform-tools are not
bundled. On first start, choose **Install ADB**. The app downloads Google's package, checks its
SHA-256 and keeps it in `~/.var/app/io.github.SahinurDEV.OpenVysor/data/OpenVysor`.

If an adb server is already running on the computer, for example from Android Studio, the
sandboxed adb uses it through `localhost:5037`. Both should be the same adb version.

## Phones over USB

`--device=all` gives the sandbox `/dev/bus/usb`. The computer must still let your user open
the phone's USB device, as for any adb use on Linux. On Debian and Ubuntu, install the udev
rules once:

```sh
sudo apt install android-sdk-platform-tools-common
```

Then reconnect the phone and tap **Allow** on it.

## Permissions

| Permission | Why |
| --- | --- |
| `--device=all` | OpenGL drawing and USB access to phones. Flatpak has no narrower USB permission that adb can use. |
| `--share=network` | adb's server on `localhost:5037`, phones over Wi-Fi, and the optional ADB download |
| `--socket=x11`, `--share=ipc` | Windows. The clipboard library uses X11, so windows use X11 or XWayland too, or copy and paste would not work on Wayland. |
| `--socket=pulseaudio` | Phone sound |
| `--filesystem=xdg-download/OpenVysor:create` | Files downloaded from the phone |
| `--filesystem=xdg-pictures/OpenVysor:create` | Screenshots |
| `--filesystem=xdg-videos/OpenVysor:create` | Recordings |

The tray icon and global shortcut are Windows and macOS only, so no D-Bus names are needed.
