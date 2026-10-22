//! Foreground self-update prompt.
//!
//! Checking for a release happens on a worker thread, but the decision is the
//! user's: the release is announced first, downloaded only after "立即更新" is
//! clicked, and the transfer progress is drawn while it runs. The window runs
//! its own message pump on the update worker thread, so the main message loop
//! is never blocked by a modal update.

use std::{
    ffi::c_void,
    mem,
    ptr,
};

use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::Gdi::{
        BeginPaint, CreateFontW, DrawTextW, EndPaint, FillRect, FrameRect, GetDC, GetDeviceCaps,
        GetMonitorInfoW, GetSysColor, GetSysColorBrush, InvalidateRect, MonitorFromPoint, ReleaseDC,
        SelectObject, SetBkMode, SetTextColor, UpdateWindow, COLOR_BTNFACE, COLOR_BTNSHADOW, COLOR_HIGHLIGHT, COLOR_WINDOW,
        COLOR_WINDOWTEXT, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, HDC, HFONT, MONITORINFO,
        MONITOR_DEFAULTTONEAREST, PAINTSTRUCT, TRANSPARENT,
    },
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Input::KeyboardAndMouse::SetFocus,
        WindowsAndMessaging::{
            AdjustWindowRectEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
            GetClientRect, GetCursorPos, GetMessageW, GetSystemMetrics, GetWindowLongPtrW, IsWindow,
            LoadCursorW, PeekMessageW, RegisterClassW, SendMessageW, SetForegroundWindow,
            SetWindowLongPtrW, ShowWindow, TranslateMessage,
            BS_DEFPUSHBUTTON, GWLP_USERDATA, IDC_ARROW, MSG, PM_REMOVE, SM_CXSCREEN, SM_CYSCREEN,
            SW_HIDE, SW_SHOW, WM_COMMAND, WM_CLOSE, WM_DESTROY, WM_ERASEBKGND, WM_PAINT,
            WM_SETFONT, WNDCLASSW, WS_CAPTION, WS_CHILD, WS_CLIPCHILDREN, WS_OVERLAPPED, WS_SYSMENU,
            WS_TABSTOP, WS_VISIBLE,
        },
    },
};

const CLASS: &str = "TransPop-Update-Prompt-071";
const WINDOW_TITLE: &str = "TransPop 更新";
const ID_UPDATE: i32 = 5100;
const ID_LATER: i32 = 5101;

// Logical (96 dpi) layout; everything is scaled by the monitor's system dpi.
const CLIENT_W: i32 = 420;
const CLIENT_H: i32 = 190;
const PADDING: i32 = 24;
const TITLE_TOP: i32 = 18;
const TITLE_H: i32 = 30;
const BODY_TOP: i32 = TITLE_TOP + TITLE_H + 8;
const BODY_H: i32 = 24;
const BAR_TOP: i32 = BODY_TOP + BODY_H + 16;
const BAR_H: i32 = 10;
const BUTTON_W: i32 = 104;
const BUTTON_H: i32 = 30;

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Prompt,
    Downloading,
    Installing,
}

struct DialogState {
    dpi: i32,
    phase: Phase,
    percent: u32,
    tag: String,
    decision: Option<bool>,
    title_font: HFONT,
    body_font: HFONT,
    update_button: HWND,
    later_button: HWND,
}

/// The prompt plus the verify-and-install window share one dialog: it announces
/// the release, waits for the yes/no answer and then shows the download itself.
pub struct UpdatePrompt {
    hwnd: HWND,
    state: *mut DialogState,
}

impl UpdatePrompt {
    pub unsafe fn open(tag: &str) -> Option<Self> {
        let dpi = system_dpi();
        let s = |value: i32| value * dpi / 96;
        let instance = GetModuleHandleW(ptr::null());
        let class_name = super::wide(CLASS);
        let mut class: WNDCLASSW = mem::zeroed();
        class.lpfnWndProc = Some(proc);
        class.hInstance = instance;
        class.hIcon = super::load_brand_icon(false);
        class.hCursor = LoadCursorW(ptr::null_mut(), IDC_ARROW);
        class.lpszClassName = class_name.as_ptr();
        RegisterClassW(&class);

        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN;
        let mut frame = RECT {
            left: 0,
            top: 0,
            right: s(CLIENT_W),
            bottom: s(CLIENT_H),
        };
        AdjustWindowRectEx(&mut frame, style, 0, 0);
        let width = frame.right - frame.left;
        let height = frame.bottom - frame.top;
        let (x, y) = centered(width, height);
        let title = super::wide(WINDOW_TITLE);
        let hwnd = CreateWindowExW(
            0, class_name.as_ptr(), title.as_ptr(), style, x, y, width, height,
            ptr::null_mut(), ptr::null_mut(), instance, ptr::null_mut(),
        );
        if hwnd.is_null() { return None; }

        let state = Box::into_raw(Box::new(DialogState {
            dpi,
            phase: Phase::Prompt,
            percent: 0,
            tag: tag.to_owned(),
            decision: None,
            title_font: create_font(-s(17), 600),
            body_font: create_font(-s(13), 400),
            update_button: ptr::null_mut(),
            later_button: ptr::null_mut(),
        }));
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize);

