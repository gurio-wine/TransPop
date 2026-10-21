#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod scanner;

use std::{
    fs::{self, OpenOptions},
    io::Write,
    mem,
    path::PathBuf,
    ptr,
    sync::{mpsc, Mutex, OnceLock},
    thread,
    time::{Duration, Instant},
};

use scanner::{Candidate, ScanReport};
use uiautomation::core::UIAutomation;
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{
        BeginPaint, CreateRoundRectRgn, CreateSolidBrush, DeleteObject, DrawTextW,
        EndPaint, GetStockObject, InvalidateRect, RoundRect, SelectObject, SetBkMode,
        UpdateWindow,
        SetTextColor, SetWindowRgn, PAINTSTRUCT, DEFAULT_GUI_FONT, TRANSPARENT,
        DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER,
    },
    System::LibraryLoader::GetModuleHandleW,
    UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT},
    UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
        GetMessageW, GetSystemMetrics, FindWindowW, PostMessageW, PostQuitMessage,
        RegisterClassW, SetLayeredWindowAttributes, SetProcessDPIAware, SetTimer,
        SetWindowPos, ShowWindow, TranslateMessage, WNDCLASSW, MSG,
        HWND_TOPMOST, LWA_ALPHA, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
        SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_HIDE, SW_SHOWNOACTIVATE,
        SWP_NOACTIVATE, SWP_SHOWWINDOW, WM_CLOSE, WM_DESTROY, WM_ERASEBKGND, WM_HOTKEY,
        WM_NCHITTEST, WM_PAINT, WM_TIMER, WS_EX_LAYERED, WS_EX_NOACTIVATE,
        WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
    },
};

const WIDTH: i32 = 392;
const HEIGHT: i32 = 102;
const TICK_ID: usize = 1;
const EXIT_HOTKEY_ID: i32 = 0x3452;
const TICK_MS: u32 = 150;

static RECEIVER: OnceLock<Mutex<mpsc::Receiver<ScanReport>>> = OnceLock::new();
static STATE: OnceLock<Mutex<State>> = OnceLock::new();
static STARTED: OnceLock<Instant> = OnceLock::new();

#[derive(Default)]
struct State {
    debug: bool,
    shown: bool,
    text: String,
    status: String,
    candidate: Option<Candidate>,
}

fn rgb(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn log_file() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("TransPop").join("diagnostics.log")
}

fn log_status(message: &str) {
    let path = log_file();
    if let Some(parent) = path.parent() { let _ = fs::create_dir_all(parent); }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{message}");
    }
}

fn spawn_scanner(sender: mpsc::Sender<ScanReport>) {
    thread::spawn(move || {
        let uia = match UIAutomation::new() {
            Ok(x) => x,
            Err(e) => {
                log_status(&format!("UIA initialization failed: {e}"));
                return;
            }
        };
        let walker = match uia.get_raw_view_walker() {
            Ok(x) => x,
            Err(e) => {
                log_status(&format!("UIA RawView walker failed: {e}"));
                return;
            }
        };
        log_status("UIA initialized; scanning top and child HWNDs");
        let mut last_log = Instant::now() - Duration::from_secs(10);
        loop {
            let start = Instant::now();
            let report = scanner::scan(&uia, &walker);
            if last_log.elapsed() >= Duration::from_secs(5) {
                // Do NOT log candidate text: it can include private information.
                log_status(&format!(
                    "scan: top={} child={} hosts={} core_seen={} core_visible={} caret={} caret_hits={} uia={} errors={} panels={} offscreen={} found={} elapsed_ms={} likely_hwnds=[{}]",
                    report.top_windows, report.child_windows, report.core_windows,
                    report.core_seen, report.core_visible, report.caret_available, report.caret_hits,
                    report.uia_checked, report.uia_errors, report.panels, report.offscreen_panels,
                    report.candidate.is_some(), start.elapsed().as_millis(), report.candidate_hints
                ));
                last_log = Instant::now();
            }
            if sender.send(report).is_err() { break; }
            thread::sleep(Duration::from_millis(350));
        }
    });
}

fn fmt_status(report: &ScanReport) -> String {
    format!(
        "Top {} | Core {} | UIA {} | Panel {} | Caret {}",
        report.top_windows, report.core_seen,
        report.uia_checked, report.panels, report.caret_hits
    )
}

