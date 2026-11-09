# Contributing to OpenVysor

Thanks for helping. The most useful contributions right now are:

- **Testing on real hardware.** Try a feature marked "being tested" in
  [docs/FEATURE_STATUS.md](docs/FEATURE_STATUS.md), especially on a Mac, a Linux PC or a phone
  from another manufacturer, and open an issue with what happened.
- **Bug reports** with your operating system, phone model and Android version.
- **Pull requests** for bugs and for items on the [roadmap](README.md#roadmap).

For larger changes, please open an issue first so we can agree on the approach.

## Setting up

The current app is in `native/`. Install [Rust](https://rustup.rs), and on Linux also install
`libasound2-dev`, `nasm` and `pkg-config`.

```sh
cd native
cargo build
cargo test
```

Before opening a pull request, make sure `cargo test` passes. CI runs it on Windows, Linux and
macOS.

If you change mirroring, decoding or input, also run the real-device tests with one authorized
phone connected, and say in the pull request which phone you used:

```sh
cargo test --release --test real_device -- --ignored --nocapture
```

Please keep phone serial numbers, personal screenshots and other private device data out of
issues, logs and commits.

## Ground rules

- Only contribute code you wrote or have the right to submit. Do not include code, assets or
  decompiled material from Vysor or any other proprietary product.
- Report results honestly. A feature counts as confirmed only after it has worked on real
  hardware, and mocked tests must be labeled as mocked.
- Never pass text from the interface into a shell command without validating it.
- Follow the [Code of Conduct](CODE_OF_CONDUCT.md).

By contributing, you agree that your work is licensed under the [Apache License 2.0](LICENSE).

## Releasing

Set `version` in `native/Cargo.toml`, commit it to `main`, then push a matching tag such as
`v0.3.0`. GitHub Actions builds every platform and publishes the release. A tag with a suffix,
such as `v0.3.0-rc.1`, is published as a pre-release.
