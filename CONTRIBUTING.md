# Contributing

See [docs/WINDOWS_SETUP.md](docs/WINDOWS_SETUP.md) to build the project and
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for how it fits together.

Before opening a pull request, run:

```powershell
pnpm exec tsc --noEmit
pnpm exec vitest run
cd src-tauri; cargo test --lib
```

If you change the mirroring code, also run the real-device test described in
[docs/TESTING.md](docs/TESTING.md).

Only contribute code you wrote or have the right to submit. Do not include code, assets or
decompiled material from Vysor or any other proprietary product.