        let class_button = super::wide("BUTTON");
        let label_update = super::wide("立即更新");
        let label_later = super::wide("稍后");
        let button_style = WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_DEFPUSHBUTTON as u32;
        let button_y = s(CLIENT_H - PADDING - BUTTON_H);
        let right = s(CLIENT_W - PADDING);
        let update_button = CreateWindowExW(
            0, class_button.as_ptr(), label_update.as_ptr(), button_style,
            right - s(BUTTON_W), button_y, s(BUTTON_W), s(BUTTON_H),
            hwnd, ID_UPDATE as usize as *mut c_void, instance, ptr::null_mut(),
        );
        let later_button = CreateWindowExW(
            0, class_button.as_ptr(), label_later.as_ptr(), button_style & !(BS_DEFPUSHBUTTON as u32),
            right - 2 * s(BUTTON_W) - s(8), button_y, s(BUTTON_W), s(BUTTON_H),
            hwnd, ID_LATER as usize as *mut c_void, instance, ptr::null_mut(),
        );
        (*state).update_button = update_button;
        (*state).later_button = later_button;
        for button in [update_button, later_button] {
            if !button.is_null() {
                SendMessageW(button, WM_SETFONT, (*state).body_font as usize, 1);
            }
        }

        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);
        SetForegroundWindow(hwnd);
        if !update_button.is_null() { SetFocus(update_button); }
        Some(UpdatePrompt { hwnd, state })
    }

    /// Block until the user answers. True means the update may download.
    pub unsafe fn wait_for_decision(&mut self) -> bool {
        let mut msg: MSG = mem::zeroed();
        while (*self.state).decision.is_none() {
            if GetMessageW(&mut msg, ptr::null_mut(), 0, 0) <= 0 { return false; }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        (*self.state).decision == Some(true)
    }

    /// The user agreed: hide the buttons and show the progress bar.
    pub unsafe fn start_download(&mut self) {
        (*self.state).phase = Phase::Downloading;
        for button in [(*self.state).update_button, (*self.state).later_button] {
            if !button.is_null() { ShowWindow(button, SW_HIDE); }
        }
        self.refresh();
    }

    pub unsafe fn set_percent(&mut self, percent: u32) {
        let percent = percent.min(100);
        if (*self.state).percent != percent {
            (*self.state).percent = percent;
            self.refresh();
        }
    }

    /// The verified executable is about to replace this one.
    pub unsafe fn install(&mut self) {
        (*self.state).phase = Phase::Installing;
        (*self.state).percent = 100;
        self.refresh();
    }

    /// Dispatch pending dialog messages without blocking; false means WM_QUIT.
    pub unsafe fn pump(&mut self) -> bool {
        let mut msg: MSG = mem::zeroed();
        while PeekMessageW(&mut msg, ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            if msg.message == 0x0012 { return false; }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        true
    }

    pub unsafe fn close(mut self) {
        if IsWindow(self.hwnd) != 0 { DestroyWindow(self.hwnd); }
        self.hwnd = ptr::null_mut();
        self.state = ptr::null_mut();
    }

    unsafe fn refresh(&self) {
        InvalidateRect(self.hwnd, ptr::null(), 0);
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut DialogState;
    match msg {
        WM_ERASEBKGND => {
            let mut client: RECT = mem::zeroed();
            GetClientRect(hwnd, &mut client);
            FillRect(lp as HDC, &client, GetSysColorBrush(COLOR_WINDOW));
            1
        }
        WM_PAINT => {
            paint(hwnd);
            0
        }
        WM_COMMAND => {
            if !state.is_null() && (wp & 0xFFFF) as i32 == ID_UPDATE {
                (*state).decision = Some(true);
            } else if !state.is_null() && (wp & 0xFFFF) as i32 == ID_LATER {
                (*state).decision = Some(false);
            }
            0
        }
        WM_CLOSE => {
            // Closing the frame is the same answer as "稍后"; the waiting
            // loop owns the lifetime so the state pointer stays valid.
            if !state.is_null() { (*state).decision = Some(false); }
            0
        }
        WM_DESTROY => {
            if !state.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                let state = Box::from_raw(state);
                if !state.title_font.is_null() { windows_sys::Win32::Graphics::Gdi::DeleteObject(state.title_font); }
                if !state.body_font.is_null() { windows_sys::Win32::Graphics::Gdi::DeleteObject(state.body_font); }
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

unsafe fn paint(hwnd: HWND) {
    let mut paint_struct: PAINTSTRUCT = mem::zeroed();
    let hdc = BeginPaint(hwnd, &mut paint_struct);
    let mut client: RECT = mem::zeroed();
    GetClientRect(hwnd, &mut client);
    FillRect(hdc, &client, GetSysColorBrush(COLOR_WINDOW));
    let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut DialogState;
    if !state.is_null() {
        let dpi = (*state).dpi;
        let s = |value: i32| value * dpi / 96;
        let left = s(PADDING);
        let width = client.right - 2 * left;

        let (title, body) = match (*state).phase {
            Phase::Prompt => (
                format!("发现新版本 {}", (*state).tag),
                format!("当前版本 v{}，是否立即下载并更新？", super::VERSION),
            ),
            Phase::Downloading if (*state).percent > 0 => (
                "正在下载更新".to_string(),
                format!("已下载 {}%", (*state).percent),
            ),
            Phase::Downloading => (
                "正在下载更新".to_string(),
                "正在连接 GitHub…".to_string(),
            ),
            Phase::Installing => (
                "正在安装更新".to_string(),
                "TransPop 即将重新启动。".to_string(),
            ),
        };
        let mut title_rect = RECT {
            left,
            top: s(TITLE_TOP),
            right: left + width,
            bottom: s(TITLE_TOP + TITLE_H),
        };
        draw_text(hdc, (*state).title_font, &title, &mut title_rect);
        let mut body_rect = RECT {
            left,
            top: s(BODY_TOP),
            right: left + width,
            bottom: s(BODY_TOP + BODY_H),
        };
        draw_text(hdc, (*state).body_font, &body, &mut body_rect);

        if (*state).phase != Phase::Prompt {
            let bar = RECT {
                left,
                top: s(BAR_TOP),
                right: left + width,
                bottom: s(BAR_TOP + BAR_H),
            };
            FillRect(hdc, &bar, GetSysColorBrush(COLOR_BTNFACE));
            let filled = width * (*state).percent.min(100) as i32 / 100;
            if filled > 0 {
                let portion = RECT {
                    left,
                    top: bar.top + 1,
                    right: left + filled,
                    bottom: bar.bottom - 1,
                };
                FillRect(hdc, &portion, GetSysColorBrush(COLOR_HIGHLIGHT));
            }
            FrameRect(hdc, &bar, GetSysColorBrush(COLOR_BTNSHADOW));
        }
    }
    EndPaint(hwnd, &paint_struct);
}

unsafe fn draw_text(hdc: HDC, font: HFONT, text: &str, rect: &mut RECT) {
    if font.is_null() { return; }
    let previous = SelectObject(hdc, font);
    SetBkMode(hdc, TRANSPARENT as i32);
    SetTextColor(hdc, GetSysColor(COLOR_WINDOWTEXT));
    let buffer = super::wide(text);
    DrawTextW(hdc, buffer.as_ptr(), -1, rect, DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX);
    SelectObject(hdc, previous);
}

unsafe fn create_font(height: i32, weight: i32) -> HFONT {
    let face = super::wide("Segoe UI");
    CreateFontW(height, 0, 0, 0, weight, 0, 0, 0, 1, 0, 0, 5, 0, face.as_ptr())
}

fn system_dpi() -> i32 {
    unsafe {
        let dc = GetDC(ptr::null_mut());
        let dpi = if dc.is_null() { 0 } else { GetDeviceCaps(dc, 90) };
        ReleaseDC(ptr::null_mut(), dc);
        if (72..=384).contains(&dpi) { dpi } else { 96 }
    }
}

unsafe fn centered(width: i32, height: i32) -> (i32, i32) {
    let mut cursor = POINT { x: 0, y: 0 };
    GetCursorPos(&mut cursor);
    let monitor = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
    let mut info: MONITORINFO = mem::zeroed();
    info.cbSize = mem::size_of::<MONITORINFO>() as u32;
    let work = if GetMonitorInfoW(monitor, &mut info) != 0 {
        info.rcWork
    } else {
        RECT {
            left: 0,
            top: 0,
            right: GetSystemMetrics(SM_CXSCREEN),
            bottom: GetSystemMetrics(SM_CYSCREEN),
        }
    };
    let x = work.left + ((work.right - work.left - width) / 2).max(0);
    let y = work.top + ((work.bottom - work.top - height) / 2).max(0);
    (x, y)
}
