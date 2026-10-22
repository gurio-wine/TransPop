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
            DT_LEFT, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, HBRUSH, HFONT, MONITORINFO,
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
            SetWindowLongPtrW, SetWindowPos, ShowWindow, SystemParametersInfoW,
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
const ID_SAVE: u16 = 4101;
const ID_CANCEL: u16 = 4102;
const ID_AZURE_REVEAL: u16 = 4103;
const ID_OPENAI_REVEAL: u16 = 4104;
const ID_AZURE_SECRET: u16 = 4105;
const ID_OPENAI_SECRET: u16 = 4106;
const ID_UPDATE: u16 = 4107;
const ID_AUTOSTART: u16 = 4108;
const ID_ALT_OUTPUT: u16 = 4109;
const ID_AUTO_UPDATE: u16 = 4110;
const ID_THEME: u16 = 4111;
const EM_SETPASSWORDCHAR: u32 = 0x00CC;
const WM_CTLCOLORSTATIC: u32 = 0x0138;
const WM_CTLCOLOREDIT: u32 = 0x0133;
const WM_CTLCOLORLISTBOX: u32 = 0x0134;
const WM_ERASEBKGND: u32 = 0x0014;
const WM_KEYDOWN: u32 = 0x0100;
const DM_GETDEFID_ANSWER: usize = 0x534B; // DC_HASDEFID
const IDOK: u16 = 1;
const IDCANCEL: u16 = 2;
const EM_SETMARGINS: u32 = 0x00D3;
const EC_LEFT_RIGHT_MARGINS: usize = 0x0003;
const TRANSPARENT: i32 = 1;
const SS_ICON: u32 = 0x0003;
const STM_SETICON: u32 = 0x0170;
const PS_SOLID: i32 = 0;

// One shared grid: 26 px side margin, 520 px of content, labels end at 136.
const CLIENT_W: i32 = 572;
const MARGIN_X: i32 = 26;
const CONTENT_W: i32 = 520;
const LABEL_W: i32 = 118;
const FIELD_X: i32 = 154;
const FIELD_W: i32 = 392;
const SECRET_W: i32 = 323;
const WINDOW_STYLE: u32 = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN;
const WINDOW_EX_STYLE: u32 = WS_EX_APPWINDOW;
const WM_APP_SETTINGS_SAVE: u32 = super::WM_APP + 30;
const WM_APP_SETTINGS_CANCEL: u32 = super::WM_APP + 31;
const WM_APP_SETTINGS_PROVIDER: u32 = super::WM_APP + 32;
const WM_APP_SETTINGS_UPDATE: u32 = super::WM_APP + 33;

