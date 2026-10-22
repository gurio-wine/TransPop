//! Tiny native Win32 settings window. Does not paint the translation bubble.
//! The window procedure only forwards form commands to the application's
//! existing message loop. No UIA or networking occurs in a Win32 callback.

use std::{collections::HashMap, fs, io::Write, mem, path::{Path, PathBuf}, ptr};
use windows_sys::Win32::{
    Foundation::{HWND, POINT, RECT},
    Graphics::{
        Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE},
        Gdi::{
            CreateFontW, CreatePen, CreateSolidBrush, DeleteObject, DrawFocusRect, DrawTextW,
            FillRect, FrameRect, GetMonitorInfoW, GetStockObject, GetSysColor, InvalidateRect,
            MonitorFromPoint, MonitorFromWindow, Polyline, RedrawWindow, SelectObject, SetBkColor,
            SetBkMode, SetTextColor, COLOR_GRAYTEXT, COLOR_HIGHLIGHT, COLOR_WINDOW, COLOR_WINDOWTEXT,
            DT_CENTER, DT_LEFT, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, HBRUSH, HFONT, MONITORINFO,
            MONITOR_DEFAULTTONEAREST, RDW_ALLCHILDREN, RDW_ERASE, RDW_INVALIDATE,
        },
    },
    System::{
        LibraryLoader::GetModuleHandleW,
        Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD},
    },
    UI::{
        Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW},
        Controls::{SetWindowTheme, DRAWITEMSTRUCT, ODS_FOCUS},
        Input::KeyboardAndMouse::{SetFocus, VK_ESCAPE},
        WindowsAndMessaging::{
            AdjustWindowRectEx, CreateWindowExW, DefWindowProcW, DestroyWindow, EnumChildWindows,
            GetClassNameW, GetClientRect, GetCursorPos, GetDlgItem, GetSystemMetrics, GetParent,
            GetWindow, GetWindowLongPtrW, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
            IsWindow, IsWindowVisible, PostMessageW, RegisterClassW, SendMessageW, SetForegroundWindow,
            SetWindowLongPtrW, SetWindowPos, SetWindowTextW, ShowWindow, SystemParametersInfoW,
            BN_CLICKED, DM_GETDEFID, GWLP_USERDATA, SPI_GETHIGHCONTRAST, SW_HIDE, SW_SHOW,
            SW_SHOWNA,
            SWP_NOACTIVATE, SWP_NOZORDER, WM_CLOSE, WM_COMMAND, WM_DRAWITEM, WM_GETFONT, WNDCLASSW,
            WS_BORDER, WS_CAPTION, WS_CHILD, WS_CLIPCHILDREN, WS_EX_APPWINDOW, WS_OVERLAPPED,
            WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
        },
    },
};

const CLASS: &str = "TransPopNativeSettings061";
const WM_SETFONT: u32 = 0x0030;
const ES_AUTOHSCROLL: u32 = 0x0080;
const BS_OWNERDRAW: u32 = 0x000B;
const ES_PASSWORD: u32 = 0x0020;
const CBS_DROPDOWN: u32 = 0x0002;
const EM_SETREADONLY: u32 = 0x00CF;
const GW_CHILD: u32 = 5;
const CB_ADDSTRING: u32 = 0x0143;
const CB_GETCURSEL: u32 = 0x0147;
const CB_SETCURSEL: u32 = 0x014E;
const CBN_SELCHANGE: u16 = 1;
const ID_PROVIDER: u16 = 4100;
const ID_CANCEL: u16 = 4102;
const ID_AZURE_REVEAL: u16 = 4103;
const ID_OPENAI_REVEAL: u16 = 4104;
const ID_AZURE_SECRET: u16 = 4105;
const ID_OPENAI_SECRET: u16 = 4106;
const ID_UPDATE: u16 = 4107;
const ID_AUTOSTART: u16 = 4108;
const ID_SHORTCUT_KEY: u16 = 4109;
const ID_MOD_SHIFT: u16 = 4114;
const ID_MOD_CTRL: u16 = 4115;
const ID_MOD_ALT: u16 = 4116;
const ID_MOD_WIN: u16 = 4117;
const ID_AUTO_UPDATE: u16 = 4110;
const ID_THEME: u16 = 4111;
const ID_UPDATE_INTERVAL: u16 = 4112;
const ID_AUTO_INSTALL: u16 = 4113;
// One owner-drawn button per tab, in display order.
const ID_TAB_TRANSLATE: u16 = 4120;
const ID_TAB_APPEARANCE: u16 = 4121;
const ID_TAB_SYSTEM: u16 = 4122;
const ID_TAB_SHORTCUT: u16 = 4124;
const ID_TAB_ABOUT: u16 = 4123;
/// Edit notification sent when the field loses focus; the moment a typed
/// value is considered final, which is what makes save-on-change safe.
const EN_KILLFOCUS: u16 = 0x0200;
const EM_SETPASSWORDCHAR: u32 = 0x00CC;
const WM_CTLCOLORSTATIC: u32 = 0x0138;
const WM_CTLCOLOREDIT: u32 = 0x0133;
const WM_CTLCOLORLISTBOX: u32 = 0x0134;
const WM_ERASEBKGND: u32 = 0x0014;
const WM_KEYDOWN: u32 = 0x0100;
const IDCANCEL: u16 = 2;
const EM_SETMARGINS: u32 = 0x00D3;
const EC_LEFT_RIGHT_MARGINS: usize = 0x0003;
const TRANSPARENT: i32 = 1;
const SS_ICON: u32 = 0x0003;
const STM_SETICON: u32 = 0x0170;
const PS_SOLID: i32 = 0;

// One shared grid: 26 px side margin, 520 px of content, labels end at 136.
const CLIENT_W: i32 = 572;
const CLIENT_H: i32 = 500;
const MARGIN_X: i32 = 26;
const CONTENT_W: i32 = 520;
const LABEL_W: i32 = 118;
const FIELD_X: i32 = 154;
const FIELD_W: i32 = 392;
const SECRET_W: i32 = 323;
// Tab strip and the single content origin every page shares.
const TAB_Y: i32 = 96;
const TAB_W: i32 = 100;
const TAB_H: i32 = 30;
const TAB_GAP: i32 = 6;
const CONTENT_TOP: i32 = 150;
const WINDOW_STYLE: u32 = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN;
const WINDOW_EX_STYLE: u32 = WS_EX_APPWINDOW;
const WM_APP_SETTINGS_SAVE: u32 = super::WM_APP + 30;
const WM_APP_SETTINGS_CANCEL: u32 = super::WM_APP + 31;
const WM_APP_SETTINGS_PROVIDER: u32 = super::WM_APP + 32;
const WM_APP_SETTINGS_UPDATE: u32 = super::WM_APP + 33;
const WM_APP_SETTINGS_PAGE: u32 = super::WM_APP + 34;

pub const SAVE_MESSAGE: u32 = WM_APP_SETTINGS_SAVE;
pub const CANCEL_MESSAGE: u32 = WM_APP_SETTINGS_CANCEL;
pub const PROVIDER_MESSAGE: u32 = WM_APP_SETTINGS_PROVIDER;
pub const UPDATE_MESSAGE: u32 = WM_APP_SETTINGS_UPDATE;
pub const PAGE_MESSAGE: u32 = WM_APP_SETTINGS_PAGE;

fn wide(s: &str) -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() }

/// Colour roles for one palette. Windows has three: the user's light or dark
/// app theme, and high contrast, which overrides both.
struct Colors {
    bg: u32,
    field: u32,
    text: u32,
    secondary: u32,
    line: u32,
    accent: u32,
}

#[derive(Clone, Copy, PartialEq)]
enum Palette {
    Light,
    Dark,
    HighContrast,
}

impl Palette {
    /// High contrast wins, then an explicit env override (screenshot hook), then
    /// the saved in-app choice, then the Windows app theme.
    unsafe fn detect(base: &Path) -> Self {
        if high_contrast_enabled() { return Self::HighContrast; }
        match std::env::var("TRANSPOP_THEME").unwrap_or_default().to_ascii_lowercase().as_str() {
            "dark" => return Self::Dark,
            "light" => return Self::Light,
            _ => {}
        }
        match ConfigFile::read(base).theme.as_str() {
            "light" => Self::Light,
            "dark" => Self::Dark,
            _ => Self::from_windows(),
        }
    }

    /// The theme the window follows while the saved choice is "system".
    unsafe fn detect_system() -> Self {
        if high_contrast_enabled() { return Self::HighContrast; }
        match std::env::var("TRANSPOP_THEME").unwrap_or_default().to_ascii_lowercase().as_str() {
            "dark" => Self::Dark,
            "light" => Self::Light,
            _ => Self::from_windows(),
        }
    }

    unsafe fn from_windows() -> Self {
        if apps_use_light_theme() { Self::Light } else { Self::Dark }
    }

    unsafe fn colors(self) -> Colors {
        match self {
            Self::Light => Colors {
                bg: 0x00FFFFFF, field: 0x00FFFFFF, text: 0x00202020,
                secondary: 0x00606060, line: 0x00E2E2E2, accent: 0x00D47800,
            },
            Self::Dark => Colors {
                bg: 0x00202020, field: 0x002D2D2D, text: 0x00F0F0F0,
                secondary: 0x00A0A0A0, line: 0x003C3C3C, accent: 0x00FFC24C,
            },
            Self::HighContrast => Colors {
                bg: GetSysColor(COLOR_WINDOW),
                field: GetSysColor(COLOR_WINDOW),
                text: GetSysColor(COLOR_WINDOWTEXT),
                secondary: GetSysColor(COLOR_GRAYTEXT),
                line: GetSysColor(COLOR_GRAYTEXT),
                accent: GetSysColor(COLOR_HIGHLIGHT),
            },
        }
    }
}

unsafe fn high_contrast_enabled() -> bool {
    let mut hc: HIGHCONTRASTW = mem::zeroed();
    hc.cbSize = mem::size_of::<HIGHCONTRASTW>() as u32;
    SystemParametersInfoW(SPI_GETHIGHCONTRAST, hc.cbSize,
        (&mut hc as *mut HIGHCONTRASTW).cast(), 0) != 0
        && hc.dwFlags & HCF_HIGHCONTRASTON != 0
}

/// AppsUseLightTheme drives app surfaces; SystemUsesLightTheme only drives the
/// shell. An unreadable value means light, which is also the safe fallback.
unsafe fn apps_use_light_theme() -> bool {
    let sub = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
    let name = wide("AppsUseLightTheme");
    let mut value: u32 = 1;
    let mut size = mem::size_of::<u32>() as u32;
    let status = RegGetValueW(HKEY_CURRENT_USER, sub.as_ptr(), name.as_ptr(), RRF_RT_REG_DWORD,
        ptr::null_mut(), (&mut value as *mut u32).cast(), &mut size);
    status != 0 || value != 0
}

