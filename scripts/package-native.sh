#!/usr/bin/env bash
# Packs native/target/release into dist-release/. Run from the repository root
# after `cargo build --release`, with one of: windows-x64, linux-x64, macos-x64, macos-arm64.
# File names carry no version, so the website can link to the latest release of each.
set -euo pipefail

platform="$1"
version="$(sed -n 's/^version = "\(.*\)"\r\{0,1\}$/\1/p' native/Cargo.toml | head -n 1)"
name="OpenVysor-$platform"
out="$PWD/dist-release"
stage="$out/$name"
built="$PWD/native/target/release/openvysor-native"

mkdir -p "$stage"
cp LICENSE "$stage/LICENSE.txt"
cp NOTICE "$stage/NOTICE.txt"
cp native/resources/scrcpy-LICENSE.txt "$stage/"

desktop_entry() {
  cat <<'DESKTOP'
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
}

build_deb() {
  local root="$out/deb"
  mkdir -p "$root/DEBIAN" "$root/usr/bin" "$root/usr/share/applications" \
    "$root/usr/share/icons/hicolor/32x32/apps" "$root/usr/share/doc/openvysor"
  install -m 755 "$built" "$root/usr/bin/openvysor"
  desktop_entry > "$root/usr/share/applications/openvysor.desktop"
  install -m 644 native/resources/icon.png "$root/usr/share/icons/hicolor/32x32/apps/openvysor.png"
  install -m 644 LICENSE NOTICE native/resources/scrcpy-LICENSE.txt native/resources/openh264-LICENSE.txt "$root/usr/share/doc/openvysor/"
  cat > "$root/DEBIAN/control" <<CONTROL
Package: openvysor
Version: $version
Section: utils
Priority: optional
Architecture: amd64
Depends: libc6 (>= 2.35), libasound2 | libasound2t64, libgl1
Recommends: adb, android-sdk-platform-tools-common
Maintainer: SahinurDEV <68515168+SahinurDEV@users.noreply.github.com>
Homepage: https://sahinurdev.github.io/OpenVysor/
Description: Mirror and control Android phones
 Free, open-source Android screen mirroring and control, with every
 phone app in its own window.
CONTROL
  dpkg-deb --root-owner-group --build "$root" "$out/$name.deb"
  rm -rf "$root"
}

build_dmg() {
  local folder="$out/dmg"
  mkdir -p "$folder"
  cp -R "$stage/OpenVysor.app" "$folder/"
  ln -s /Applications "$folder/Applications"
  cp "$stage"/*.txt "$folder/"
  # hdiutil sometimes reports a busy resource on a fresh runner.
  hdiutil create -volname OpenVysor -srcfolder "$folder" -ov -format UDZO "$out/$name.dmg" \
    || { sleep 5; hdiutil create -volname OpenVysor -srcfolder "$folder" -ov -format UDZO "$out/$name.dmg"; }
  rm -rf "$folder"
}

case "$platform" in
  windows-*)
    cp "$built.exe" "$stage/OpenVysor.exe"
    cp "$built.exe" "$out/$name.exe"
    (cd "$out" && 7z a -tzip "$name.zip" "$name" >/dev/null)
    ;;
  linux-*)
    cp native/resources/openh264-LICENSE.txt "$stage/"
    cp "$built" "$stage/openvysor"
    cp native/resources/icon.png "$stage/openvysor.png"
    desktop_entry > "$stage/openvysor.desktop"
    tar -C "$out" -czf "$out/$name.tar.gz" "$name"
    build_deb
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
    build_dmg
    ;;
  *)
    echo "unknown platform: $platform" >&2
    exit 1
    ;;
esac

rm -rf "$stage"
ls -l "$out"
