# Changelog

## 0.4.0

### Added

- Workspaces: save the open windows under a name and reopen them, in place, with one click.
- Per-app quality and remembered window positions. Right-click an app to choose its quality.
- Key mapping: bind keyboard keys to spots on the screen, for games and repetitive taps.
- A Files page to browse the phone's shared storage, download, and upload by drag and drop.
- A Notifications page, off until turned on. Text is never saved, and an app opens to reply.
- A tray icon and Ctrl+Shift+O (Cmd+Shift+O on macOS) to bring the app forward. Windows and macOS only.
- Mirror all, for several phones at once.
- Use over Wi-Fi: one click moves a USB-connected phone to Wi-Fi with `adb tcpip`, which also
  works on Android 10 and older.
- The IP address tab lists phones found on the network, and addresses that worked before.
  Those addresses are tried again when the app starts, until you choose Disconnect.
- A Windows installer, `OpenVysor-windows-x64-setup.exe`. It needs no administrator rights,
  adds Start menu and optional desktop shortcuts, and an uninstaller that appears in
  Settings → Apps. The uninstaller can also remove settings and the downloaded ADB.

### Changed

- Page shortcuts are now Ctrl+1 to Ctrl+5, to include the two new pages.
- Pairing with a code now connects as well, without asking for the second port.
- An IP address without a port uses 5555. Failed Wi-Fi connections explain what to check.
- On Windows, the manual ADB instructions link to Google's download, because the winget
  package `Google.PlatformTools` now fails with a 404. The winget manifest no longer depends
  on it.

### Fixed

- Text copied on the computer before a phone window opened never reached that phone. It is
  now sent when the window opens.
- Use over Wi-Fi gives up after about 20 seconds when the phone cannot be reached, instead of
  several minutes, and puts the phone back in USB mode.
- A phone window no longer stays on "Connecting…" forever when the stream ends while it starts,
  for example when the phone cannot encode at the chosen resolution. It now shows the reason and
  a Reconnect button, and reconnecting works.
- Two quick reconnects no longer race each other into a "session exists" error.
- A phone that does not answer the details request no longer slows down noticing other phones
  being plugged in or unplugged.
- If adb is moved or deleted while the app runs, the app looks for it again instead of failing
  until restarted.
- Phone sound works on outputs that use integer samples, common on Linux.
- ADB unzipped by hand with an extra folder level, such as `platform-tools-latest-windows`, is
  now found, and no restart is needed after unzipping it.
- The Windows build, testing and architecture documents describe the native app instead of the
  old Tauri version.

## 0.3.1

### Added

- One-click ADB setup on Windows and macOS. It downloads a fixed version of Google's
  platform-tools and checks its SHA-256 before unpacking.
- Smooth, Balanced and Sharp quality presets.
- An "Open windows" list on the Devices page, and an "Open" badge on running apps.
- Ctrl+1, Ctrl+2 and Ctrl+3 to switch pages. In app search, Enter opens the first result and
  Esc clears the search.

### Changed

- Buttons and app tiles animate on hover.

## 0.3.0

### Added

- macOS and Linux builds, with OpenH264 for video decoding.
- Automated tests on Windows, Linux and macOS, and release packages built from version tags.

### Changed

- Child processes are cleaned up after a crash on macOS and Linux as well as Windows.
- ADB lookup and setup help now match each operating system.

## 0.2.0

### Added

- Native Rust app with a redesigned interface, with no browser engine.
- Phone-shaped windows, with one window per app on a virtual display.
- Wi-Fi pairing, clipboard sync, drag and drop, phone sound and MP4 recording.

### Changed

- Phone windows open faster, and each frame takes less work to display.