/// Everything the window procedure needs to paint. Boxed by the window and
/// reached through GWLP_USERDATA, because a Win32 child outlives any borrow.
struct UiTheme {
    dark: bool,
    /// Frame colour currently applied; Windows only repaints the non-client
    /// area across a hide/show cycle, so a real change is tracked.
    frame_dark: bool,
    dpi: i32,
    colors: Colors,
    bg_brush: HBRUSH,
    field_brush: HBRUSH,
    line_brush: HBRUSH,
    accent_brush: HBRUSH,
    /// Secondary-text statics, painted with the muted colour.
    notes: Vec<isize>,
    /// Hairline statics, painted with the line colour.
    lines: Vec<isize>,
    /// Drop-downs whose closed field is painted by this theme.
    combos: Vec<isize>,
    /// Owner-drawn tab buttons, and which page is showing.
    tabs: Vec<isize>,
    page: usize,
}

impl UiTheme {
    unsafe fn create(palette: Palette, dpi: i32) -> Self {
        let mut theme = Self {
            dark: palette == Palette::Dark,
            frame_dark: palette == Palette::Dark,
            dpi,
            colors: palette.colors(),
            bg_brush: ptr::null_mut(),
            field_brush: ptr::null_mut(),
            line_brush: ptr::null_mut(),
            accent_brush: ptr::null_mut(),
            notes: Vec::new(),
            lines: Vec::new(),
            combos: Vec::new(),
            tabs: Vec::new(),
            page: 0,
        };
        theme.rebuild_brushes();
        theme
    }

    unsafe fn rebuild_brushes(&mut self) {
        self.release();
        self.bg_brush = CreateSolidBrush(self.colors.bg);
        self.field_brush = CreateSolidBrush(self.colors.field);
        self.line_brush = CreateSolidBrush(self.colors.line);
        self.accent_brush = CreateSolidBrush(self.colors.accent);
    }

    /// Switches palette in place: brush handles change, control lists stay.
    unsafe fn recolour(&mut self, palette: Palette) {
        self.colors = palette.colors();
        self.dark = palette == Palette::Dark;
        self.rebuild_brushes();
    }

    fn is_combo(&self, control: HWND) -> bool {
        self.combos.contains(&(control as isize))
    }

    fn tab_index(&self, control: HWND) -> Option<usize> {
        self.tabs.iter().position(|tab| *tab == control as isize)
    }

    unsafe fn colors_for(&self, control: HWND) -> (u32, HBRUSH) {
        let key = control as isize;
        if self.lines.contains(&key) {
            (self.colors.line, self.line_brush)
        } else if self.notes.contains(&key) {
            (self.colors.secondary, self.bg_brush)
        } else {
            (self.colors.text, self.bg_brush)
        }
    }

    unsafe fn release(&self) {
        for brush in [self.bg_brush, self.field_brush, self.line_brush, self.accent_brush] {
            if !brush.is_null() { DeleteObject(brush); }
        }
    }
}

/// Owner-drawn checkbox. The stock button ignores WM_CTLCOLORBTN, so a dark
/// window would keep black labels (Windows 11 has no dark button theme either).
/// Owner-drawn buttons keep no check state of their own, so the 0/1 flag lives
/// in the control user data.
unsafe fn button_checked(control: HWND) -> bool {
    GetWindowLongPtrW(control, GWLP_USERDATA) == 1
}

unsafe fn set_button_checked(control: HWND, checked: bool) {
    SetWindowLongPtrW(control, GWLP_USERDATA, if checked { 1 } else { 0 });
    InvalidateRect(control, ptr::null(), 0);
}

unsafe fn draw_checkbox(item: &DRAWITEMSTRUCT, theme: &UiTheme) {
    let s = |value: i32| value * theme.dpi / 96;
    let hdc = item.hDC;
    FillRect(hdc, &item.rcItem, theme.bg_brush);
    let size = s(16).max(12);
    let left = item.rcItem.left;
    let top = (item.rcItem.top + item.rcItem.bottom - size) / 2;
    let glyph = RECT { left, top, right: left + size, bottom: top + size };
    let checked = button_checked(item.hwndItem);
    if checked {
        FillRect(hdc, &glyph, theme.accent_brush);
        let pen = CreatePen(PS_SOLID, s(2).max(1), theme.colors.bg);
        if !pen.is_null() {
            let old = SelectObject(hdc, pen);
            let points = [
                POINT { x: left + size * 20 / 100, y: top + size * 52 / 100 },
                POINT { x: left + size * 42 / 100, y: top + size * 74 / 100 },
                POINT { x: left + size * 78 / 100, y: top + size * 28 / 100 },
            ];
            Polyline(hdc, points.as_ptr(), points.len() as i32);
            SelectObject(hdc, old);
            DeleteObject(pen);
        }
    } else {
        let border = CreateSolidBrush(theme.colors.secondary);
        if !border.is_null() {
            FrameRect(hdc, &glyph, border);
            DeleteObject(border);
        }
    }
    let mut text = [0u16; 256];
    let len = GetWindowTextW(item.hwndItem, text.as_mut_ptr(), text.len() as i32).max(0) as usize;
    let font = SendMessageW(item.hwndItem, WM_GETFONT, 0, 0) as HFONT;
    let old_font = if font.is_null() { ptr::null_mut() } else { SelectObject(hdc, font) };
    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, theme.colors.text);
    let mut area = RECT {
        left: left + size + s(10),
        top: item.rcItem.top,
        right: item.rcItem.right,
        bottom: item.rcItem.bottom,
    };
    DrawTextW(hdc, text.as_ptr(), len as i32, &mut area,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
    if !old_font.is_null() { SelectObject(hdc, old_font); }
    if item.itemState & ODS_FOCUS != 0 { DrawFocusRect(hdc, &item.rcItem); }
}

/// Owner-drawn tab: a filled card for the page that is showing and plain text
/// for the rest. The stock tab control paints a light strip on Windows 10 that
/// no theme call can darken, so the strip is drawn here like the checkboxes.
unsafe fn draw_tab(item: &DRAWITEMSTRUCT, theme: &UiTheme, index: usize) {
    let s = |value: i32| value * theme.dpi / 96;
    let hdc = item.hDC;
    let active = theme.page == index;
    FillRect(hdc, &item.rcItem, theme.bg_brush);
    let mut area = item.rcItem;
    if active {
        let mut card = item.rcItem;
        card.bottom -= s(2);
        FillRect(hdc, &card, theme.field_brush);
        // Underline the active tab against the page background.
        let underline = RECT {
            left: item.rcItem.left,
            top: item.rcItem.bottom - s(2),
            right: item.rcItem.right,
            bottom: item.rcItem.bottom,
        };
        FillRect(hdc, &underline, theme.accent_brush);
    }
    let mut text = [0u16; 64];
    let len = GetWindowTextW(item.hwndItem, text.as_mut_ptr(), text.len() as i32).max(0) as usize;
    let font = SendMessageW(item.hwndItem, WM_GETFONT, 0, 0) as HFONT;
    let old_font = if font.is_null() { ptr::null_mut() } else { SelectObject(hdc, font) };
    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, if active { theme.colors.text } else { theme.colors.secondary });
    DrawTextW(hdc, text.as_ptr(), len as i32, &mut area,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
    if !old_font.is_null() { SelectObject(hdc, old_font); }
    if item.itemState & ODS_FOCUS != 0 {
        let mut focus = item.rcItem;
        focus.left += s(2);
        focus.right -= s(2);
        DrawFocusRect(hdc, &focus);
    }
}


