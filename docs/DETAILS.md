# Detection, limitations and diagnostics

A lightweight local mining and salvage assistant for Star Citizen. It reads the RS digits on screen and overlays an estimate of the dominant resource and rock count, or the number of salvage panels. Native Win32 interface, Windows OCR, no embedded browser or cloud service. All application text and bundled mineral names are in English.

## Getting started

1. Extract the release archive and run **SC-MiningAssist.exe** on Windows 10/11 x64.
2. Click **Start scanning**. The window minimizes; mining and salvage detection are automatic.
3. Return to Star Citizen in **borderless windowed mode** and display an RS signature in scan mode.

No calibration or options are required. A lightweight pixel filter finds plausible complete HUD text runs, and Windows OCR reads those small regions. The scanner area is prioritized, with a wider envelope covering **80% of the game window's width**, between **8% and 78% of its height**, to allow head movement. If the fast path cannot read the text, contextual grayscale/threshold OCR and overlapping tiles remain available as fallback.

Tracking runs at up to **8 reads per second**; broad visual acquisition runs at up to **4 per second**. Expensive full OCR searches are limited to once per second. Long OCR operations never create a backlog of overdue scans. Results notify the overlay immediately instead of waiting for its maintenance timer. The pixel filter does not guess digits or resources.

Two identical successful readings within two seconds confirm a new signature. A brief unreadable frame between them does not reset confirmation; a different readable value does. Tracking relocates the full text run each time, retaining surrounding context so a changing number or newly visible unit is not clipped out of the OCR input.

A confirmed signature remains visible while successful readings continue. Brief OCR misses are tolerated for **five seconds after the last successful reading**. A different readable signature, multiple ambiguous signals, or an explicitly recognized measurement at the tracked location invalidates the previous result. Pausing or switching away from the game hides the overlay.

## Minimal interface

The compact dark control window contains only the current status, **Start scanning / Pause scanning**, and **Quit**. Detection mode, area, rate and opacity are automatic. **Ctrl + Alt + F8** toggles pause/resume. Double-click the crystal tray icon to reopen the window; right-click for the controls or **Quit SC-MiningAssist**.

The overlay remains a single click-through line: resource, rock count, and **LOW / MEDIUM / HIGH** in green, gold or red; salvage shows **SALVAGE · N PANELS**. It is **28 px tall** at 100% Windows scaling, centered against the monitor's top edge, with cyan/gold text and cut corners. No animations, focus stealing or additional information line. Unidentified ROC/FPS deposits and salvage are not assigned mineral difficulty.

**Quit**, the X button, Alt+F4 and taskbar Close all exit the program. **Start scanning** keeps it active in the background. Launching again reopens the existing window. The program starts paused, does not add itself to Windows startup, and does not require administrator privileges.

Legacy preferences in **%LOCALAPPDATA%\\SC-MiningAssist\\settings.json** no longer constrain detection: old manual areas, activity filters, refresh settings and opacity choices are reset to the automatic policy when loading. The custom mineral catalog remains supported.

## What the signature tells us

For a presumed homogeneous cluster: **total RS = base RS × rock count**.

Example: **10800 = 3600 × 3 → Bexalite · 3 rocks**. This estimates the dominant resource, not the full composition or percentages. Multiple matches remain ambiguous; automatic mode does not choose one arbitrarily. If a search detects multiple distinct signals, the overlay stays hidden until a unique reading is found.

