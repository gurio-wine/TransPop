# TransPop / 翻译气泡

Rust + Win32 + UI Automation. Microsoft Pinyin first-candidate preview for Windows 10/11.

Prototype: the bubble **only displays the Chinese candidate**. It does not translate or type anything, and has no keyboard hooks, input method installation, screenshot capture or OCR.

## Build

GitHub Actions: **Build TransPop Rust Windows** → **Run workflow** → download `TransPop-Rust-win-x64` artifact and unzip it. Or build on Windows with `cargo build --release --target x86_64-pc-windows-msvc`.

## Run and test

Open PowerShell in the folder containing `TransPop.exe` and run:

```powershell
.\TransPop.exe --debug
```

You should immediately see a bubble at the upper-left, with **等待微软拼音候选…**. This bubble should stay visible while debugging. In Notepad, use Microsoft Pinyin to enter `nihao` without pressing Space. No mouse positioning needed. Candidate text should appear below the input method candidate window.

When it does not work, check the running process and scan log:

```powershell
Get-Process TransPop -ErrorAction SilentlyContinue
Get-Content "$env:LOCALAPPDATA\TransPop\diagnostics.log" -Tail 25
```

`--debug` displays status independent of successful candidate detection; normal mode displays a bubble only when a candidate was found. To stop the tool, use `taskkill /IM TransPop.exe /F`. Delete the EXE to uninstall; optional logs are at `%LOCALAPPDATA%\TransPop`.

This project targets Windows 10 and Windows 11; Windows 11 remains unverified. Build and real runtime behavior must still be tested on a Windows PC. No repair documents or changelogs are packaged.