/// Applies the palette to the frame and every child control. Runs on open and
/// whenever the in-app theme picker changes, so no restart is needed.
unsafe fn apply_window_theme(hwnd: HWND, theme: &mut UiTheme) {
    let changed = theme.frame_dark != theme.dark;
    theme.frame_dark = theme.dark;
    let dark: u32 = if theme.dark { 1 } else { 0 };
    // 20 is the current attribute; 19 is the pre-20H1 number, still worth a try.
    if DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
        (&dark as *const u32).cast(), 4) != 0 {
        let _ = DwmSetWindowAttribute(hwnd, 19, (&dark as *const u32).cast(), 4);
    }
    // The non-client area ignores runtime attribute flips until the window is
    // re-shown, so a live switch costs one flicker.
    if changed && IsWindowVisible(hwnd) != 0 {
        ShowWindow(hwnd, SW_HIDE);
        ShowWindow(hwnd, SW_SHOWNA);
    }
    unsafe extern "system" fn retheme(child: HWND, lp: isize) -> i32 {
        let dark = *(lp as *const bool);
        let mut class = [0u16; 32];
        let len = GetClassNameW(child, class.as_mut_ptr(), class.len() as i32).max(0) as usize;
        let name = String::from_utf16_lossy(&class[..len]).to_ascii_lowercase();
        // Checkboxes are owner-drawn and need no scheme.
        let scheme = if !dark { "" }
            else if name.contains("combobox") { "DarkMode_CFD" }
            else if name.contains("edit") || name.contains("button") { "DarkMode_Explorer" }
            else { return 1 };
        let scheme = wide(scheme);
        SetWindowTheme(child, scheme.as_ptr(), ptr::null());
        1
    }
    EnumChildWindows(hwnd, Some(retheme), (&theme.dark as *const bool) as isize);
    RedrawWindow(hwnd, ptr::null(), ptr::null_mut(),
        RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN);
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: usize, lp: isize) -> isize {
    let theme = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const UiTheme;
    match msg {
        WM_ERASEBKGND => {
            // The class brush is only a fallback: repaint with the theme colour.
            if !theme.is_null() {
                let mut rect: RECT = mem::zeroed();
                GetClientRect(hwnd, &mut rect);
                FillRect(wp as _, &rect, (*theme).bg_brush);
            }
            1
        }
        WM_CTLCOLORSTATIC => {
            if theme.is_null() { return DefWindowProcW(hwnd, msg, wp, lp); }
            // Windows 10 has no dark combobox theme, so the closed field is
            // painted unthemed and asks the parent for its colours.
            if (*theme).dark
                && ((*theme).is_combo(lp as HWND) || (*theme).is_combo(GetParent(lp as HWND))) {
                SetBkColor(wp as _, (*theme).colors.field);
                SetTextColor(wp as _, (*theme).colors.text);
                return (*theme).field_brush as isize;
            }
            let (text, brush) = (*theme).colors_for(lp as HWND);
            SetBkMode(wp as _, TRANSPARENT);
            SetTextColor(wp as _, text);
            brush as isize
        }
        WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => {
            // Light mode keeps the stock control painting, which already matches.
            if theme.is_null() || !(*theme).dark {
                return DefWindowProcW(hwnd, msg, wp, lp);
            }
            SetBkColor(wp as _, (*theme).colors.field);
            SetTextColor(wp as _, (*theme).colors.text);
            (*theme).field_brush as isize
        }
        WM_DRAWITEM => {
            if theme.is_null() { return DefWindowProcW(hwnd, msg, wp, lp); }
            let item = &*(lp as *const DRAWITEMSTRUCT);
            match (*theme).tab_index(item.hwndItem) {
                Some(index) => draw_tab(item, &*theme, index),
                None => draw_checkbox(item, &*theme),
            }
            1
        }
        // No default button: Enter must never dismiss a form that saves itself.
        DM_GETDEFID => 0,
        WM_KEYDOWN if wp as u16 == VK_ESCAPE => {
            let main = super::CONTROL_HWND.load(std::sync::atomic::Ordering::Acquire) as HWND;
            if !main.is_null() { PostMessageW(main, WM_APP_SETTINGS_CANCEL, 0, 0); }
            0
        }
        WM_CLOSE => {
            let main = super::CONTROL_HWND.load(std::sync::atomic::Ordering::Acquire) as HWND;
            if !main.is_null() { PostMessageW(main, WM_APP_SETTINGS_CANCEL, 0, 0); }
            0
        }
        WM_COMMAND => {
            let id = (wp & 0xffff) as u16;
            let notify = ((wp >> 16) & 0xffff) as u16;
            let control = lp as HWND;
            // Tabs only repaint the page; the window keeps one size.
            if notify as u32 == BN_CLICKED
                && matches!(id, ID_TAB_TRANSLATE | ID_TAB_APPEARANCE | ID_TAB_SYSTEM
                    | ID_TAB_SHORTCUT | ID_TAB_ABOUT) {
                let page = match id {
                    ID_TAB_APPEARANCE => 1,
                    ID_TAB_SYSTEM => 2,
                    ID_TAB_SHORTCUT => 3,
                    ID_TAB_ABOUT => 4,
                    _ => 0,
                };
                let main = super::CONTROL_HWND.load(std::sync::atomic::Ordering::Acquire) as HWND;
                if !main.is_null() { PostMessageW(main, WM_APP_SETTINGS_PAGE, page, 0); }
                return 0;
            }
            // Owner-drawn checks do not flip themselves; mirror the click into
            // the button state, then repaint.
            if notify as u32 == BN_CLICKED
                && matches!(id, ID_AUTOSTART | ID_AUTO_UPDATE | ID_MOD_SHIFT | ID_MOD_CTRL
                    | ID_MOD_ALT | ID_MOD_WIN | ID_AUTO_INSTALL
                    | ID_AZURE_REVEAL | ID_OPENAI_REVEAL) {
                let was_checked = button_checked(control);
                set_button_checked(control, !was_checked);
                if id == ID_AZURE_REVEAL || id == ID_OPENAI_REVEAL {
                    // Password visibility is local to this dialog; it never persists
                    // secrets or changes the saved values by itself.
                    let secret_id = if id == ID_AZURE_REVEAL { ID_AZURE_SECRET } else { ID_OPENAI_SECRET };
                    let edit = GetDlgItem(hwnd, secret_id as i32);
                    if !edit.is_null() {
                        SendMessageW(edit, EM_SETPASSWORDCHAR,
                            if was_checked { '*' as usize } else { 0 }, 0);
                        InvalidateRect(edit, ptr::null(), 1);
                    }
                    return 0;
                }
                // A real switch: apply it now, exactly like the Save button did.
                let main = super::CONTROL_HWND.load(std::sync::atomic::Ordering::Acquire) as HWND;
                if !main.is_null() { PostMessageW(main, WM_APP_SETTINGS_SAVE, 0, 0); }
                return 0;
            }
            if id == ID_THEME && notify == CBN_SELCHANGE && !theme.is_null() {
                let palette = match SendMessageW(control, CB_GETCURSEL, 0, 0) {
                    1 => Palette::Light,
                    2 => Palette::Dark,
                    _ => Palette::detect_system(),
                };
                let theme = theme as *mut UiTheme;
                (*theme).recolour(palette);
                apply_window_theme(hwnd, &mut *theme);
                // The choice has to outlive the dialog, so it is saved too.
                let main = super::CONTROL_HWND.load(std::sync::atomic::Ordering::Acquire) as HWND;
                if !main.is_null() { PostMessageW(main, WM_APP_SETTINGS_SAVE, 0, 0); }
                return 0;
            }
            // Save-on-change: every committed edit reports the same message the
            // Save button used to send, so the main loop keeps one apply path.
            if id == ID_PROVIDER && notify == CBN_SELCHANGE {
                let main = super::CONTROL_HWND.load(std::sync::atomic::Ordering::Acquire) as HWND;
                if !main.is_null() {
                    PostMessageW(main, WM_APP_SETTINGS_PROVIDER, 0, 0);
                    PostMessageW(main, WM_APP_SETTINGS_SAVE, 0, 0);
                }
                return 0;
            }
            let committed = notify == CBN_SELCHANGE || notify == EN_KILLFOCUS;
            if committed {
                let main = super::CONTROL_HWND.load(std::sync::atomic::Ordering::Acquire) as HWND;
                if !main.is_null() { PostMessageW(main, WM_APP_SETTINGS_SAVE, 0, 0); }
                return 0;
            }
            let outgoing = if id == ID_CANCEL || id == IDCANCEL { WM_APP_SETTINGS_CANCEL }
                else if id == ID_UPDATE { WM_APP_SETTINGS_UPDATE }
                else { 0 };
            if outgoing != 0 {
                let main = super::CONTROL_HWND.load(std::sync::atomic::Ordering::Acquire) as HWND;
                if !main.is_null() { PostMessageW(main, outgoing, 0, 0); }
                return 0;
            }
            DefWindowProcW(hwnd, msg, wp, lp)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

/// Shared translator.conf parser. The settings window and the runtime loader
/// both build on this type, so key names, defaults and value normalization
/// cannot drift apart again.
#[derive(Clone, Debug)]
pub struct ConfigFile {
    pub provider: String,
    pub azure_url: String,
    pub azure_region: String,
    pub azure_key: String,
    pub openai_url: String,
    pub openai_model: String,
    pub openai_key: String,
    pub timeout_seconds: String,
    pub debounce_ms: String,
    pub alt_output_shortcut: String,
    pub auto_update: bool,
    pub auto_install: bool,
    pub update_interval_minutes: String,
    pub theme: String,
}

/// Update check intervals offered in the settings window, in minutes.
pub const UPDATE_INTERVALS: [u32; 7] = [15, 30, 60, 180, 360, 720, 1440];

pub const DEFAULT_UPDATE_INTERVAL: u32 = 30;

/// Shortcut that pastes the translation instead of the Chinese candidate.
pub const DEFAULT_SHORTCUT: &str = "lalt";

/// Modifier bits of a shortcut spec, in the order the label prints them.
pub const MOD_CTRL: u8 = 1;
pub const MOD_SHIFT: u8 = 2;
pub const MOD_ALT: u8 = 4;
pub const MOD_WIN: u8 = 8;

/// Keys the shortcut editor can name and the config file may hold. One table
/// serves both directions, so the spelling cannot drift.
pub const SHORTCUT_KEYS: [(&str, u32); 58] = [
    ("lalt", 0xA4), ("ralt", 0xA5), ("lctrl", 0xA2), ("rctrl", 0xA3),
    ("lshift", 0xA0), ("rshift", 0xA1), ("lwin", 0x5B), ("rwin", 0x5C),
    ("space", 0x20), ("enter", 0x0D), ("tab", 0x09), ("backspace", 0x08),
    ("esc", 0x1B), ("insert", 0x2D), ("delete", 0x2E), ("home", 0x24),
    ("end", 0x23), ("pageup", 0x21), ("pagedown", 0x22),
    ("left", 0x25), ("up", 0x26), ("right", 0x27), ("down", 0x28),
    ("capslock", 0x14), ("numlock", 0x90), ("scrolllock", 0x91),
    ("pause", 0x13), ("printscreen", 0x2C), ("apps", 0x5D),
    ("oem_1", 0xBA), ("oem_plus", 0xBB), ("oem_comma", 0xBC),
    ("oem_minus", 0xBD), ("oem_period", 0xBE), ("oem_2", 0xBF),
    ("oem_3", 0xC0), ("oem_4", 0xDB), ("oem_5", 0xDC), ("oem_6", 0xDD),
    ("oem_7", 0xDE),
    ("num0", 0x60), ("num1", 0x61), ("num2", 0x62), ("num3", 0x63),
    ("num4", 0x64), ("num5", 0x65), ("num6", 0x66), ("num7", 0x67),
    ("num8", 0x68), ("num9", 0x69), ("num_multiply", 0x6A), ("num_add", 0x6B),
    ("num_subtract", 0x6D), ("num_decimal", 0x6E), ("num_divide", 0x6F),
    ("mute", 0xAD), ("volumeup", 0xAF), ("volumedown", 0xAE),
];

const LETTERS: [&str; 26] = ["a","b","c","d","e","f","g","h","i","j","k","l",
    "m","n","o","p","q","r","s","t","u","v","w","x","y","z"];
const DIGITS: [&str; 10] = ["0","1","2","3","4","5","6","7","8","9"];
const FKEYS: [&str; 24] = ["f1","f2","f3","f4","f5","f6","f7","f8","f9","f10",
    "f11","f12","f13","f14","f15","f16","f17","f18","f19","f20",
    "f21","f22","f23","f24"];

/// Keys offered in the settings page. The config file accepts every name in
/// SHORTCUT_KEYS, so a hand-edited chord keeps working even when it is not
/// listed here.
pub const SHORTCUT_CHOICES: [(&str, u32); 31] = [
    ("\u{5de6} Alt", 0xA4), ("\u{53f3} Alt", 0xA5),
    ("\u{5de6} Ctrl", 0xA2), ("\u{53f3} Ctrl", 0xA3),
    ("\u{5de6} Shift", 0xA0), ("\u{53f3} Shift", 0xA1),
    ("Space", 0x20), ("Enter", 0x0D), ("Tab", 0x09), ("Backspace", 0x08),
    ("Esc", 0x1B), ("Insert", 0x2D), ("Delete", 0x2E), ("Home", 0x24),
    ("End", 0x23), ("Page Up", 0x21), ("Page Down", 0x22),
    ("Left", 0x25), ("Up", 0x26), ("Right", 0x27), ("Down", 0x28),
    ("F1", 0x70), ("F2", 0x71), ("F3", 0x72), ("F4", 0x73),
    ("F5", 0x74), ("F6", 0x75), ("F7", 0x76), ("F8", 0x77),
    ("F9", 0x78), ("F10", 0x79),
];

/// First entry is the off switch, so an empty choice is always expressible.
fn shortcut_key_items() -> Vec<String> {
    let mut items = vec!["\u{672a}\u{8bbe}\u{7f6e}".to_string()];
    items.extend(SHORTCUT_CHOICES.iter().map(|(name, _)| (*name).to_string()));
    items
}

/// Canonical spelling of a shortcut. Anything unreadable becomes `none`, which
/// is the safe state: no shortcut at all rather than a surprise keystroke.
pub fn normalize_shortcut(spec: &str) -> String {
    match parse_shortcut(spec) {
        Some((mods, vk)) => shortcut_spec(mods, vk),
        None => "none".to_string(),
    }
}

/// `(modifiers, virtual key)` for a stored spec, or None when it is off.
pub fn parse_shortcut(spec: &str) -> Option<(u8, u32)> {
    let spec = spec.trim().to_ascii_lowercase();
    if spec.is_empty() || spec == "none" || spec == "off" { return None; }
    let mut mods = 0u8;
    for part in spec.split('+') {
        match part.trim() {
            "ctrl" | "control" => mods |= MOD_CTRL,
            "shift" => mods |= MOD_SHIFT,
            "alt" => mods |= MOD_ALT,
            "win" => mods |= MOD_WIN,
            key => return shortcut_key(key).map(|vk| (mods, vk)),
        }
    }
    None
}

fn shortcut_key(name: &str) -> Option<u32> {
    if let Some((_, vk)) = SHORTCUT_KEYS.iter().find(|(n, _)| *n == name) { return Some(*vk); }
    let bytes = name.as_bytes();
    if bytes.len() == 1 {
        if bytes[0].is_ascii_lowercase() { return Some(bytes[0].to_ascii_uppercase() as u32); }
        if bytes[0].is_ascii_digit() { return Some(bytes[0] as u32); }
    }
    if let Some(number) = name.strip_prefix('f').and_then(|n| n.parse::<u32>().ok()) {
        if (1..=24).contains(&number) { return Some(0x70 + number - 1); }
    }
    None
}

fn shortcut_name(vk: u32) -> Option<&'static str> {
    if (0x41..=0x5A).contains(&vk) { return Some(LETTERS[(vk - 0x41) as usize]); }
    if (0x30..=0x39).contains(&vk) { return Some(DIGITS[(vk - 0x30) as usize]); }
    if (0x70..=0x87).contains(&vk) { return Some(FKEYS[(vk - 0x70) as usize]); }
    SHORTCUT_KEYS.iter().find(|(_, code)| *code == vk).map(|(name, _)| *name)
}

/// Inverse of parse_shortcut, in the same modifier order the label shows.
pub fn shortcut_spec(mods: u8, vk: u32) -> String {
    let Some(key) = shortcut_name(vk) else { return "none".to_string(); };
    let mut parts: Vec<&str> = Vec::new();
    if mods & MOD_CTRL != 0 { parts.push("ctrl"); }
    if mods & MOD_SHIFT != 0 { parts.push("shift"); }
    if mods & MOD_ALT != 0 { parts.push("alt"); }
    if mods & MOD_WIN != 0 { parts.push("win"); }
    parts.push(key);
    parts.join("+")
}

/// Human label for the settings window; the config file keeps the spec.
pub fn shortcut_label(spec: &str) -> String {
    let Some((mods, vk)) = parse_shortcut(spec) else { return "\u{672a}\u{8bbe}\u{7f6e}".to_string(); };
    let mut parts: Vec<String> = Vec::new();
    if mods & MOD_CTRL != 0 { parts.push("Ctrl".into()); }
    if mods & MOD_SHIFT != 0 { parts.push("Shift".into()); }
    if mods & MOD_ALT != 0 { parts.push("Alt".into()); }
    if mods & MOD_WIN != 0 { parts.push("Win".into()); }
    let name = shortcut_name(vk).unwrap_or("?");
    let pretty = match name {
        "lalt" => "\u{5de6} Alt".to_string(),
        "ralt" => "\u{53f3} Alt".to_string(),
        "lctrl" => "\u{5de6} Ctrl".to_string(),
        "rctrl" => "\u{53f3} Ctrl".to_string(),
        "lshift" => "\u{5de6} Shift".to_string(),
        "rshift" => "\u{53f3} Shift".to_string(),
        "space" => "\u{7a7a}\u{683c}".to_string(),
        "pageup" => "Page Up".to_string(),
        "pagedown" => "Page Down".to_string(),
        "capslock" => "Caps Lock".to_string(),
        "numlock" => "Num Lock".to_string(),
        "scrolllock" => "Scroll Lock".to_string(),
        "printscreen" => "Print Screen".to_string(),
        "backspace" => "Backspace".to_string(),
        other if other.len() == 1 => other.to_ascii_uppercase(),
        other if other.starts_with('f') && other[1..].bytes().all(|b| b.is_ascii_digit())
            => other.to_ascii_uppercase(),
        other => {
            let mut chars = other.chars();
            chars.next().map(|first| first.to_ascii_uppercase().to_string() + chars.as_str())
                .unwrap_or_default()
        }
    };
    parts.push(pretty);
    parts.join(" + ")
}

/// Self-describing label: the picker sits next to the checkbox, so the
/// value itself has to say what it counts.
pub fn interval_label(minutes: u32) -> String {
    if minutes >= 1440 { format!("\u{6bcf} {} \u{5929}", minutes / 1440) }
    else if minutes >= 60 { format!("\u{6bcf} {} \u{5c0f}\u{65f6}", minutes / 60) }
    else { format!("\u{6bcf} {minutes} \u{5206}\u{949f}") }
}

pub const DEFAULT_AZURE_URL: &str =
    "https://api.cognitive.microsofttranslator.com/translate?api-version=3.0&to=en";
pub const DEFAULT_OPENAI_URL: &str = "https://api.openai.com/v1/chat/completions";
pub const DEFAULT_OPENAI_MODEL: &str = "gpt-4o-mini";

/// Canonical Chat Completions endpoint. Accepts the shapes people paste: a bare
/// host, .../v1, .../v1/, .../v1/chat, the full endpoint with or without a
/// trailing slash; any query string survives untouched.
pub fn normalize_openai_url(url: &str) -> String {
    let trimmed = url.trim();
    if trimmed.is_empty() { return String::new(); }
    let (endpoint, suffix) = match trimmed.find(['?', '#']) {
        Some(index) => (&trimmed[..index], &trimmed[index..]),
        None => (trimmed, ""),
    };
    let mut path = endpoint.trim_end_matches('/').to_string();
    let lower = path.to_ascii_lowercase();
    if lower.ends_with("/chat/completions") {
        // Already canonical.
    } else if lower.ends_with("/chat") {
        path.push_str("/completions");
    } else if lower.ends_with("/v1") {
        path.push_str("/chat/completions");
    } else {
        // Keep whatever path prefix the provider documents, but give a bare
        // host the conventional /v1 root.
        let after_scheme = path.find("://").map_or(0, |index| index + 3);
        if !path[after_scheme..].contains('/') { path.push_str("/v1"); }
        path.push_str("/chat/completions");
    }
    path.push_str(suffix);
    path
}

/// Only the three documented values ever reach the file.
pub fn normalize_theme(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "light" => "light".into(),
        "dark" => "dark".into(),
        _ => "system".into(),
    }
}

/// Only explicit off-values disable a boolean setting.
pub fn truthy(value: &str, default: bool) -> bool {
    let value = value.trim().to_ascii_lowercase();
    if value.is_empty() { return default; }
    !matches!(value.as_str(), "off" | "false" | "0" | "no")
}

impl ConfigFile {
    pub fn read(base: &Path) -> Self {
        let mut map = HashMap::new();
        if let Ok(content) = fs::read_to_string(base.join("translator.conf")) {
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') { continue; }
                if let Some((key, value)) = line.split_once('=') {
                    map.insert(key.trim().to_ascii_lowercase(), value.trim().to_string());
                }
            }
        }
        let get = |key: &str, default: &str| map.get(key).cloned().unwrap_or_else(|| default.into());
        let flag = |key: &str, default: bool| map.get(key).is_none_or(|value| truthy(value, default));
        Self {
            provider: get("provider", "off").to_ascii_lowercase(),
            azure_url: get("azure_url", DEFAULT_AZURE_URL),
            azure_region: get("azure_region", ""),
            azure_key: super::secret::open(&get("azure_key", "")),
            openai_url: normalize_openai_url(&get("openai_url", DEFAULT_OPENAI_URL)),
            openai_model: get("openai_model", DEFAULT_OPENAI_MODEL),
            openai_key: super::secret::open(&get("openai_key", "")),
            timeout_seconds: get("timeout_seconds", "8"),
            debounce_ms: get("debounce_ms", "300"),
            // Older configs only had a boolean; keep a disabled setting disabled.
            alt_output_shortcut: normalize_shortcut(&get("alt_output_shortcut",
                if map.get("alt_output").is_some_and(|v| !truthy(v, true)) { "none" }
                else { DEFAULT_SHORTCUT })),
            auto_update: flag("auto_update", true),
            auto_install: flag("auto_install", true),
            update_interval_minutes: get("update_interval_minutes",
                &DEFAULT_UPDATE_INTERVAL.to_string()),
            theme: normalize_theme(&get("theme", "system")),
        }
    }

    /// First run only: write a commented sample without touching user values.
    pub fn ensure_sample(base: &Path) {
        let path = base.join("translator.conf");
        if fs::metadata(&path).is_ok() { return; }
        let sample = "# TransPop translator settings (UTF-8); use tray Settings to apply without restart.\n\
# provider=bing uses an undocumented Edge/Microsoft webpage endpoint; no API key.\n\
# The Bing mode may change, block automated calls, or enforce rate limits without notice.\n\
# Other choices: provider=azure or provider=openai; provider=off sends nothing.\n\
# API keys saved from Settings are encrypted with Windows DPAPI for the current user.\n\
# Environment variables TRANSPOP_AZURE_KEY / TRANSPOP_OPENAI_KEY remain safer.\n\
provider=off\n\
azure_url=https://api.cognitive.microsofttranslator.com/translate?api-version=3.0&to=en\n\
azure_region=\n\
azure_key=\n\
openai_url=https://api.openai.com/v1/chat/completions\n\
openai_model=gpt-4o-mini\n\
openai_key=\n\
# A local OpenAI-compatible endpoint can use http://127.0.0.1:PORT/v1/chat/completions.\n\
timeout_seconds=8\n\
debounce_ms=300\n\
# auto_update=on checks GitHub Releases in the background every\n\
# update_interval_minutes (min 15); auto_install=on then downloads and installs\n\
# a newer release after verifying the digest GitHub publishes for its asset,\n\
# while auto_install=off only announces the release and waits for a click.\n\
auto_update=on\n\
auto_install=on\n\
update_interval_minutes=30\n\
# alt_output_shortcut is pressed once (no chord) to paste the translation\n\
# instead of the Chinese candidate. Write none to disable it.\n\
alt_output_shortcut=lalt\n\
theme=system\n";
        let _ = fs::OpenOptions::new().write(true).create_new(true).open(&path)
            .and_then(|mut file| file.write_all(sample.as_bytes()));
    }
}

