# TransPop / 翻译气泡 — Rust v0.6

A **Rust-only**, non-invasive Microsoft Pinyin first-candidate preview bubble for Windows 10/11.
No C#, .NET, WPF, TypeScript, Tauri or Electron. No installer, driver or IME registration.

> **Prototype**: displays the original Chinese first candidate in the bubble to verify automatic discovery. Translation with Bing / custom AI will be added after discovery is verified. Does not insert text or modify the candidate list.

## How to build without installing Rust

1. Create a GitHub repository (e.g., `TransPop`), and upload these files **including** `.github/workflows/windows.yml`.
2. Open **Actions → Build TransPop Rust Windows → Run workflow**.
3. After success, download artifact `TransPop-Rust-win-x64`, unzip twice and run `TransPop.exe`.
4. In **Microsoft Pinyin** in Notepad, type `nihao` without hitting Space. The candidate bubble should appear **without touching the mouse**.

For local build: `cargo build --release --target x86_64-pc-windows-msvc` on Windows with Rust MSVC tools.

## Changes vs C# prototype

- Scans **top-level AND child HWNDs**. Win10 Build 19045 uses `Windows.UI.Core.CoreWindow` of `TextInputHost` nested inside `ApplicationFrameWindow`. **Enumerating top-level windows alone misses it.**
- Only inspects visible, plausible-sized `CoreWindow` / IME windows; UIA is queried using `ElementFromHandle`, never `ElementFromPoint`.
- Reads UIA **Raw View** and both `IME_Prediction_Window` and `IME_Candidate_Window`.
- Reads first `ListItem.Name` and panel bounding rectangle, positions the preview just beneath it.
- UIA is on a dedicated background MTA thread. Bubble is Win32/GDI, transparent to input, always on top, not activated.
- **No keyboard hooks, no screenshot, no OCR, no input modification, no network requests.**

## Diagnostic mode

Run from PowerShell:

```powershell
.\TransPop.exe --debug
```

Debug mode shows a small fixed **status bubble** even when no candidate is found.
The status line reports `Top`, `Child`, `Core`, `UIA`, `Panel`, `Err`.

A log is also written every ~5 seconds to:

```
%LOCALAPPDATA%\TransPop\diagnostics.log
```

To view it:

```powershell
Get-Content "$env:LOCALAPPDATA\TransPop\diagnostics.log" -Tail 20
```

If the program remains waiting, send the status line and last 20 log lines. **Candidate text itself is never stored in the log.**

To exit (prototype has no tray UI yet): close from Task Manager or run:

```powershell
taskkill /IM TransPop.exe /F
```

To uninstall, exit and delete the program exe and `%LOCALAPPDATA%\TransPop` if you also wish to delete logs. No registry keys are created.

## Compatibility / limits

- The exact UIA tree has been **confirmed on one Win10 Build 19045 system** using the older C# diagnostic, but **this new Rust automatic scanner has not yet been built or run on a Windows computer**.
- Windows 11 is a compatibility target, not yet verified.
- UI Automation can be slow or unavailable with some protected applications/secure desktop; this program does not bypass those boundaries.
- Using an input method other than Microsoft Pinyin is not guaranteed to work.
- No translation engine or settings UI yet. First objective is to eliminate permanent “waiting for input”.

## Implementation notes

- `src/scanner.rs`: EnumWindows + EnumChildWindows; RawView walker; candidate extraction
- `src/main.rs`: dedicated MTA scanner, Win32 click-through bubble, privacy-preserving log
- `.github/workflows/windows.yml`: Windows MSVC GitHub Actions release EXE artifact

The Win10 observed UIA panel ID is `IME_Prediction_Window`; Microsoft also documents `IME_Candidate_Window` for conversion candidates.