unsafe fn update(hwnd: HWND) {
    let Some(receiver) = RECEIVER.get() else { return; };
    let mut latest: Option<ScanReport> = None;
    if let Ok(rx) = receiver.lock() {
        while let Ok(report) = rx.try_recv() { latest = Some(report); }
    }
    let Some(global) = STATE.get() else { return; };
    let Ok(mut state) = global.lock() else { return; };
    let previous_text = state.text.clone();
    let previous_status = state.status.clone();
    if let Some(report) = latest {
        state.status = fmt_status(&report);
        state.candidate = report.candidate;
        state.text = match &state.candidate {
            Some(candidate) => candidate.text.clone(),
            None => "等待微软拼音候选…".to_string(),
        };
    } else if state.debug {
        // The UI must remain responsive even if a cross-process UIA query hangs.
        let seconds = STARTED.get().map(|t| t.elapsed().as_secs()).unwrap_or(0);
        if seconds >= 3 && state.candidate.is_none() {
            state.status = format!("尚无扫描结果 · {} 秒 · 请检查日志", seconds);
        }
    }

    let showing = state.candidate.is_some() || state.debug;
    if showing {
        let (mut x, mut y) = match &state.candidate {
            Some(c) => (c.x, c.bottom + 9),
            None => (24, 24),
        };
        let vx = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
        let vy = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
        let vw = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
        let vh = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
        if let Some(c) = &state.candidate {
            if y + HEIGHT > vy + vh { y = c.y - HEIGHT - 9; }
        }
        x = x.clamp(vx, (vx + vw - WIDTH).max(vx));
        y = y.clamp(vy, (vy + vh - HEIGHT).max(vy));
        unsafe {
            SetWindowPos(hwnd, HWND_TOPMOST, x, y, WIDTH, HEIGHT,
                SWP_NOACTIVATE | SWP_SHOWWINDOW);
            if !state.shown { ShowWindow(hwnd, SW_SHOWNOACTIVATE); }
            if !state.shown || previous_text != state.text || previous_status != state.status {
                InvalidateRect(hwnd, ptr::null(), 1);
            }
        }
    } else if state.shown {
        unsafe { ShowWindow(hwnd, SW_HIDE); }
    }
    state.shown = showing;
}

unsafe fn paint(hwnd: HWND) {
    let mut ps: PAINTSTRUCT = unsafe { mem::zeroed() };
    let dc = unsafe { BeginPaint(hwnd, &mut ps) };
    if dc.is_null() { return; }
    let (body, debug_line) = STATE.get().and_then(|s| s.lock().ok()).map(|s| {
        (s.text.clone(), if s.debug { s.status.clone() } else { "仅预览 · 不修改输入".into() })
    }).unwrap_or_else(|| ("TransPop".into(), String::new()));

    unsafe {
        let brush = CreateSolidBrush(rgb(251, 252, 255));
        let rim = CreateSolidBrush(rgb(214, 227, 247));
        let old_brush = SelectObject(dc, rim);
        RoundRect(dc, 0, 0, WIDTH - 1, HEIGHT - 1, 24, 24);
        SelectObject(dc, brush);
        RoundRect(dc, 1, 1, WIDTH - 2, HEIGHT - 2, 24, 24);
        SelectObject(dc, old_brush);
        DeleteObject(brush);
        DeleteObject(rim);
        SetBkMode(dc, TRANSPARENT as i32);
        let font = GetStockObject(DEFAULT_GUI_FONT);
        let previous_font = SelectObject(dc, font);

        SetTextColor(dc, rgb(58, 111, 201));
        draw_text(dc, "●  TransPop · 输入法候选预览", RECT { left: 17, top: 8, right: WIDTH-16, bottom: 32 });
        SetTextColor(dc, rgb(27, 37, 54));
        draw_text(dc, &body, RECT { left: 17, top: 34, right: WIDTH-16, bottom: 66 });
        SetTextColor(dc, rgb(120, 133, 154));
        draw_text(dc, &debug_line, RECT { left: 17, top: 73, right: WIDTH-16, bottom: 95 });
        SelectObject(dc, previous_font);
        EndPaint(hwnd, &ps);
    }
}