#[derive(Clone, Debug)]
pub struct SettingsValues {
    pub provider: String,
    pub azure_url: String,
    pub azure_region: String,
    pub azure_key: String,
    pub openai_url: String,
    pub openai_model: String,
    pub openai_key: String,
    pub timeout_seconds: String,
    pub debounce_ms: String,
    pub alt_output_shortcut: String,
    pub auto_update: bool,
    pub auto_install: bool,
    pub update_interval_minutes: String,
    pub theme: String,
}
impl SettingsValues {
    pub fn from_file(base: &Path) -> Self {
        let file = ConfigFile::read(base);
        Self {
            provider: file.provider,
            azure_url: file.azure_url,
            azure_region: file.azure_region,
            azure_key: file.azure_key,
            openai_url: file.openai_url,
            openai_model: file.openai_model,
            openai_key: file.openai_key,
            timeout_seconds: file.timeout_seconds,
            debounce_ms: file.debounce_ms,
            alt_output_shortcut: file.alt_output_shortcut,
            auto_update: file.auto_update,
            auto_install: file.auto_install,
            update_interval_minutes: file.update_interval_minutes,
            theme: file.theme,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if !["bing", "azure", "openai", "off"].contains(&self.provider.as_str()) {
            return Err("请选择翻译服务".into());
        }
        if self.provider == "azure" {
            if self.azure_key.trim().is_empty()
                && std::env::var("TRANSPOP_AZURE_KEY").map_or(true, |v| v.is_empty()) {
                return Err("Azure 模式需要 API Key（也可使用环境变量）".into());
            }
            if !self.azure_url.starts_with("https://") { return Err("Azure 地址必须使用 HTTPS".into()); }
        }
        if self.provider == "openai" {
            if !super::allowed_url(&self.openai_url) { return Err("接口地址必须是 HTTPS，或本机 HTTP".into()); }
            if self.openai_model.trim().is_empty() { return Err("请输入模型名称".into()); }
            if self.openai_key.is_empty()
                && std::env::var("TRANSPOP_OPENAI_KEY").map_or(true, |v| v.is_empty())
                && !super::is_loopback_http(&self.openai_url) {
                return Err("远程 OpenAI 兼容接口需要 API Key".into());
            }
        }
        if !self.timeout_seconds.parse::<u64>().is_ok_and(|n| (2..=30).contains(&n)) {
            return Err("请求超时请设置为 2–30 秒".into());
        }
        if !self.debounce_ms.parse::<u64>().is_ok_and(|n| (100..=1000).contains(&n)) {
            return Err("翻译防抖请设置为 100–1000 毫秒".into());
        }
        if !self.update_interval_minutes.parse::<u32>().is_ok_and(|n| n >= 15) {
            return Err("检查间隔请设置为 15 分钟以上".into());
        }
        if !["system", "light", "dark"].contains(&self.theme.as_str()) {
            return Err("未知的主题设置".into());
        }
        for v in [ &self.azure_url, &self.azure_region, &self.azure_key,
                   &self.openai_url, &self.openai_model, &self.openai_key ] {
            if v.contains('\n') || v.contains('\r') { return Err("配置值不能包含换行".into()); }
        }
        Ok(())
    }

    pub fn save(&self, base: &Path) -> Result<(), String> {
        // Store the canonical endpoint and theme so the file, the form and the
        // translator all agree on one spelling.
        let mut values = self.clone();
        values.openai_url = normalize_openai_url(&self.openai_url);
        values.theme = normalize_theme(&self.theme);
        values.validate()?;
        let path = base.join("translator.conf");
        // A failed read is not an empty file: rewriting from an empty base
        // would silently drop user comments and unknown keys.
        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(format!("无法读取 translator.conf，已取消保存（{e}）")),
        };
        // Keys are sealed with DPAPI for the current Windows user before they
        // reach the file; a seal failure keeps the old file untouched.
        let sealed_azure = super::secret::seal(&values.azure_key)
            .map_err(|e| format!("无法加密保存 Azure API Key（{e}）"))?;
        let sealed_openai = super::secret::seal(&values.openai_key)
            .map_err(|e| format!("无法加密保存 OpenAI API Key（{e}）"))?;
        let entries = [
            ("provider", values.provider.clone()),
            ("azure_url", values.azure_url.clone()),
            ("azure_region", values.azure_region.clone()),
            ("azure_key", sealed_azure),
            ("openai_url", values.openai_url.clone()),
            ("openai_model", values.openai_model.clone()),
            ("openai_key", sealed_openai),
            ("timeout_seconds", values.timeout_seconds.clone()),
            ("debounce_ms", values.debounce_ms.clone()),
            ("alt_output_shortcut", values.alt_output_shortcut.clone()),
            ("auto_update", if values.auto_update { "on".to_string() } else { "off".to_string() }),
            ("auto_install", if values.auto_install { "on".to_string() } else { "off".to_string() }),
            ("update_interval_minutes", values.update_interval_minutes.clone()),
            ("theme", values.theme.clone()),
        ];
        let mut seen = std::collections::HashSet::<String>::new();
        let mut result = String::new();
        // Preserve user comments and unknown keys, replacing only known values.
        for line in content.lines() {
            let raw_key = line.split_once('=').map(|(k, _)| k.trim().to_ascii_lowercase());
            if let Some(key) = raw_key {
                if let Some((_, value)) = entries.iter().find(|(name, _)| *name == key) {
                    if seen.insert(key.clone()) { result.push_str(&format!("{key}={value}\n")); }
                    continue;
                }
            }
            result.push_str(line);
            result.push('\n');
        }
        for (key, value) in &entries {
            if !seen.contains(*key) { result.push_str(&format!("{key}={value}\n")); }
        }
        // Normal Windows files are short; an error here must keep the form open.
        // Write a sibling temp file, flush it, then rename it into place, so a
        // crash or a full disk cannot leave a truncated translator.conf behind.
        let temp = base.join("translator.conf.tmp");
        let mut file = match fs::File::create(&temp) {
            Ok(file) => file,
            Err(_) => return Err("无法写入 translator.conf，请检查文件权限".to_string()),
        };
        let written = file.write_all(result.as_bytes()).and_then(|()| file.sync_all());
        drop(file);
        if written.is_err() || fs::rename(&temp, &path).is_err() {
            let _ = fs::remove_file(&temp);
            return Err("无法保存 translator.conf，请检查文件权限".to_string());
        }
        Ok(())
    }
}

