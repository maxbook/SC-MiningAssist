# Contributing to MiningMod

Keep MiningMod small, local and unobtrusive. The app should remain a native Rust/Win32 utility with an English UI, automatic detection and a one-line overlay.

## Build

Use Windows x64, rustup, Visual Studio C++ Build Tools and a Windows SDK. The pinned toolchain in rust-toolchain.toml includes rustfmt and Clippy. Run from the repository root:

```powershell
.\build.ps1 -Format
.\build.ps1 -Lint
.\build.ps1 -Test
.\build.ps1
```

The build script discovers the MSVC linker and SDK. A standard rustup installation works; an optional .tools/cargo and .tools/rustup installation is also supported. No private SDK, game installation or screenshot is needed to compile. Generated binaries and test output are ignored by Git.

## Checks that need a Windows desktop

Close MiningMod first. Install an English or French Windows OCR language, then run:

```powershell
.\tests\verify-panel.ps1
.\tests\verify-hud.ps1
.\tests\verify-lifecycle.ps1
.\tests\verify-tracking.ps1
```

The panel and HUD checks use PrintWindow on MiningMod's own windows. They do not start a desktop capture session. The lifecycle checks exercise single-instance startup, tray-menu shutdown and taskbar Close. Tracking tests generate their own synthetic images, cover target changes and distance rejection, and write reports under artifacts/.

If artifacts/original.png, head-left.png, head-right-down.png, 1440p.png or 1080p.png exist, tracking checks also replay them as optional cockpit fixtures expecting RS 10800. These personal game screenshots are not part of the repository. The separate verify-ocr.ps1 script needs a supplied 5120×2880 screenshot with RS 10800; it is not a clean-checkout prerequisite.

GitHub CI checks formatting, Clippy, unit tests, release compilation and packaging. Desktop/OCR checks are local integration checks because they require an interactive Windows session and OCR language features. Do not describe CI alone as in-game validation.

## Code map

| File | Purpose |
| --- | --- |
| src/capture.rs | GDI capture, game window and foreground checks |
| src/vision.rs, src/ocr.rs | Text proposals, Windows OCR and numeric parsing |
| src/tracker.rs, src/worker.rs | Tracking, fallback cadence and worker scheduling |
| src/core.rs, src/difficulty.rs | Catalog matching, stabilization and difficulty |
| src/ui.rs, src/panel.rs, src/hud.rs | Native controls, tray, custom painting and overlay |
| data/ | Community reference data with provenance |

Native messages can reenter while controls update. Painting and shutdown must not retain or recursively borrow the application RefCell. Keep capture/OCR off the UI thread. Do not turn an ambiguous signature into a unique resource or infer ship capability from mineral ratings.

For detection fixes, add a focused failing case using synthetic images or a minimal parser/tracker test. Describe the game patch, resolution, Windows scaling and the before/after result. Do not commit personal screenshots, logs, credentials, settings or game assets.

## Icon and dependencies

Edit assets/miningmod.svg, then run assets/build-icon.ps1 on Windows. The script generates the committed PNG and ICO at 16–256 px; System.Drawing is a build-time convenience, not an application dependency.

Keep Cargo.lock committed. When changing dependencies, review their license metadata and regenerate notices with scripts/update-notices.ps1. Commit the updated THIRD_PARTY_NOTICES.txt. Contributions are accepted under the repository's MIT license.