unsafe fn draw_text(dc: windows_sys::Win32::Graphics::Gdi::HDC, s: &str, mut rect: RECT) {
    let u16s: Vec<u16> = s.encode_utf16().collect();
    unsafe {
        DrawTextW(dc, u16s.as_ptr(), u16s.len() as i32, &mut rect,
            DT_LEFT | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX);
    }
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_TIMER if w == TICK_ID => { unsafe { update(hwnd) }; 0 }
        WM_HOTKEY if w == EXIT_HOTKEY_ID as usize => { log_status("exit: Ctrl+Alt+Q hotkey"); unsafe { DestroyWindow(hwnd) }; 0 }
        WM_PAINT => { unsafe { paint(hwnd) }; 0 }
        WM_ERASEBKGND => 1,
        WM_NCHITTEST => -1, // HTTRANSPARENT: don't steal mouse input from the IME.
        WM_CLOSE => { unsafe { DestroyWindow(hwnd) }; 0 }
        WM_DESTROY => {
            log_status("window: WM_DESTROY received");
            unsafe { UnregisterHotKey(hwnd, EXIT_HOTKEY_ID); PostQuitMessage(0) };
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, w, l) },
    }
}

fn main() {
    // `--quit` closes the already-running background copy without Task Manager.
    if std::env::args().any(|arg| arg == "--quit" || arg == "--exit") {
        let class_name = wide("TransPopBubbleRust");
        unsafe {
            let hwnd = FindWindowW(class_name.as_ptr(), ptr::null());
            if !hwnd.is_null() { PostMessageW(hwnd, WM_CLOSE, 0, 0); }
        }
        return;
    }
    let debug = std::env::args().any(|arg| arg == "--debug");
    STARTED.set(Instant::now()).ok();
    STATE.set(Mutex::new(State {
        debug,
        text: "等待微软拼音候选…".into(),
        status: "扫描启动中 · Ctrl+Alt+Q 退出".into(),
        ..State::default()
    })).ok();
    let (sender, receiver) = mpsc::channel();
    RECEIVER.set(Mutex::new(receiver)).ok();
    std::panic::set_hook(Box::new(|info| {
        log_status(&format!("PANIC: {info}"));
    }));
    log_status(&format!("---- TransPop v0.6.7 start | debug={} ----", debug));

    unsafe {
        // Match the physical coordinates returned by UIA even at non-100% display scaling.
        SetProcessDPIAware();
        let class_name = wide("TransPopBubbleRust");
        let title = wide("TransPop 翻译气泡");
        let instance = GetModuleHandleW(ptr::null());
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: class_name.as_ptr(),
            ..mem::zeroed()
        };
        if RegisterClassW(&class) == 0 {
            log_status("ERROR: RegisterClassW failed");
            return;
        }
        let window = CreateWindowExW(
            WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST
                | WS_EX_LAYERED | WS_EX_TRANSPARENT,
            class_name.as_ptr(), title.as_ptr(), WS_POPUP,
            0, 0, WIDTH, HEIGHT, ptr::null_mut(), ptr::null_mut(), instance, ptr::null(),
        );
        if window.is_null() {
            log_status("ERROR: CreateWindowExW failed");
            return;
        }
        // Do not launch duplicate background copies.
        // Exit via Ctrl+Alt+Q or `TransPop.exe --quit`.
        if RegisterHotKey(window, EXIT_HOTKEY_ID, MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, 'Q' as u32) == 0 {
            log_status("WARN: Ctrl+Alt+Q global exit hotkey unavailable; --quit still works");
        }
        SetLayeredWindowAttributes(window, 0, 248, LWA_ALPHA);
        let region = CreateRoundRectRgn(0, 0, WIDTH, HEIGHT, 25, 25);
        if !region.is_null() { SetWindowRgn(window, region, 1); }
        if SetTimer(window, TICK_ID, TICK_MS, None) == 0 {
            log_status("ERROR: SetTimer failed; no UI status updates are possible");
        }
        if debug {
            SetWindowPos(window, HWND_TOPMOST, 24, 24, WIDTH, HEIGHT,
                SWP_NOACTIVATE | SWP_SHOWWINDOW);
            ShowWindow(window, SW_SHOWNOACTIVATE);
            UpdateWindow(window); // Render the initial message *before* the first UIA query.
            log_status("debug bubble displayed at (24,24)");
        }
        spawn_scanner(sender);
        let mut msg: MSG = mem::zeroed();
        loop {
            let result = GetMessageW(&mut msg, ptr::null_mut(), 0, 0);
            if result <= 0 {
                log_status(&format!("message loop stopped; GetMessageW={result}"));
                break;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