// Legacy translator.conf files stored API keys as plaintext. Re-save once at
// startup so no upgrade leaves a cleartext secret on disk; a rejected save is
// logged and the app keeps working with the old file until the next save.
pub fn migrate_plaintext_secrets(base: &Path) {
    let Ok(content) = fs::read_to_string(base.join("translator.conf")) else { return };
    let has_plaintext = content.lines().any(|line| {
        let line = line.trim();
        if line.starts_with('#') { return false; }
        let Some((key, value)) = line.split_once('=') else { return false };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        matches!(key.as_str(), "azure_key" | "openai_key")
            && !value.is_empty() && !super::secret::is_sealed(value)
    });
    if !has_plaintext { return; }
    match SettingsValues::from_file(base).save(base) {
        Ok(()) => super::log_global("SECRETS_ENCRYPTED"),
        Err(reason) => super::log_global(format!("SECRETS_ENCRYPT_SKIPPED {reason}")),
    }
}

pub struct SettingsWindow {
    pub hwnd: HWND,
    base: PathBuf,
    dpi: i32,
    font: HFONT,
    title_font: HFONT,
    section_font: HFONT,
    note_font: HFONT,
    theme: Box<UiTheme>,
    brand_mark: HWND,
    heading: HWND,
    service_title: HWND,
    service_divider: HWND,
    provider: HWND,
    connection_title: HWND,
    connection_divider: HWND,
    azure: [(HWND, HWND); 3],
    openai: [(HWND, HWND); 3],
    azure_reveal: HWND,
    openai_reveal: HWND,
    behavior_title: HWND,
    behavior_divider: HWND,
    timeout_label: HWND,
    timeout: HWND,
    timeout_unit: HWND,
    debounce_label: HWND,
    debounce: HWND,
    debounce_unit: HWND,
    autostart: HWND,
    shortcut_label: HWND,
    shortcut_key: HWND,
    mod_shift: HWND,
    mod_ctrl: HWND,
    mod_alt: HWND,
    mod_win: HWND,
    shortcut_hint: HWND,
    auto_update: HWND,
    auto_install: HWND,
    update_interval: HWND,
    update_intervals: Vec<u32>,
    update_button: HWND,
    startup_title: HWND,
    update_title: HWND,
    interval_caption: HWND,
    theme_label: HWND,
    theme_picker: HWND,
    tabs: [HWND; 5],
    about_text: HWND,
}

impl SettingsWindow {
    pub unsafe fn open(base: PathBuf, dpi: i32) -> Option<Self> {
        let instance = GetModuleHandleW(ptr::null());
        let class = wide(CLASS);
        let mut wc: WNDCLASSW = mem::zeroed();
        wc.lpfnWndProc = Some(proc);
        wc.hInstance = instance;
        wc.lpszClassName = class.as_ptr();
        wc.hbrBackground = GetStockObject(0) as _; // light fallback; WM_ERASEBKGND paints
        wc.hIcon = super::load_brand_icon(false);
        let _ = RegisterClassW(&wc);

        let scale = |v: i32| v * dpi / 96;
        // A throwaway frame size: apply_layout sizes the window from the client grid.
        let title = wide("TransPop · 偏好设置");
        let hwnd = CreateWindowExW(WINDOW_EX_STYLE, class.as_ptr(), title.as_ptr(), WINDOW_STYLE,
            0, 0, scale(CLIENT_W), scale(400), ptr::null_mut(), ptr::null_mut(), instance, ptr::null());
        if hwnd.is_null() { return None; }

        // One boxed paint state. Its address goes into GWLP_USERDATA and must
        // stay valid while the SettingsWindow itself is moved into the caller.
        let mut theme = Box::new(UiTheme::create(Palette::detect(&base), dpi));
        let theme_ptr: *mut UiTheme = &mut *theme;
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, theme_ptr as isize);

        let font_face = wide("Segoe UI");
        let font = CreateFontW(-scale(14), 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 0, font_face.as_ptr());
        let title_font = CreateFontW(-scale(20), 0, 0, 0, 600, 0, 0, 0, 1, 0, 0, 5, 0, font_face.as_ptr());
        let section_font = CreateFontW(-scale(14), 0, 0, 0, 600, 0, 0, 0, 1, 0, 0, 5, 0, font_face.as_ptr());
        let note_font = CreateFontW(-scale(12), 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 0, font_face.as_ptr());
        // No coordinates here on purpose: layout() owns every position and size.
        let create = |kind: &str, text: &str, id: u16, style: u32| -> HWND {
            let c = wide(kind); let t = wide(text);
            let child = CreateWindowExW(0, c.as_ptr(), t.as_ptr(), WS_CHILD | WS_VISIBLE | style,
                0, 0, 0, 0, hwnd, id as usize as HWND, instance, ptr::null());
            if !child.is_null() && !font.is_null() { SendMessageW(child, WM_SETFONT, font as usize, 1); }
            child
        };
        let set_font = |child: HWND, font: HFONT| {
            if !child.is_null() && !font.is_null() { SendMessageW(child, WM_SETFONT, font as usize, 1); }
        };
        let values = SettingsValues::from_file(&base);

        // Header: mark and product name only; version lives on 关于.
        let mark = create("STATIC", "", 0, SS_ICON);
        let brand_icon = super::load_brand_icon(false);
        if !mark.is_null() && !brand_icon.is_null() {
            SendMessageW(mark, STM_SETICON, brand_icon as usize, 0);
        }
        let heading = create("STATIC", "TransPop", 0, 0);
        set_font(heading, title_font);

        let service_divider = create("STATIC", "", 0, 0);

