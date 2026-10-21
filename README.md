# TransPop

Lightweight, native Rust Microsoft Pinyin candidate preview experiment for Windows 10/11.

## Usage

Run `TransPop.exe` for normal background mode, or `TransPop.exe --debug` for a permanent debug bubble. Open Notepad, use Microsoft Pinyin to type `nihao` without committing.

**Exit:** Press **Ctrl+Alt+Q** or run `TransPop.exe --quit` from the EXE directory. If the hotkey is already in use, the `--quit` command still works. You can also end `TransPop.exe` via Task Manager.

Logs: `%LOCALAPPDATA%\TransPop\diagnostics.log`. To inspect last lines in PowerShell:

```powershell
Get-Content "$env:LOCALAPPDATA\TransPop\diagnostics.log" -Tail 30
```

This build only previews the first Chinese candidate. Translation service integration is not yet included. The Windows 10 native/UIA HWND retrieval method has been proven in an earlier diagnostic, but automatic discovery has **not** been verified to work yet on Windows 10 or 11. No OCR, text injection, or keylogging is used.