For salvage panels: **total RS = 2000 × panel count**. Example: **8000 → Salvage · 4 panels**. Source: [community extraction of 4.7 signatures](https://github.com/Diftic/SC_Signature_Scanner/blob/master/SignatureValue.md#salvage), checked November 15, 2026. This rule covers the corresponding panels and hull pieces; it does not identify an entire ship or its RMC quantity. Wrecks with other signatures are not identified yet.

The application automatically searches for mining rocks and salvage panels. For example, **18000** can produce **Bexalite ×5 / Salvage ×9 · ?**: the number alone cannot distinguish them. The diagnostic command line still supports explicit mining, salvage, ROC and FPS filters; these are not exposed as application options.

ROC/FPS modes show deposit counts only, without guessing the gem type. Distances with units, rounded values such as **10.8K**, incomplete readings and unmatched numbers are ignored in automatic mode.

Difficulty is a **mineral-level estimate**, not confirmation that a particular rock can be fractured. It takes the higher relative resistance/instability rating from the [SCMINER community mineral directory](https://scminer.rocks/data/mineral-directory), checked November 15, 2026, and groups those ratings into three categories. This grouping is specific to SC-MiningAssist. The table and source are in [data/mining-difficulty.json](../data/mining-difficulty.json). Cluster count does not change the difficulty of an individual rock. Actual mass, secondary composition, scanned resistance, laser head and modules are still needed to judge feasibility. A custom resource with no known properties displays **?**.

The mineral catalog in [data/signatures.json](../data/signatures.json) is a community 4.7+ reference from [STARVEIN](https://starvein.app/en/signatures), checked November 15, 2026. It is not in-game validation of the current patch. Search limits are 12 rocks (**max_rocks**) and 100 panels (**max_panels**); these are application limits, not guaranteed in-game cluster sizes. **salvage_panel_rs** sets the panel base signature. Place a modified **signatures.json** beside the executable and restart to override the embedded catalog. Older catalogs without salvage fields receive the defaults. Custom names are displayed exactly as supplied.

## Limitations

- Mining was reported working in game during development. Local checks cover the development screenshot, simulated HUD translations, 1440p/1080p reductions and control images. **Salvage has been tested on synthetic images and still needs in-game validation.** Simulated movement does not cover every cockpit perspective, TrackIR or motion blur.
- Tiny, distorted or obscured text, reflections, HDR and other HUD numbers can affect OCR. A stable reading and mathematical match do not guarantee correct target identification.
- Reacquiring a signature after a large movement can take several seconds. A signature outside the search envelope or screen cannot be read.
- Borderless windowed mode is recommended: an external Win32 overlay is not reliable over exclusive fullscreen.
- Windows OCR must be installed for English or French. The interface stays English regardless of the OCR language. Missing OCR produces an explicit error. Unpackaged operation has been verified on a development PC, but Microsoft documents official support around package identity; compatibility with every Windows installation is not guaranteed.
- Game-window size changes reset tracking automatically. Unusual cockpit layouts can still require reacquisition through the wider fallback.

## Lightweight operation

No Python, Electron, WebView, downloadable model or .NET runtime dependency. The CRT is statically linked; the executable uses Windows system libraries. **dist** is the portable deliverable. **.tools**, **target** and **artifacts** are development-only folders and are not distributed.

GDI captures pixels into memory; Windows OCR runs on a worker thread. No game-memory access, injection or changes to Star Citizen files. The foreground process name is only used to suspend capture outside the game. During normal use, images are neither saved nor transmitted. The live overlay is excluded from capture to prevent reading its own numbers.

Local measurements on the regression images: tracking **1–15 ms**, small-motion recentering about **1–2 ms** on synthetic fixtures, first search on real cockpit screenshots **20–190 ms** depending on resolution and fallback. Expensive unsuccessful searches can still take several hundred milliseconds.

On the same simulated sequence, a signal appearing just after a search was confirmed **250 ms after appearing**, versus **1,000 ms** with the previous version. The previous tracker also kept accepting a tracked number after it became a distance; the new context check clears that result. These are offline regression results, not measured in-game latency or FPS guarantees. Resolution, scene, motion and hardware affect results.

## Development

Recent stable Rust (built with 1.100), target **x86_64-pc-windows-msvc**, Visual Studio C++ tools and Windows SDK. Dependencies are pinned in **Cargo.lock**.

~~~powershell
.\build.ps1          # portable release in dist/
.\build.ps1 -Test    # parsing, calculations, ambiguity and stabilization
.\build.ps1 -Lint    # Clippy with warnings denied
.\tests\verify-ocr.ps1 -ImagePath 'C:\path\mining.jpg'
.\tests\verify-lifecycle.ps1  # app closed: reopen, quit, tray menu, double launch
.\tests\verify-hud.ps1        # compact UI, geometry, Win32 styles, rendering, icon
.\tests\verify-tracking.ps1   # motion, dim text, misses and target changes
~~~

The OCR script expects the original 5120 × 2880 screenshot containing RS 10800. It writes fixtures and reports to **artifacts** (not distributed). An optional project-local toolchain in **.tools** is detected by **build.ps1**; a standard rustup installation works as well.

To diagnose an image:

~~~powershell
# Automatic search and verification of the small tracked rectangle
Start-Process .\dist\SC-MiningAssist.exe -ArgumentList '--ocr-file "C:\path\capture.jpg" --output "C:\path\report.json"' -WindowStyle Hidden -Wait
# Read a precise x,y,width,height rectangle
Start-Process .\dist\SC-MiningAssist.exe -ArgumentList '--ocr-file "C:\path\capture.jpg" --rect 2540,928,105,45 --output "C:\path\report.json"' -WindowStyle Hidden -Wait
~~~

Diagnostic commands write only the requested report. They do not modify settings or source images. The default activity is Auto; use **--mode auto|mining|salvage|roc|fps** to select another.

**--ocr-sequence MANIFEST.json --output REPORT.json** runs the application's tracker on a list of image entries such as **{ "image": "absolute path", "at_ms": 250 }**, using a simulated clock. It does not capture the screen. Sequence checks cover appearance latency, small/large head movements, dim text, complete-number tracking, newly appearing distance units, multiple signals, target expiry and real cockpit images at several resolutions. Add **--fast** to **--ocr-file** to inspect the visual proposal path without the full OCR fallback.

Technical references: [Windows OCR](https://learn.microsoft.com/en-us/uwp/api/windows.media.ocr), [OCR coordinates and rotation](https://learn.microsoft.com/en-us/uwp/api/windows.media.ocr.ocrresult.textangle), [click-through windows](https://learn.microsoft.com/en-us/windows/win32/winmsg/window-features).

Independent community project, not affiliated with Cloud Imperium Games.