        // Tab strip: four owner-drawn buttons, one page each. Owner drawing
        // keeps the selected tab readable in the dark palette, which the
        // stock tab control cannot do on Windows 10.
        let tab_translate = create("BUTTON", "翻译", ID_TAB_TRANSLATE,
            BS_OWNERDRAW | WS_TABSTOP);
        let tab_appearance = create("BUTTON", "外观", ID_TAB_APPEARANCE,
            BS_OWNERDRAW | WS_TABSTOP);
        let tab_system = create("BUTTON", "系统", ID_TAB_SYSTEM,
            BS_OWNERDRAW | WS_TABSTOP);
        let tab_shortcut = create("BUTTON", "快捷键", ID_TAB_SHORTCUT,
            BS_OWNERDRAW | WS_TABSTOP);
        let tab_about = create("BUTTON", "关于", ID_TAB_ABOUT,
            BS_OWNERDRAW | WS_TABSTOP);

        // Translation service.
        let service_title = create("STATIC", "翻译服务", 0, 0);
        set_font(service_title, section_font);
        let provider = create("COMBOBOX", "", ID_PROVIDER, CBS_DROPDOWN | WS_TABSTOP);
        for label in ["Bing 翻译（无需密钥）", "Azure Translator", "OpenAI 兼容接口", "关闭翻译"] {
            let item = wide(label);
            SendMessageW(provider, CB_ADDSTRING, 0, item.as_ptr() as isize);
        }
        let pos = match values.provider.as_str() { "bing" => 0, "azure" => 1, "openai" => 2, _ => 3 };
        SendMessageW(provider, CB_SETCURSEL, pos, 0);
        // A read-only drop-down keeps a real edit child: Windows 10 only honours
        // theme colours on that child, a drop-down list paints itself.
        let provider_edit = GetWindow(provider, GW_CHILD);
        if !provider_edit.is_null() { SendMessageW(provider_edit, EM_SETREADONLY, 1, 0); }

        // Service connection; only visible for providers that need credentials.
        let connection_divider = create("STATIC", "", 0, 0);
        let connection_title = create("STATIC", "服务连接", 0, 0);
        set_font(connection_title, section_font);
        let field = |label: &str, value: &str, secret_id: u16| -> (HWND, HWND) {
            let label_hwnd = create("STATIC", label, 0, 0);
            let password = if secret_id != 0 { ES_PASSWORD } else { 0 };
            let edit_hwnd = create("EDIT", value, secret_id,
                WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL | password);
            (label_hwnd, edit_hwnd)
        };
        let azure = [
            field("翻译地址", &values.azure_url, 0),
            field("资源区域", &values.azure_region, 0),
            field("API Key", &values.azure_key, ID_AZURE_SECRET),
        ];
        let openai = [
            field("接口地址", &values.openai_url, 0),
            field("模型名称", &values.openai_model, 0),
            field("API Key", &values.openai_key, ID_OPENAI_SECRET),
        ];
        let azure_reveal = create("BUTTON", "显示", ID_AZURE_REVEAL, BS_OWNERDRAW | WS_TABSTOP);
        let openai_reveal = create("BUTTON", "显示", ID_OPENAI_REVEAL, BS_OWNERDRAW | WS_TABSTOP);

        // Translation behaviour.
        let behavior_divider = create("STATIC", "", 0, 0);
        let behavior_title = create("STATIC", "翻译行为", 0, 0);
        set_font(behavior_title, section_font);
        let timeout_label = create("STATIC", "请求超时", 0, 0);
        let timeout = create("EDIT", &values.timeout_seconds, 0,
            WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL);
        let timeout_unit = create("STATIC", "秒", 0, 0);
        let debounce_label = create("STATIC", "翻译防抖", 0, 0);
        let debounce = create("EDIT", &values.debounce_ms, 0,
            WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL);
        let debounce_unit = create("STATIC", "ms", 0, 0);
        let autostart = create("BUTTON", "开机自启", ID_AUTOSTART,
            BS_OWNERDRAW | WS_TABSTOP);
        // An unreadable registry state must not look like "disabled": the next
        // Save would then delete a working autostart entry. Assume enabled.
        let previously_enabled = match super::startup::enabled_for_current_exe() {
            Ok(enabled) => enabled,
            Err(reason) => {
                super::log_global(format!("AUTOSTART_STATE_UNREADABLE {reason}"));
                true
            }
        };
        set_button_checked(autostart, previously_enabled);

        // Two separate switches: checking for a release and installing it.
        let auto_update = create("BUTTON", "检查更新", ID_AUTO_UPDATE,
            BS_OWNERDRAW | WS_TABSTOP);
        set_button_checked(auto_update, values.auto_update);
        let auto_install = create("BUTTON", "自动更新", ID_AUTO_INSTALL,
            BS_OWNERDRAW | WS_TABSTOP);
        set_button_checked(auto_install, values.auto_install);
        // Interval between background checks; the current value may be one
        // the presets no longer offer, so it is kept as an extra entry.
        let update_interval = create("COMBOBOX", "", ID_UPDATE_INTERVAL,
            CBS_DROPDOWN | WS_TABSTOP);
        let current: u32 = values.update_interval_minutes.parse().unwrap_or(DEFAULT_UPDATE_INTERVAL);
        let mut intervals = UPDATE_INTERVALS.to_vec();
        if !intervals.contains(&current) { intervals.push(current); intervals.sort_unstable(); }
        for minutes in &intervals {
            let item = wide(&interval_label(*minutes));
            SendMessageW(update_interval, CB_ADDSTRING, 0, item.as_ptr() as isize);
        }
        let interval_pos = intervals.iter().position(|m| *m == current).unwrap_or(0);
        SendMessageW(update_interval, CB_SETCURSEL, interval_pos, 0);
        let interval_edit = GetWindow(update_interval, GW_CHILD);
        if !interval_edit.is_null() { SendMessageW(interval_edit, EM_SETREADONLY, 1, 0); }
        let update_button = create("BUTTON", "立即检查", ID_UPDATE, WS_TABSTOP);
        let startup_title = create("STATIC", "启动", 0, 0);
        set_font(startup_title, section_font);
        let update_title = create("STATIC", "更新", 0, 0);
        set_font(update_title, section_font);
        let interval_caption = create("STATIC", "检查间隔", 0, 0);
        // In-app theme switch: system, light, dark.
        let theme_label = create("STATIC", "主题", 0, 0);
        set_font(theme_label, section_font);
        let theme_picker = create("COMBOBOX", "", ID_THEME, CBS_DROPDOWN | WS_TABSTOP);
        for label in ["跟随系统", "浅色", "深色"] {
            let item = wide(label);
            SendMessageW(theme_picker, CB_ADDSTRING, 0, item.as_ptr() as isize);
        }
        let theme_pos = match values.theme.as_str() { "light" => 1, "dark" => 2, _ => 0 };
        SendMessageW(theme_picker, CB_SETCURSEL, theme_pos, 0);
        let picker_edit = GetWindow(theme_picker, GW_CHILD);
        if !picker_edit.is_null() { SendMessageW(picker_edit, EM_SETREADONLY, 1, 0); }
        // No Save button and no footer: every change applies at once and the
        // caption bar already closes the window.
        let shortcut_caption = create("STATIC", "输入英文", 0, 0);
        set_font(shortcut_caption, section_font);
        // One key list plus modifier switches covers any single key or chord
        // without needing a modal capture loop inside the window procedure.
        let shortcut_key = create("COMBOBOX", "", ID_SHORTCUT_KEY,
            CBS_DROPDOWN | WS_TABSTOP);
        let key_items = shortcut_key_items();
        for item in &key_items {
            let text = wide(item);
            SendMessageW(shortcut_key, CB_ADDSTRING, 0, text.as_ptr() as isize);
        }
        let (saved_mods, saved_vk) = parse_shortcut(&values.alt_output_shortcut)
            .unwrap_or((0, 0));
        let key_index = SHORTCUT_CHOICES.iter().position(|(_, vk)| *vk == saved_vk)
            .map(|index| index as isize + 1).unwrap_or(0);
        SendMessageW(shortcut_key, CB_SETCURSEL, key_index as usize, 0);
        let key_edit = GetWindow(shortcut_key, GW_CHILD);
        if !key_edit.is_null() { SendMessageW(key_edit, EM_SETREADONLY, 1, 0); }
        let mod_shift = create("BUTTON", "Shift", ID_MOD_SHIFT, BS_OWNERDRAW | WS_TABSTOP);
        let mod_ctrl = create("BUTTON", "Ctrl", ID_MOD_CTRL, BS_OWNERDRAW | WS_TABSTOP);
        let mod_alt = create("BUTTON", "Alt", ID_MOD_ALT, BS_OWNERDRAW | WS_TABSTOP);
        let mod_win = create("BUTTON", "Win", ID_MOD_WIN, BS_OWNERDRAW | WS_TABSTOP);
        set_button_checked(mod_shift, saved_mods & MOD_SHIFT != 0);
        set_button_checked(mod_ctrl, saved_mods & MOD_CTRL != 0);
        set_button_checked(mod_alt, saved_mods & MOD_ALT != 0);
        set_button_checked(mod_win, saved_mods & MOD_WIN != 0);
        let shortcut_hint = create("STATIC",
            "不按下方的修饰键时，单按这个键即可；选「未设置」则功能关闭。", 0, 0);
        set_font(shortcut_hint, note_font);
        let about_text = create("STATIC", &format!(
            "TransPop v{}\r\n\r\n在微软拼音候选窗下方实时显示英文翻译。\r\n\r\n仓库：github.com/gurio-wine/TransPop\r\n许可：MIT",
            super::VERSION), 0, 0);

        // A slightly inset caret keeps addresses off the field frame.
        let pad = scale(8);
        for edit in [azure[0].1, azure[1].1, azure[2].1, openai[0].1, openai[1].1, openai[2].1,
                     timeout, debounce] {
            if !edit.is_null() {
                SendMessageW(edit, EM_SETMARGINS, EC_LEFT_RIGHT_MARGINS,
                    (pad | (pad << 16)) as isize);
            }
        }
        // Which statics are secondary text and which are hairlines.
        for control in [service_divider, connection_divider, behavior_divider] {
            (*theme_ptr).lines.push(control as isize);
        }
        // Drop-downs whose closed field the theme has to paint itself.
        (*theme_ptr).combos.push(provider as isize);
        (*theme_ptr).combos.push(theme_picker as isize);
        (*theme_ptr).combos.push(update_interval as isize);
        for tab in [tab_translate, tab_appearance, tab_system, tab_shortcut, tab_about] {
            (*theme_ptr).tabs.push(tab as isize);
        }
        apply_window_theme(hwnd, &mut *theme_ptr);

