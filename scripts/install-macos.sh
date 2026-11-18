#!/bin/bash
# Installs the latest OpenVysor on macOS:
#   curl -fsSL https://raw.githubusercontent.com/SahinurDEV/OpenVysor/main/scripts/install-macos.sh | bash
# Files fetched by curl carry no quarantine flag, so macOS opens the app without the
# "could not verify" prompt that a browser download gets. The zip is checked against the
# release's SHA256SUMS.txt before anything is installed.
set -euo pipefail

repo="SahinurDEV/OpenVysor"
base="https://github.com/$repo/releases/latest/download"

if [ "$(uname -s)" != "Darwin" ]; then
  echo "This installer is for macOS." >&2
  exit 1
fi

# Rosetta reports x86_64, so ask the hardware rather than the running shell.
if [ "$(sysctl -n hw.optional.arm64 2>/dev/null || echo 0)" = "1" ]; then
  platform="macos-arm64"
else
  platform="macos-x64"
fi
zip="OpenVysor-$platform.zip"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "Downloading $zip..."
curl -fL --progress-bar -o "$tmp/$zip" "$base/$zip"
curl -fsSL -o "$tmp/SHA256SUMS.txt" "$base/SHA256SUMS.txt"

expected="$(awk -v f="$zip" '$2 == f || $2 == "*" f { print $1 }' "$tmp/SHA256SUMS.txt")"
actual="$(shasum -a 256 "$tmp/$zip" | awk '{ print $1 }')"
if [ -z "$expected" ] || [ "$expected" != "$actual" ]; then
  echo "Checksum mismatch for $zip; nothing was installed." >&2
  exit 1
fi

ditto -x -k "$tmp/$zip" "$tmp/unpacked"
app="$tmp/unpacked/OpenVysor-$platform/OpenVysor.app"
[ -d "$app" ] || { echo "OpenVysor.app not found in $zip" >&2; exit 1; }

dest="/Applications"
if [ ! -w "$dest" ]; then
  dest="$HOME/Applications"
  mkdir -p "$dest"
fi

# Close a running copy first, and wait for it, or reopening the update fails.
if pgrep -xq OpenVysor; then
  osascript -e 'quit app "OpenVysor"' >/dev/null 2>&1 || true
  for _ in $(seq 1 20); do pgrep -xq OpenVysor || break; sleep 0.5; done
fi
rm -rf "$dest/OpenVysor.app"
ditto "$app" "$dest/OpenVysor.app"
xattr -dr com.apple.quarantine "$dest/OpenVysor.app" 2>/dev/null || true

echo "Installed to $dest/OpenVysor.app"
open "$dest/OpenVysor.app"
