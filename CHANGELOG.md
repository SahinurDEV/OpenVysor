# Changelog

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