        let this = Self { hwnd, base, dpi, font, title_font, section_font, note_font, theme,
            brand_mark: mark, heading, service_title, service_divider, provider,
            connection_title, connection_divider, azure, openai, azure_reveal, openai_reveal,
            behavior_title, behavior_divider, timeout_label, timeout, timeout_unit,
            debounce_label, debounce, debounce_unit, autostart,
            shortcut_label: shortcut_caption, shortcut_key, mod_shift, mod_ctrl,
            mod_alt, mod_win, shortcut_hint,
            auto_update, auto_install, update_interval, update_intervals: intervals,
            update_button, startup_title, update_title, interval_caption,
            theme_label, theme_picker,
            tabs: [tab_translate, tab_appearance, tab_system, tab_shortcut, tab_about],
            about_text };
        this.apply_layout(true);
        ShowWindow(hwnd, SW_SHOW);
        SetForegroundWindow(hwnd);
        SetFocus(provider);
        Some(this)
    }

    pub unsafe fn activate(&self) {
        if IsWindow(self.hwnd) != 0 {
            ShowWindow(self.hwnd, SW_SHOW);
            SetForegroundWindow(self.hwnd);
        }
    }

    /// Positions every control and returns the client height the form needs.
    /// This is the only place that knows coordinates; visibility follows the
    /// selected provider, so switching providers only needs a new layout pass.
    unsafe fn layout(&self) -> i32 {
        let s = |value: i32| value * self.dpi / 96;
        let place = |control: HWND, x: i32, y: i32, width: i32, height: i32| {
            if !control.is_null() {
                SetWindowPos(control, ptr::null_mut(), s(x), s(y), s(width), s(height),
                    SWP_NOZORDER | SWP_NOACTIVATE);
            }
        };
        let show = |control: HWND, visible: bool| {
            if !control.is_null() {
                ShowWindow(control, if visible { SW_SHOW } else { SW_HIDE });
            }
        };

        place(self.brand_mark, MARGIN_X, 18, 34, 34);
        place(self.heading, 74, 12, 440, 32);

        place(self.service_divider, MARGIN_X, 80, CONTENT_W, 1);

        // Tab strip: a fixed row under the header.
        for (index, tab) in self.tabs.iter().enumerate() {
            place(*tab, MARGIN_X + index as i32 * (TAB_W + TAB_GAP), TAB_Y, TAB_W, TAB_H);
        }

        // The three pages share one origin; only the active page is shown, so
        // every page can use the same coordinates without fighting.
        let top = CONTENT_TOP;
        let selected = SendMessageW(self.provider, CB_GETCURSEL, 0, 0);
        let advanced = selected == 1 || selected == 2;
        let translating = (*self.theme).page == 0;
        let appearance = (*self.theme).page == 1;
        let system = (*self.theme).page == 2;
        let shortcuts = (*self.theme).page == 3;
        let about = (*self.theme).page == 4;

        // Translation page. The credential block only exists for providers that
        // need one, so 翻译行为 slides up instead of leaving a hole.
        place(self.service_title, MARGIN_X, top, 460, 22);
        place(self.provider, MARGIN_X, top + 28, CONTENT_W, 30);
        let mut y = top + 74;
        place(self.connection_title, MARGIN_X, y, 460, 22);
        let rows = if selected == 1 { &self.azure } else { &self.openai };
        let reveal = if selected == 1 { self.azure_reveal } else { self.openai_reveal };
        for (row, (label, edit)) in rows.iter().enumerate() {
            let row_y = y + 28 + row as i32 * 36;
            place(*label, MARGIN_X, row_y + 4, LABEL_W, 22);
            let width = if row == 2 { SECRET_W } else { FIELD_W };
            place(*edit, FIELD_X, row_y, width, 28);
            if row == 2 { place(reveal, 485, row_y + 3, 61, 22); }
        }
        if advanced { y += 28 + 2 * 36 + 28; }
        place(self.behavior_divider, MARGIN_X, y, CONTENT_W, 1);
        y += 18;
        place(self.behavior_title, MARGIN_X, y, 460, 22);
        y += 32;
        place(self.timeout_label, MARGIN_X, y + 4, LABEL_W, 22);
        place(self.timeout, FIELD_X, y, 90, 28);
        place(self.timeout_unit, FIELD_X + 98, y + 4, 140, 22);
        y += 36;
        place(self.debounce_label, MARGIN_X, y + 4, LABEL_W, 22);
        place(self.debounce, FIELD_X, y, 84, 28);
        place(self.debounce_unit, FIELD_X + 92, y + 4, 60, 22);

        // Appearance page.
        place(self.theme_label, MARGIN_X, top, 460, 22);
        place(self.theme_picker, MARGIN_X, top + 28, 220, 30);

        // System page: two titled groups with a rule between them, so startup
        // and updates never read as one pile of switches.
        place(self.startup_title, MARGIN_X, top, 460, 22);
        place(self.autostart, MARGIN_X, top + 28, CONTENT_W, 24);
        place(self.connection_divider, MARGIN_X, top + 70, CONTENT_W, 1);
        place(self.update_title, MARGIN_X, top + 88, 460, 22);
        place(self.auto_update, MARGIN_X, top + 116, CONTENT_W, 24);
        place(self.interval_caption, MARGIN_X, top + 149, LABEL_W, 22);
        place(self.update_interval, FIELD_X, top + 145, 160, 30);
        place(self.auto_install, MARGIN_X, top + 185, CONTENT_W, 24);
        place(self.update_button, MARGIN_X, top + 219, 112, 30);

        // 快捷键 page: one key, then the modifiers it is combined with.
        place(self.shortcut_label, MARGIN_X, top, 460, 22);
        place(self.shortcut_key, MARGIN_X, top + 28, 220, 30);
        place(self.mod_shift, MARGIN_X, top + 74, 120, 24);
        place(self.mod_ctrl, MARGIN_X + 132, top + 74, 120, 24);
        place(self.mod_alt, MARGIN_X, top + 104, 120, 24);
        place(self.mod_win, MARGIN_X + 132, top + 104, 120, 24);
        place(self.shortcut_hint, MARGIN_X, top + 144, CONTENT_W, 44);

        // About page.
        place(self.about_text, MARGIN_X, top, CONTENT_W, 200);

        for control in [self.service_title, self.provider] { show(control, translating); }
        for (label, edit) in self.azure {
            show(label, translating && selected == 1);
            show(edit, translating && selected == 1);
        }
        for (label, edit) in self.openai {
            show(label, translating && selected == 2);
            show(edit, translating && selected == 2);
        }
        show(self.connection_title, translating && advanced);
        show(self.azure_reveal, translating && selected == 1);
        show(self.openai_reveal, translating && selected == 2);
        for control in [self.behavior_divider, self.behavior_title, self.timeout_label,
                        self.timeout, self.timeout_unit, self.debounce_label, self.debounce,
                        self.debounce_unit] {
            show(control, translating);
        }
        show(self.theme_label, appearance);
        show(self.theme_picker, appearance);
        for control in [self.autostart, self.auto_update, self.auto_install,
                        self.update_interval, self.update_button, self.startup_title,
                        self.update_title, self.interval_caption] {
            show(control, system);
        }
        for control in [self.shortcut_label, self.shortcut_key, self.mod_shift,
                        self.mod_ctrl, self.mod_alt, self.mod_win, self.shortcut_hint] {
            show(control, shortcuts);
        }
        show(self.connection_divider, system);
        show(self.about_text, about);

        CLIENT_H
    }

    /// Resizes the frame to the client area the layout needs. Centring happens
    /// only on first show; provider switches keep the window where the user put it.
    unsafe fn apply_layout(&self, center: bool) {
        let client_h = self.layout();
        let s = |value: i32| value * self.dpi / 96;
        let mut rect = RECT { left: 0, top: 0, right: s(CLIENT_W), bottom: s(client_h) };
        AdjustWindowRectEx(&mut rect, WINDOW_STYLE, 0, WINDOW_EX_STYLE);
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        let (x, y) = self.placement(width, height, center);
        SetWindowPos(self.hwnd, ptr::null_mut(), x, y, width, height,
            SWP_NOZORDER | SWP_NOACTIVATE);
    }

    unsafe fn placement(&self, width: i32, height: i32, center: bool) -> (i32, i32) {
        let monitor = if center {
            let mut cursor = POINT { x: 0, y: 0 };
            GetCursorPos(&mut cursor);
            MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST)
        } else {
            MonitorFromWindow(self.hwnd, MONITOR_DEFAULTTONEAREST)
        };
        let mut info: MONITORINFO = mem::zeroed();
        info.cbSize = mem::size_of::<MONITORINFO>() as u32;
        let work = if !monitor.is_null() && GetMonitorInfoW(monitor, &mut info) != 0
            && info.rcWork.right > info.rcWork.left && info.rcWork.bottom > info.rcWork.top {
            info.rcWork
        } else {
            RECT { left: 0, top: 0, right: GetSystemMetrics(0), bottom: GetSystemMetrics(1) }
        };
        let mut current: RECT = mem::zeroed();
        GetWindowRect(self.hwnd, &mut current);
        let (x, y) = if center {
            (work.left + (work.right - work.left - width) / 2,
             work.top + (work.bottom - work.top - height) / 2)
        } else {
            (current.left, current.top)
        };
        // Keep the dialog inside the work area, taskbar included.
        (x.max(work.left).min((work.right - width).max(work.left)),
         y.max(work.top).min((work.bottom - height).max(work.top)))
    }

    /// Provider switches only move controls inside the fixed frame.
    pub unsafe fn update_provider(&self) {
        self.apply_layout(false);
    }

    /// Switches the visible page. The frame keeps its size, so the dialog
    /// never jumps when the user browses tabs.
    pub unsafe fn show_page(&self, page: usize) {
        let page = page.min(4);
        let theme = self.theme.as_ref() as *const UiTheme as *mut UiTheme;
        unsafe { (*theme).page = page; }
        self.layout();
        for tab in self.tabs {
            if !tab.is_null() { InvalidateRect(tab, ptr::null(), 0); }
        }
    }

    /// Re-reads translator.conf into the widgets. Only used after a rejected
    /// value, so the form never keeps something that failed to save and then
    /// writes it again on the next keystroke.
    pub unsafe fn reload_fields(&self) {
        let values = SettingsValues::from_file(&self.base);
        let set = |control: HWND, text: &str| {
            if !control.is_null() {
                let wide = wide(text);
                SetWindowTextW(control, wide.as_ptr());
            }
        };
        set(self.azure[0].1, &values.azure_url);
        set(self.azure[1].1, &values.azure_region);
        set(self.openai[0].1, &values.openai_url);
        set(self.openai[1].1, &values.openai_model);
        set(self.timeout, &values.timeout_seconds);
        set(self.debounce, &values.debounce_ms);
        set_button_checked(self.autostart, self.autostart_checked());
        let (mods, vk) = parse_shortcut(&values.alt_output_shortcut).unwrap_or((0, 0));
        let index = SHORTCUT_CHOICES.iter().position(|(_, code)| *code == vk)
            .map(|index| index as usize + 1).unwrap_or(0);
        SendMessageW(self.shortcut_key, CB_SETCURSEL, index, 0);
        set_button_checked(self.mod_shift, mods & MOD_SHIFT != 0);
        set_button_checked(self.mod_ctrl, mods & MOD_CTRL != 0);
        set_button_checked(self.mod_alt, mods & MOD_ALT != 0);
        set_button_checked(self.mod_win, mods & MOD_WIN != 0);
        set_button_checked(self.auto_update, values.auto_update);
        set_button_checked(self.auto_install, values.auto_install);
        let current: u32 = values.update_interval_minutes.parse()
            .unwrap_or(DEFAULT_UPDATE_INTERVAL);
        if let Some(index) = self.update_intervals.iter().position(|m| *m == current) {
            SendMessageW(self.update_interval, CB_SETCURSEL, index, 0);
        }
    }

    /// There is no footer any more; a rejected value could not be applied, so
    /// the dialog shows the reason inline where the shortcut is captured and
    /// the caller only logs the event.
    pub unsafe fn set_status(&self, _text: &str) {}

    unsafe fn get_text(hwnd: HWND) -> String {
        if hwnd.is_null() { return String::new(); }
        let len = GetWindowTextLengthW(hwnd);
        if len < 0 || len > 4096 { return String::new(); }
        let mut buf = vec![0u16; len as usize + 1];
        let count = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        String::from_utf16_lossy(&buf[..count.max(0) as usize]).trim().to_string()
    }

    pub unsafe fn gather(&self) -> SettingsValues {
        let provider = match SendMessageW(self.provider, CB_GETCURSEL, 0, 0) {
            0 => "bing", 1 => "azure", 2 => "openai", _ => "off",
        }.to_string();
        SettingsValues {
            provider,
            azure_url: Self::get_text(self.azure[0].1),
            azure_region: Self::get_text(self.azure[1].1),
            azure_key: Self::get_text(self.azure[2].1),
            openai_url: Self::get_text(self.openai[0].1),
            openai_model: Self::get_text(self.openai[1].1),
            openai_key: Self::get_text(self.openai[2].1),
            timeout_seconds: Self::get_text(self.timeout),
            debounce_ms: Self::get_text(self.debounce),
            alt_output_shortcut: {
                let index = SendMessageW(self.shortcut_key, CB_GETCURSEL, 0, 0);
                if index <= 0 {
                    "none".to_string()
                } else {
                    let vk = SHORTCUT_CHOICES[(index - 1) as usize].1;
                    let mut mods = 0u8;
                    if button_checked(self.mod_ctrl) { mods |= MOD_CTRL; }
                    if button_checked(self.mod_shift) { mods |= MOD_SHIFT; }
                    if button_checked(self.mod_alt) { mods |= MOD_ALT; }
                    if button_checked(self.mod_win) { mods |= MOD_WIN; }
                    shortcut_spec(mods, vk)
                }
            },
            auto_update: button_checked(self.auto_update),
            auto_install: button_checked(self.auto_install),
            update_interval_minutes: {
                let index = SendMessageW(self.update_interval, CB_GETCURSEL, 0, 0);
                let minutes = self.update_intervals.get(index.max(0) as usize)
                    .copied().unwrap_or(DEFAULT_UPDATE_INTERVAL);
                minutes.to_string()
            },
            theme: match SendMessageW(self.theme_picker, CB_GETCURSEL, 0, 0) {
                1 => "light".to_string(),
                2 => "dark".to_string(),
                _ => "system".to_string(),
            },
        }
    }
    pub unsafe fn autostart_checked(&self) -> bool {
        button_checked(self.autostart)
    }
    pub unsafe fn set_autostart_checked(&self, checked: bool) {
        set_button_checked(self.autostart, checked);
    }
    pub fn base(&self) -> &Path { &self.base }

    pub unsafe fn close(self) {
        // Clear the paint state pointer before children are destroyed.
        SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
        DestroyWindow(self.hwnd);
        for font in [self.font, self.title_font, self.section_font, self.note_font] {
            if !font.is_null() { DeleteObject(font); }
        }
        self.theme.release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_base(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("transpop-settings-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn save_preserves_comments_unknown_keys_and_replaces_atomically() {
        let base = temp_base("save");
        let path = base.join("translator.conf");
        fs::write(&path, "# keep me\nprovider=bing\nunknown_key=1\n").unwrap();
        let values = SettingsValues::from_file(&base);
        assert_eq!(values.provider, "bing");
        values.save(&base).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("# keep me"), "comment lost: {text}");
        assert!(text.contains("unknown_key=1"), "unknown key lost: {text}");
        assert!(text.contains("provider=bing"));
        assert!(!base.join("translator.conf.tmp").exists(), "temp file left behind");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn provider_from_file_is_normalized() {
        let base = temp_base("provider");
        fs::write(base.join("translator.conf"), "provider=Bing\n").unwrap();
        assert_eq!(SettingsValues::from_file(&base).provider, "bing");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn save_seals_keys_and_from_file_decrypts_them() {
        let base = temp_base("sealed");
        let path = base.join("translator.conf");
        fs::write(&path, "provider=openai\nopenai_key=sk-plain-123\n").unwrap();
        let values = SettingsValues::from_file(&base);
        assert_eq!(values.openai_key, "sk-plain-123");
        values.save(&base).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("openai_key=dpapi:"), "key not sealed: {text}");
        assert!(!text.contains("sk-plain-123"), "plaintext key left on disk");
        assert_eq!(SettingsValues::from_file(&base).openai_key, "sk-plain-123");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn startup_migration_encrypts_legacy_plaintext_keys() {
        let base = temp_base("migrate");
        let path = base.join("translator.conf");
        fs::write(&path, "# keep me\nprovider=bing\nazure_key=legacy-azure\n").unwrap();
        migrate_plaintext_secrets(&base);
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("azure_key=dpapi:"), "key not migrated: {text}");
        assert!(!text.contains("legacy-azure"), "plaintext key left on disk");
        assert!(text.contains("# keep me"), "comment lost: {text}");
        assert_eq!(SettingsValues::from_file(&base).azure_key, "legacy-azure");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn startup_migration_leaves_sealed_files_untouched() {
        let base = temp_base("sealed-untouched");
        let path = base.join("translator.conf");
        let original = "provider=bing\nazure_key=dpapi:00112233\n";
        fs::write(&path, original).unwrap();
        migrate_plaintext_secrets(&base);
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn shared_parser_feeds_settings_and_runtime_defaults() {
        let base = temp_base("shared");
        fs::write(base.join("translator.conf"),
            "provider=OpenAI\nauto_update=off\nalt_output_shortcut=ctrl+space\n             openai_url=http://127.0.0.1:8080/v1/chat/completions\n")
            .unwrap();
        let file = ConfigFile::read(&base);
        assert_eq!(file.provider, "openai");
        assert!(!file.auto_update);
        assert_eq!(file.alt_output_shortcut, "ctrl+space");
        assert_eq!(file.openai_url, "http://127.0.0.1:8080/v1/chat/completions");
        assert_eq!(file.azure_url, DEFAULT_AZURE_URL);
        assert_eq!(file.openai_model, DEFAULT_OPENAI_MODEL);
        let values = SettingsValues::from_file(&base);
        assert_eq!(values.provider, file.provider);
        assert_eq!(values.auto_update, file.auto_update);
        assert!(truthy("on", false));
        assert!(!truthy("No", true));
        assert!(truthy("", true));
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn sample_file_is_written_once_and_parsed() {
        let base = temp_base("sample");
        ConfigFile::ensure_sample(&base);
        let text = fs::read_to_string(base.join("translator.conf")).unwrap();
        assert!(text.contains("auto_update=on"), "sample missing auto_update: {text}");
        assert!(ConfigFile::read(&base).auto_update);
        ConfigFile::ensure_sample(&base); // never overwrites an existing file
        assert_eq!(fs::read_to_string(base.join("translator.conf")).unwrap(), text);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn openai_url_accepts_base_paths_and_appends_the_endpoint() {
        let cases = [
            ("https://api.example.com", "https://api.example.com/v1/chat/completions"),
            ("https://api.example.com/", "https://api.example.com/v1/chat/completions"),
            ("https://api.example.com/v1", "https://api.example.com/v1/chat/completions"),
            ("https://api.example.com/v1/", "https://api.example.com/v1/chat/completions"),
            ("https://api.example.com/v1/chat", "https://api.example.com/v1/chat/completions"),
            ("https://api.example.com/v1/chat/", "https://api.example.com/v1/chat/completions"),
            ("https://api.example.com/v1/chat/completions",
                "https://api.example.com/v1/chat/completions"),
            ("https://api.example.com/v1/chat/completions/",
                "https://api.example.com/v1/chat/completions"),
            ("https://api.example.com/api/v3", "https://api.example.com/api/v3/chat/completions"),
            ("  https://api.example.com/v1?x=1  ",
                "https://api.example.com/v1/chat/completions?x=1"),
        ];
        for (input, expected) in cases {
            assert_eq!(normalize_openai_url(input), expected, "input {input}");
        }
        assert_eq!(normalize_openai_url(""), "");
    }

    #[test]
    fn update_interval_defaults_to_six_hours_and_rejects_under_a_quarter_hour() {
        let base = temp_base("interval");
        fs::create_dir_all(&base).unwrap();
        let values = SettingsValues::from_file(&base);
        assert_eq!(values.update_interval_minutes, "30");
        assert!(values.auto_update, "checking defaults to on");
        assert!(values.auto_install, "installing defaults to on");
        let mut quiet = values.clone();
        quiet.provider = "bing".into();
        quiet.auto_install = false;
        quiet.save(&base).unwrap();
        let text = fs::read_to_string(base.join("translator.conf")).unwrap();
        assert!(text.contains("auto_install=off"), "auto_install not saved: {text}");
        assert_eq!(SettingsValues::from_file(&base).update_interval_minutes, "30");
        let mut short = values.clone();
        short.provider = "bing".into();
        short.update_interval_minutes = "14".into();
        assert!(short.validate().is_err(), "14 minutes must be rejected");
        short.update_interval_minutes = "15".into();
        assert!(short.validate().is_ok(), "15 minutes must be accepted");
        assert_eq!(interval_label(15), "\u{6bcf} 15 \u{5206}\u{949f}");
        assert_eq!(interval_label(360), "\u{6bcf} 6 \u{5c0f}\u{65f6}");
        assert_eq!(interval_label(1440), "\u{6bcf} 1 \u{5929}");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn theme_defaults_to_system_and_the_shortcut_defaults_to_left_alt() {
        let base = temp_base("defaults");
        fs::create_dir_all(&base).unwrap();
        let file = ConfigFile::read(&base);
        assert_eq!(file.theme, "system");
        assert_eq!(file.alt_output_shortcut, DEFAULT_SHORTCUT);
        assert_eq!(normalize_shortcut(" nope "), "none");
        assert_eq!(normalize_shortcut("Ctrl + Space"), "ctrl+space");
        assert_eq!(normalize_shortcut("shift+alt+f5"), "shift+alt+f5");
        assert_eq!(shortcut_label("lalt"), "\u{5de6} Alt");
        assert_eq!(shortcut_label("ctrl+shift+a"), "Ctrl + Shift + A");
        assert_eq!(shortcut_label("none"), "\u{672a}\u{8bbe}\u{7f6e}");
        assert_eq!(parse_shortcut(""), None);
        assert_eq!(parse_shortcut("f24"), Some((0, 0x87)));
        assert_eq!(normalize_theme(" Dark "), "dark");
        assert_eq!(normalize_theme("LIGHT"), "light");
        assert_eq!(normalize_theme("auto"), "system");
        let _ = fs::remove_dir_all(&base);
    }
}
