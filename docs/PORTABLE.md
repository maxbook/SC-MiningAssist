# SC-MiningAssist — portable Windows preview

1. Extract this entire archive into a folder.
2. Run SC-MiningAssist.exe on Windows 10/11 x64.
3. Click Start scanning, then return to Star Citizen in borderless windowed mode.
4. Display the RS signature in scan mode. The top-edge overlay shows estimated resources, rock counts or salvage panel counts.

Ctrl + Alt + F8 pauses/resumes. Double-click the tray icon to reopen the window; right-click for controls. Quit, the X, Alt+F4 and taskbar Close exit the program. SC-MiningAssist starts paused, has no installer, does not require administrator privileges and does not add itself to Windows startup.

English or French Windows OCR support must be installed. The interface is in English. If OCR is unavailable, open Windows Settings > Time & language > Language & region, install English or French language features with OCR support, then restart SC-MiningAssist. Windows editions differ; the OCR optional capability may need to be installed by your administrator.

If no result appears, confirm Star Citizen is the foreground app, display a complete readable RS number, and allow time for a fallback search after a large head movement. Exclusive fullscreen, motion blur, reflections and small HUD text can interfere. A hotkey already registered by another app can be replaced by the tray controls.

Results are estimates from community signature data, not the scanned composition. LOW / MEDIUM / HIGH is a mineral hint, not a promise that a ship can break a particular rock. Multiple matches stay ambiguous. Salvage has synthetic-image coverage and still needs in-game validation. Game updates can change signatures.

A confirmed result stays visible while successful reads continue; brief OCR misses are tolerated for five seconds. Pausing or switching away hides the overlay. Detection runs locally using Windows OCR. Normal operation never saves or sends screen images and never reads game memory or modifies game files.

The optional signatures.json beside the executable overrides the built-in catalog. Settings are stored in %LOCALAPPDATA%\SC-MiningAssist\settings.json. Older manual areas, rates and activity filters are reset to automatic behavior. To uninstall, quit and delete the extracted folder; the settings folder can also be removed.

The preview executable is unsigned. Source code, documentation and release checksums are supplied with the GitHub project. Retain LICENSE and THIRD_PARTY_NOTICES.txt when redistributing.

Independent community project, not affiliated with Cloud Imperium Games.