pub const SAVE_MESSAGE: u32 = WM_APP_SETTINGS_SAVE;
pub const CANCEL_MESSAGE: u32 = WM_APP_SETTINGS_CANCEL;
pub const PROVIDER_MESSAGE: u32 = WM_APP_SETTINGS_PROVIDER;
pub const UPDATE_MESSAGE: u32 = WM_APP_SETTINGS_UPDATE;

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
            draw_checkbox(&*(lp as *const DRAWITEMSTRUCT), &*theme);
            1
        }
        // Answer the dialog manager so IsDialogMessageW can route Enter to Save.
        DM_GETDEFID => ((DM_GETDEFID_ANSWER as isize) << 16) | ID_SAVE as isize,
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
            // Owner-drawn checks do not flip themselves; mirror the click into
            // the button state, then repaint.
            if notify as u32 == BN_CLICKED
                && matches!(id, ID_AUTOSTART | ID_ALT_OUTPUT | ID_AUTO_UPDATE
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
                }
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
                return 0;
            }
            let outgoing = if id == ID_SAVE || id == IDOK { WM_APP_SETTINGS_SAVE }
                else if id == ID_CANCEL || id == IDCANCEL { WM_APP_SETTINGS_CANCEL }
                else if id == ID_PROVIDER && notify == CBN_SELCHANGE { WM_APP_SETTINGS_PROVIDER }
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
    pub alt_output: bool,
    pub auto_update: bool,
    pub theme: String,
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
            alt_output: flag("alt_output", true),
            auto_update: flag("auto_update", true),
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
# auto_update=on checks GitHub Releases at startup and every 6 hours and installs\n\
# a newer release after verifying the digest GitHub publishes for its asset.\n\
auto_update=on\n\
alt_output=on\n\
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
    pub alt_output: bool,
    pub auto_update: bool,
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
            alt_output: file.alt_output,
            auto_update: file.auto_update,
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
            ("alt_output", if values.alt_output { "true".to_string() } else { "false".to_string() }),
            ("auto_update", if values.auto_update { "on".to_string() } else { "off".to_string() }),
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
    subtitle: HWND,
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
    alt_output: HWND,
    auto_update: HWND,
    update_button: HWND,
    footer_divider: HWND,
    theme_label: HWND,
    theme_picker: HWND,
    save_button: HWND,
    cancel_button: HWND,
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

        // Header: mark, product name, one quiet subtitle line.
        let mark = create("STATIC", "", 0, SS_ICON);
        let brand_icon = super::load_brand_icon(false);
        if !mark.is_null() && !brand_icon.is_null() {
            SendMessageW(mark, STM_SETICON, brand_icon as usize, 0);
        }
        let heading = create("STATIC", "TransPop", 0, 0);
        set_font(heading, title_font);
        let subtitle = create("STATIC", &format!("偏好设置 · v{}", super::VERSION), 0, 0);
        set_font(subtitle, note_font);
        let service_divider = create("STATIC", "", 0, 0);

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
        let autostart = create("BUTTON", "登录 Windows 时自动启动", ID_AUTOSTART,
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
        let alt_output = create("BUTTON", "轻按左 Alt：取消拼音并输入英文", ID_ALT_OUTPUT,
            BS_OWNERDRAW | WS_TABSTOP);
        set_button_checked(alt_output, values.alt_output);
        let auto_update = create("BUTTON", "自动检查并安装 GitHub 更新", ID_AUTO_UPDATE,
            BS_OWNERDRAW | WS_TABSTOP);
        set_button_checked(auto_update, values.auto_update);
        let update_button = create("BUTTON", "检查更新", ID_UPDATE, WS_TABSTOP);
        let footer_divider = create("STATIC", "", 0, 0);
        // In-app theme switch: system, light, dark.
        let theme_label = create("STATIC", "主题", 0, 0);
        set_font(theme_label, note_font);
        let theme_picker = create("COMBOBOX", "", ID_THEME, CBS_DROPDOWN | WS_TABSTOP);
        for label in ["跟随系统", "浅色", "深色"] {
            let item = wide(label);
            SendMessageW(theme_picker, CB_ADDSTRING, 0, item.as_ptr() as isize);
        }
        let theme_pos = match values.theme.as_str() { "light" => 1, "dark" => 2, _ => 0 };
        SendMessageW(theme_picker, CB_SETCURSEL, theme_pos, 0);
        let picker_edit = GetWindow(theme_picker, GW_CHILD);
        if !picker_edit.is_null() { SendMessageW(picker_edit, EM_SETREADONLY, 1, 0); }
        let save_button = create("BUTTON", "保存设置", ID_SAVE, WS_TABSTOP | 0x00000001); // BS_DEFPUSHBUTTON
        let cancel_button = create("BUTTON", "取消", ID_CANCEL, WS_TABSTOP);

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
        (*theme_ptr).notes.push(subtitle as isize);
        for control in [service_divider, connection_divider, behavior_divider, footer_divider] {
            (*theme_ptr).lines.push(control as isize);
        }
        // Drop-downs whose closed field the theme has to paint itself.
        (*theme_ptr).combos.push(provider as isize);
        (*theme_ptr).combos.push(theme_picker as isize);
        apply_window_theme(hwnd, &mut *theme_ptr);

        let this = Self { hwnd, base, dpi, font, title_font, section_font, note_font, theme,
            brand_mark: mark, heading, subtitle, service_title, service_divider, provider,
            connection_title, connection_divider, azure, openai, azure_reveal, openai_reveal,
            behavior_title, behavior_divider, timeout_label, timeout, timeout_unit,
            debounce_label, debounce, debounce_unit, autostart, alt_output,
            auto_update, update_button, footer_divider, theme_label, theme_picker,
            save_button, cancel_button };
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
        place(self.subtitle, 75, 44, 470, 20);
        place(self.service_divider, MARGIN_X, 80, CONTENT_W, 1);

        let mut y = 98;
        place(self.service_title, MARGIN_X, y, 460, 22);
        y += 28;
        place(self.provider, MARGIN_X, y, CONTENT_W, 30);
        y += 30 + 22;

        // Separator before 服务连接, or before 翻译行为 when there is none.
        place(self.behavior_divider, MARGIN_X, y, CONTENT_W, 1);
        y += 18;

        let selected = SendMessageW(self.provider, CB_GETCURSEL, 0, 0);
        let advanced = selected == 1 || selected == 2;
        for (label, edit) in self.azure {
            show(label, selected == 1);
            show(edit, selected == 1);
        }
        for (label, edit) in self.openai {
            show(label, selected == 2);
            show(edit, selected == 2);
        }
        show(self.connection_title, advanced);
        show(self.connection_divider, advanced);
        show(self.azure_reveal, selected == 1);
        show(self.openai_reveal, selected == 2);

        if advanced {
            place(self.connection_title, MARGIN_X, y, 460, 22);
            y += 28;
            let rows = if selected == 1 { &self.azure } else { &self.openai };
            let reveal = if selected == 1 { self.azure_reveal } else { self.openai_reveal };
            for (row, (label, edit)) in rows.iter().enumerate() {
                place(*label, MARGIN_X, y + 4, LABEL_W, 22);
                let width = if row == 2 { SECRET_W } else { FIELD_W };
                place(*edit, FIELD_X, y, width, 28);
                if row == 2 { place(reveal, 485, y + 3, 61, 22); }
                if row < 2 { y += 36; }
            }
            y += 28 + 18;
            place(self.connection_divider, MARGIN_X, y, CONTENT_W, 1);
            y += 18;
        }

        place(self.behavior_title, MARGIN_X, y, 460, 22);
        y += 28;
        place(self.timeout_label, MARGIN_X, y + 4, LABEL_W, 22);
        place(self.timeout, FIELD_X, y, 90, 28);
        place(self.timeout_unit, FIELD_X + 98, y + 4, 140, 22);
        y += 36;
        place(self.debounce_label, MARGIN_X, y + 4, LABEL_W, 22);
        place(self.debounce, FIELD_X, y, 84, 28);
        place(self.debounce_unit, FIELD_X + 92, y + 4, 60, 22);
        y += 28 + 14;

        place(self.autostart, MARGIN_X, y, 300, 24);
        place(self.alt_output, MARGIN_X, y + 30, CONTENT_W, 24);
        place(self.auto_update, MARGIN_X, y + 60, 380, 24);
        place(self.update_button, 450, y + 57, 96, 30);
        y += 57 + 30;

        place(self.footer_divider, MARGIN_X, y + 14, CONTENT_W, 1);
        y += 14 + 1 + 14;
        place(self.theme_label, MARGIN_X, y + 6, 40, 20);
        place(self.theme_picker, 70, y + 2, 150, 30);
        place(self.cancel_button, 342, y, 84, 32);
        place(self.save_button, 434, y, 112, 32);
        y + 32 + 18
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

    /// The connection block appears or disappears with the provider, so the
    /// frame has to take its height from layout() again.
    pub unsafe fn update_provider(&self) {
        self.apply_layout(false);
    }

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
            alt_output: button_checked(self.alt_output),
            auto_update: button_checked(self.auto_update),
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
            "provider=OpenAI\nauto_update=off\nalt_output=yes\n             openai_url=http://127.0.0.1:8080/v1/chat/completions\n")
            .unwrap();
        let file = ConfigFile::read(&base);
        assert_eq!(file.provider, "openai");
        assert!(!file.auto_update);
        assert!(file.alt_output);
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
    fn theme_defaults_to_system_and_alt_output_is_on_by_default() {
        let base = temp_base("defaults");
        fs::create_dir_all(&base).unwrap();
        let file = ConfigFile::read(&base);
        assert_eq!(file.theme, "system");
        assert!(file.alt_output, "Alt text output should default to on");
        assert_eq!(normalize_theme(" Dark "), "dark");
        assert_eq!(normalize_theme("LIGHT"), "light");
        assert_eq!(normalize_theme("auto"), "system");
        let _ = fs::remove_dir_all(&base);
    }
}
