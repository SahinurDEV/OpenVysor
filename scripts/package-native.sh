#!/usr/bin/env bash
# Packs native/target/release into dist-release/. Run from the repository root
# after `cargo build --release`, with one of: windows-x64, linux-x64, macos-x64, macos-arm64.
set -euo pipefail

platform="$1"
version="$(sed -n 's/^version = "\(.*\)"$/\1/p' native/Cargo.toml | head -n 1)"
name="OpenVysor-$version-$platform"
out="dist-release"
stage="$out/$name"
built="native/target/release/openvysor-native"

mkdir -p "$stage"
cp LICENSE "$stage/LICENSE.txt"
cp NOTICE "$stage/NOTICE.txt"
cp native/resources/scrcpy-LICENSE.txt "$stage/"

case "$platform" in
  windows-*)
    cp "$built.exe" "$stage/OpenVysor.exe"
    (cd "$out" && 7z a -tzip "$name.zip" "$name" >/dev/null)
    ;;
  linux-*)
    cp native/resources/openh264-LICENSE.txt "$stage/"
    cp "$built" "$stage/openvysor"
    cp native/resources/icon.png "$stage/openvysor.png"
    cat > "$stage/openvysor.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=OpenVysor
Comment=Mirror and control Android phones
Exec=openvysor
Icon=openvysor
Terminal=false
Categories=Utility;
StartupWMClass=openvysor
DESKTOP
    tar -C "$out" -czf "$out/$name.tar.gz" "$name"
    ;;
  macos-*)
    cp native/resources/openh264-LICENSE.txt "$stage/"
    app="$stage/OpenVysor.app/Contents"
    mkdir -p "$app/MacOS" "$app/Resources"
    cp "$built" "$app/MacOS/OpenVysor"

    iconset="$out/OpenVysor.iconset"
    mkdir -p "$iconset"
    for size in 16 32 64 128 256 512; do
      sips -z "$size" "$size" native/resources/icon.png --out "$iconset/icon_${size}x${size}.png" >/dev/null
    done
    iconutil -c icns "$iconset" -o "$app/Resources/OpenVysor.icns"
    rm -rf "$iconset"

    cat > "$app/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>OpenVysor</string>
  <key>CFBundleDisplayName</key><string>OpenVysor</string>
  <key>CFBundleIdentifier</key><string>io.github.sahinurdev.openvysor</string>
  <key>CFBundleExecutable</key><string>OpenVysor</string>
  <key>CFBundleIconFile</key><string>OpenVysor</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$version</string>
  <key>CFBundleVersion</key><string>$version</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
    # Apple Silicon refuses to run a bundle with no signature at all.
    codesign --force --deep --sign - "$stage/OpenVysor.app"
    (cd "$out" && ditto -c -k --keepParent "$name" "$name.zip")
    ;;
  *)
    echo "unknown platform: $platform" >&2
    exit 1
    ;;
esac

rm -rf "$stage"
ls -l "$out"
