//! Tiny native Win32 settings window. Does not paint the translation bubble.
//! The window procedure only forwards form commands to the application's
//! existing message loop. No UIA or networking occurs in a Win32 callback.

use std::{cell::RefCell, collections::HashMap, fs, io::Write, mem, path::{Path, PathBuf}, ptr};
use windows_sys::Win32::{
    Foundation::{HWND, POINT, RECT},
    Graphics::{
        Dwm::{DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE},
        Gdi::{
            BeginPaint, CreateFontW, CreatePen, CreateRoundRectRgn, CreateSolidBrush, DeleteObject,
            DrawTextW, EndPaint, FillRect, GetDC, GetMonitorInfoW, GetStockObject, GetSysColor,
            GetTextMetricsW,
            InvalidateRect, MonitorFromPoint, MonitorFromWindow, Polyline, RedrawWindow, ReleaseDC,
            RoundRect, SelectObject, SetBkColor, SetBkMode, SetTextColor, SetWindowRgn,
            COLOR_GRAYTEXT, COLOR_HIGHLIGHT, COLOR_WINDOW, COLOR_WINDOWTEXT, DT_CALCRECT, DT_CENTER,
            DT_LEFT, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, HBRUSH, HDC, HFONT, MONITORINFO,
            MONITOR_DEFAULTTONEAREST, NULL_BRUSH, NULL_PEN, PAINTSTRUCT, RDW_ALLCHILDREN,
            RDW_ERASE, RDW_INVALIDATE, TEXTMETRICW,
        },
    },
    System::{
        LibraryLoader::GetModuleHandleW,
        Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD},
    },
    UI::{
        Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW},
        Controls::{SetWindowTheme, DRAWITEMSTRUCT, MEASUREITEMSTRUCT, ODS_SELECTED,
            ODT_MENU},
        Shell::{DefSubclassProc, SetWindowSubclass, ShellExecuteW},
        Input::KeyboardAndMouse::{
            GetKeyState, MapVirtualKeyW, SetFocus, MAPVK_VSC_TO_VK_EX, VK_CONTROL,
            VK_ESCAPE,
            VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT, VK_TAB,
        },
        WindowsAndMessaging::{
            AdjustWindowRectEx, AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW,
            DestroyMenu, DestroyWindow, EnumChildWindows, GetClassNameW, GetClientRect, GetCursorPos,
            GetDlgItem, GetParent, GetSystemMetrics, GetWindowLongPtrW, GetWindowRect, GetWindowTextLengthW,
            GetWindowTextW, IsWindow, IsWindowVisible, PostMessageW, RegisterClassW, SendMessageW,
            SetForegroundWindow, SetMenuInfo, SetWindowLongPtrW, SetWindowPos, SetWindowTextW,
            ShowWindow, SystemParametersInfoW, TrackPopupMenu, BN_CLICKED, DM_GETDEFID,
            GWLP_USERDATA, MENUINFO, MF_OWNERDRAW, MIM_BACKGROUND,
            SPI_GETHIGHCONTRAST, SW_HIDE, SW_SHOW, SW_SHOWNA,
            SW_SHOWNORMAL, SWP_NOACTIVATE,
            SWP_NOZORDER, TPM_LEFTALIGN, TPM_NONOTIFY, TPM_RETURNCMD, TPM_TOPALIGN, WM_CHAR,
            WM_CLOSE, WM_COMMAND, WM_DRAWITEM, WM_GETFONT, WM_KEYUP, WM_KILLFOCUS, WM_MEASUREITEM,
            WM_PAINT,
            WM_SYSCHAR, WM_SYSKEYDOWN, WM_SYSKEYUP, WNDCLASSW, WS_CAPTION, WS_CHILD,
            WS_CLIPCHILDREN, WS_EX_APPWINDOW, WS_OVERLAPPED, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
        },
    },
};

const CLASS: &str = "TransPopNativeSettings061";
const WM_SETFONT: u32 = 0x0030;
const ES_AUTOHSCROLL: u32 = 0x0080;
const BS_OWNERDRAW: u32 = 0x000B;
const ES_PASSWORD: u32 = 0x0020;
const ID_PROVIDER: u16 = 4100;
const ID_CANCEL: u16 = 4102;
const ID_AZURE_REVEAL: u16 = 4103;
const ID_OPENAI_REVEAL: u16 = 4104;
const ID_AZURE_SECRET: u16 = 4105;
const ID_OPENAI_SECRET: u16 = 4106;
const ID_UPDATE: u16 = 4107;
const ID_AUTOSTART: u16 = 4108;
const ID_SHORTCUT_KEY: u16 = 4109;
const ID_THEME: u16 = 4111;
const ID_UPDATE_INTERVAL: u16 = 4112;
const ID_AUTO_INSTALL: u16 = 4113;
const ID_OPEN_REPO: u16 = 4114;
const ID_SHORTCUT_ADD: u16 = 4115;
const ID_AUTO_RESTART: u16 = 4116;
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
const WM_GETDLGCODE: u32 = 0x0087;
const WM_LBUTTONDOWN: u32 = 0x0201;
const DLGC_WANTALLKEYS: isize = 0x0004;
const SUBCLASS_ID: usize = 1;
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
// One size and one corner radius for every interactive control, so fields,
// selectors and buttons line up instead of each page inventing its own.
const CONTROL_H: i32 = 30;
const RADIUS: i32 = 6;
// Shortcut row: fixed-width centred fields plus one + button, on its own row.
pub const MAX_SHORTCUTS: usize = 3;
const SHORTCUT_W: i32 = 152;
const SHORTCUT_GAP: i32 = 6;
const SHORTCUT_ADD_W: i32 = 30;
/// Width shared by the two numeric fields, so their unit labels align too.
const SHORT_FIELD_W: i32 = 90;
/// Corner radius of the frame itself, in logical pixels.
const WINDOW_RADIUS: i32 = 10;
/// What a clicked row shows until a key is pressed.
const SHORTCUT_HINT: &str = "请设置按键";
const WINDOW_STYLE: u32 = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN;
const WINDOW_EX_STYLE: u32 = WS_EX_APPWINDOW;
const WM_APP_SETTINGS_SAVE: u32 = super::WM_APP + 30;
const WM_APP_SETTINGS_CANCEL: u32 = super::WM_APP + 31;
const WM_APP_SETTINGS_PROVIDER: u32 = super::WM_APP + 32;
const WM_APP_SETTINGS_UPDATE: u32 = super::WM_APP + 33;
const WM_APP_SETTINGS_PAGE: u32 = super::WM_APP + 34;
const WM_APP_SETTINGS_SHORTCUT: u32 = super::WM_APP + 35;

pub const SAVE_MESSAGE: u32 = WM_APP_SETTINGS_SAVE;
pub const CANCEL_MESSAGE: u32 = WM_APP_SETTINGS_CANCEL;
pub const PROVIDER_MESSAGE: u32 = WM_APP_SETTINGS_PROVIDER;
pub const UPDATE_MESSAGE: u32 = WM_APP_SETTINGS_UPDATE;
pub const PAGE_MESSAGE: u32 = WM_APP_SETTINGS_PAGE;
pub const SHORTCUT_MESSAGE: u32 = WM_APP_SETTINGS_SHORTCUT;

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
                bg: 0x00FFFFFF, field: 0x00F4F4F4, text: 0x00202020,
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

/// Surface and text colours for the translation bubble, so the overlay
/// follows the same palette as the window. The bubble sits over arbitrary
/// desktop content, so dark mode lifts it to the field tone instead of the
/// window tone.
pub(crate) fn bubble_colors(base: &Path) -> (u32, u32) {
    let palette = unsafe { Palette::detect(base) };
    let colors = unsafe { palette.colors() };
    let bg = if palette == Palette::Dark { colors.field } else { colors.bg };
    (bg, colors.text)
}

/// A flat selector: the stock combo box paints a light field and a white
/// arrow that no theme call fixes on Windows 10, so the choice is drawn as a
/// plain row and the options come from a native popup menu instead.
struct Select {
    hwnd: isize,
    items: Vec<String>,
    index: usize,
}

/// Everything the window procedure needs to paint. Boxed by the window and
/// reached through GWLP_USERDATA, because a Win32 child outlives any borrow.
struct UiTheme {
    dark: bool,
    /// Frame colour currently applied; Windows only repaints the non-client
    /// area across a hide/show cycle, so a real change is tracked.
    frame_dark: bool,
    dpi: i32,
    /// The form font, shared by every painted control and the popup rows.
    font: HFONT,
    /// Height of one line of that font, in device pixels: a native field is
    /// shrunk to exactly this so its text sits in the middle of its plate.
    line_h: i32,
    colors: Colors,
    bg_brush: HBRUSH,
    field_brush: HBRUSH,
    line_brush: HBRUSH,
    accent_brush: HBRUSH,
    /// Secondary-text statics, painted with the muted colour.
    notes: Vec<isize>,
    /// Hairline statics, painted with the line colour.
    lines: Vec<isize>,
    /// Read-only edits, painted with the field fill like live ones.
    fields: Vec<isize>,
    /// Rounded plates painted under those fields, in client coordinates.
    plates: Vec<(isize, RECT)>,
    /// Labels of the popup a selector is showing right now, its choice, and
    /// the width Windows should give the popup.
    menu_items: Vec<String>,
    menu_current: usize,
    menu_width: i32,
    /// Owner-drawn shortcut rows, and the "+" that appends one.
    shortcut_rows: Vec<isize>,
    shortcut_plus: isize,
    /// Plain owner-drawn buttons, painted like a field with centred text.
    buttons: Vec<isize>,
    /// Flat selectors: one text row plus a chevron, menu on click.
    selects: Vec<Select>,
    /// Owner-drawn tab buttons, and which page is showing.
    tabs: Vec<isize>,
    page: usize,
}

impl UiTheme {
    /// Logical pixels to device pixels, the one conversion the form uses.
    fn scale(&self, value: i32) -> i32 {
        value * self.dpi / 96
    }

    unsafe fn create(palette: Palette, dpi: i32) -> Self {
        let mut theme = Self {
            dark: palette == Palette::Dark,
            frame_dark: palette == Palette::Dark,
            dpi,
            font: ptr::null_mut(),
            line_h: 0,
            colors: palette.colors(),
            bg_brush: ptr::null_mut(),
            field_brush: ptr::null_mut(),
            line_brush: ptr::null_mut(),
            accent_brush: ptr::null_mut(),
            notes: Vec::new(),
            lines: Vec::new(),
            fields: Vec::new(),
            plates: Vec::new(),
            menu_items: Vec::new(),
            menu_current: 0,
            menu_width: 0,
            shortcut_rows: Vec::new(),
            shortcut_plus: 0,
            buttons: Vec::new(),
            selects: Vec::new(),
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

    /// Position of this control in the selector list (an identity, not a choice).
    fn select_position(&self, control: HWND) -> Option<usize> {
        self.selects.iter().position(|select| select.hwnd == control as isize)
    }

    /// The option the user actually picked for this selector.
    fn chosen_index(&self, control: HWND) -> Option<usize> {
        self.selects.iter()
            .find(|select| select.hwnd == control as isize)
            .map(|select| select.index)
    }

    /// One flat selector's current choice and its option labels.
    fn selection(&self, control: HWND) -> Option<(usize, Vec<String>)> {
        self.selects.iter()
            .find(|select| select.hwnd == control as isize)
            .map(|select| (select.index, select.items.clone()))
    }

    fn set_selection(&mut self, control: HWND, index: usize) {
        if let Some(select) = self.selects.iter_mut()
            .find(|select| select.hwnd == control as isize) {
            select.index = index;
        }
        unsafe { InvalidateRect(control, ptr::null(), 0) };
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
        } else if self.fields.contains(&key) || self.select_position(control).is_some() {
            (self.colors.text, self.field_brush)
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

/// Fills a rectangle with rounded corners. GDI has no filled round-rect
/// primitive, so the stock null pen turns RoundRect into a pure fill.
unsafe fn fill_round(hdc: HDC, rect: &RECT, brush: HBRUSH, radius: i32) {
    if brush.is_null() { return; }
    let old_brush = SelectObject(hdc, brush);
    let old_pen = SelectObject(hdc, GetStockObject(NULL_PEN));
    let diameter = radius.max(0) * 2;
    RoundRect(hdc, rect.left, rect.top, rect.right, rect.bottom, diameter, diameter);
    if !old_pen.is_null() { SelectObject(hdc, old_pen); }
    if !old_brush.is_null() { SelectObject(hdc, old_brush); }
}

/// Clips a child window to a rounded region. The native EDIT paints its own
/// square corners, so the only way to round it is to cut its window shape.
unsafe fn round_window(hwnd: HWND, width: i32, height: i32, diameter: i32) {
    if hwnd.is_null() { return; }
    let region = CreateRoundRectRgn(0, 0, width + 1, height + 1, diameter, diameter);
    if region.is_null() { return; }
    // After success Windows owns the region; delete only on failure.
    if SetWindowRgn(hwnd, region, 1) == 0 { DeleteObject(region); }
}

/// Outlines a rounded rectangle with a hairline of the given colour.
unsafe fn ring_round(hdc: HDC, rect: &RECT, colour: u32, radius: i32, width: i32) {
    let pen = CreatePen(PS_SOLID, width.max(1), colour);
    if pen.is_null() { return; }
    let old_pen = SelectObject(hdc, pen);
    let old_brush = SelectObject(hdc, GetStockObject(NULL_BRUSH));
    let diameter = radius.max(0) * 2;
    RoundRect(hdc, rect.left, rect.top, rect.right, rect.bottom, diameter, diameter);
    if !old_brush.is_null() { SelectObject(hdc, old_brush); }
    if !old_pen.is_null() { SelectObject(hdc, old_pen); }
    DeleteObject(pen);
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
        fill_round(hdc, &glyph, theme.accent_brush, s(3));
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
        // A ring, not a box: no border anywhere else in the form either.
        ring_round(hdc, &glyph, theme.colors.secondary, s(3), s(1));
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
}

/// Owner-drawn shortcut row: field fill, the label centred both ways, and a
/// focus ring. An EDIT cannot centre its text vertically and always shows a
/// caret, so the row is a BS_OWNERDRAW button painted here instead. The "+"
/// shares this painter and gets a glyph centred in its own square.
unsafe fn draw_shortcut(item: &DRAWITEMSTRUCT, theme: &UiTheme, plus: bool) {
    let s = |value: i32| value * theme.dpi / 96;
    let hdc = item.hDC;
    FillRect(hdc, &item.rcItem, theme.bg_brush);
    fill_round(hdc, &item.rcItem, theme.field_brush, s(RADIUS));
    if plus {
        let cx = (item.rcItem.left + item.rcItem.right) / 2;
        let cy = (item.rcItem.top + item.rcItem.bottom) / 2;
        let arm = s(6);
        let pen = CreatePen(PS_SOLID, s(1).max(1), theme.colors.text);
        if !pen.is_null() {
            let old = SelectObject(hdc, pen);
            let horizontal = [POINT { x: cx - arm, y: cy }, POINT { x: cx + arm, y: cy }];
            let vertical = [POINT { x: cx, y: cy - arm }, POINT { x: cx, y: cy + arm }];
            Polyline(hdc, horizontal.as_ptr(), 2);
            Polyline(hdc, vertical.as_ptr(), 2);
            SelectObject(hdc, old);
            DeleteObject(pen);
        }
    } else {
        let mut text = [0u16; 256];
        let len = GetWindowTextW(item.hwndItem, text.as_mut_ptr(), text.len() as i32).max(0) as usize;
        if len > 0 {
            let font = SendMessageW(item.hwndItem, WM_GETFONT, 0, 0) as HFONT;
            let old_font = if font.is_null() { ptr::null_mut() } else { SelectObject(hdc, font) };
            SetBkMode(hdc, TRANSPARENT);
            let hint = String::from_utf16_lossy(&text[..len]) == SHORTCUT_HINT;
            SetTextColor(hdc, if hint { theme.colors.secondary } else { theme.colors.text });
            let mut area = item.rcItem;
            DrawTextW(hdc, text.as_ptr(), len as i32, &mut area,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
            if !old_font.is_null() { SelectObject(hdc, old_font); }
        }
    }
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
        // A filled card, like the controls: colour alone marks the page.
        fill_round(hdc, &item.rcItem, theme.field_brush, s(RADIUS));
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
}

/// Flat selector: the current choice on a plain fill with a chevron on the
/// right, so it reads as a value rather than as a text field.
unsafe fn draw_select(item: &DRAWITEMSTRUCT, theme: &UiTheme) {
    let s = |value: i32| value * theme.dpi / 96;
    let hdc = item.hDC;
    FillRect(hdc, &item.rcItem, theme.bg_brush);
    fill_round(hdc, &item.rcItem, theme.field_brush, s(RADIUS));
    let mut text = [0u16; 256];
    let len = GetWindowTextW(item.hwndItem, text.as_mut_ptr(), text.len() as i32).max(0) as usize;
    let font = SendMessageW(item.hwndItem, WM_GETFONT, 0, 0) as HFONT;
    let old_font = if font.is_null() { ptr::null_mut() } else { SelectObject(hdc, font) };
    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, theme.colors.text);
    let arrow = s(14);
    let mut area = RECT {
        left: item.rcItem.left + s(10),
        top: item.rcItem.top,
        right: item.rcItem.right - arrow - s(10),
        bottom: item.rcItem.bottom,
    };
    DrawTextW(hdc, text.as_ptr(), len as i32, &mut area,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
    // Chevron: two strokes meeting at a point, drawn with a hairline pen.
    let cx = item.rcItem.right - s(14);
    let cy = (item.rcItem.top + item.rcItem.bottom) / 2;
    let pen = CreatePen(PS_SOLID, s(1).max(1), theme.colors.secondary);
    if !pen.is_null() {
        let old_pen = SelectObject(hdc, pen);
        let points = [
            POINT { x: cx - s(4), y: cy - s(2) },
            POINT { x: cx, y: cy + s(2) },
            POINT { x: cx + s(4), y: cy - s(2) },
        ];
        Polyline(hdc, points.as_ptr(), points.len() as i32);
        SelectObject(hdc, old_pen);
        DeleteObject(pen);
    }
    if !old_font.is_null() { SelectObject(hdc, old_font); }
}

/// Owner-drawn plain button: a rounded field-coloured slab with the label
/// centred both ways, so it matches the selectors instead of wearing the
/// stock 3D frame. A pressed button darkens to the line colour.
unsafe fn draw_button(item: &DRAWITEMSTRUCT, theme: &UiTheme) {
    let s = |value: i32| value * theme.dpi / 96;
    let hdc = item.hDC;
    FillRect(hdc, &item.rcItem, theme.bg_brush);
    let brush = if item.itemState & ODS_SELECTED != 0 {
        theme.line_brush
    } else {
        theme.field_brush
    };
    fill_round(hdc, &item.rcItem, brush, s(RADIUS));
    let mut text = [0u16; 128];
    let len = GetWindowTextW(item.hwndItem, text.as_mut_ptr(), text.len() as i32).max(0) as usize;
    let font = SendMessageW(item.hwndItem, WM_GETFONT, 0, 0) as HFONT;
    let old_font = if font.is_null() { ptr::null_mut() } else { SelectObject(hdc, font) };
    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, theme.colors.text);
    let mut area = item.rcItem;
    DrawTextW(hdc, text.as_ptr(), len as i32, &mut area,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
    if !old_font.is_null() { SelectObject(hdc, old_font); }
}

/// Owner-drawn popup row: the stock menu paints a grey check gutter and a hard
/// square highlight that do not match the flat form, so every row is drawn
/// here. The current choice is marked by the accent colour, the hot row by a
/// rounded field fill.
unsafe fn draw_menu(item: &DRAWITEMSTRUCT, theme: &UiTheme) {
    let s = |value: i32| value * theme.dpi / 96;
    let hdc = item.hDC;
    FillRect(hdc, &item.rcItem, theme.bg_brush);
    let index = item.itemData;
    if item.itemState & ODS_SELECTED != 0 {
        let mut row = item.rcItem;
        row.top += s(1);
        row.bottom -= s(1);
        fill_round(hdc, &row, theme.field_brush, s(4));
    }
    let Some(label) = theme.menu_items.get(index) else { return; };
    let text = wide(label);
    let font = theme.font;
    let old_font = if font.is_null() { ptr::null_mut() } else { SelectObject(hdc, font) };
    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, if index == theme.menu_current { theme.colors.accent } else { theme.colors.text });
    let mut area = RECT {
        left: item.rcItem.left + s(12),
        top: item.rcItem.top,
        right: item.rcItem.right - s(12),
        bottom: item.rcItem.bottom,
    };
    DrawTextW(hdc, text.as_ptr(), text.len() as i32 - 1, &mut area,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
    if !old_font.is_null() { SelectObject(hdc, old_font); }
}

/// Pops the themed menu under a flat selector and returns the picked option.
/// A dismissed menu yields None, so the stored choice stays as it was.
unsafe fn pick_option(hwnd: HWND, control: HWND, theme: &mut UiTheme) -> Option<(usize, String)> {
    let (current, items) = theme.selection(control)?;
    let menu = CreatePopupMenu();
    if menu.is_null() { return None; }
    // The rows are painted from these labels, so they must be reachable from
    // the window procedure while the menu is up.
    theme.menu_items = items.clone();
    theme.menu_current = current;
    theme.menu_width = menu_width(control, theme, &items);
    let mut info: MENUINFO = mem::zeroed();
    info.cbSize = mem::size_of::<MENUINFO>() as u32;
    info.fMask = MIM_BACKGROUND;
    info.hbrBack = theme.bg_brush;
    SetMenuInfo(menu, &info);
    // The command id is the option number and itemData is the option index:
    // the row painter needs the index, the menu only reports the id.
    for index in 0..items.len() {
        AppendMenuW(menu, MF_OWNERDRAW, index + 1, index as usize as *const u16);
    }
    let mut rect: RECT = mem::zeroed();
    GetWindowRect(control, &mut rect);
    let chosen = TrackPopupMenu(menu,
        TPM_LEFTALIGN | TPM_TOPALIGN | TPM_RETURNCMD | TPM_NONOTIFY,
        rect.left, rect.bottom, 0, hwnd, ptr::null());
    DestroyMenu(menu);
    theme.menu_items.clear();
    theme.menu_width = 0;
    if chosen == 0 { return None; }
    items.get(chosen as usize - 1).map(|label| (chosen as usize - 1, label.clone()))
}

/// Width of the popup: the widest label plus padding, never narrower than the
/// control it drops from, so the menu lines up with the field above it.
unsafe fn menu_width(control: HWND, theme: &UiTheme, items: &[String]) -> i32 {
    let s = |value: i32| value * theme.dpi / 96;
    let mut widest = 0;
    let hdc = GetDC(control);
    if !hdc.is_null() {
        let old_font = if theme.font.is_null() { ptr::null_mut() } else { SelectObject(hdc, theme.font) };
        for item in items {
            let text = wide(item);
            let mut area = RECT { left: 0, top: 0, right: 0, bottom: 0 };
            DrawTextW(hdc, text.as_ptr(), text.len() as i32 - 1, &mut area,
                DT_CALCRECT | DT_SINGLELINE | DT_NOPREFIX);
            widest = widest.max(area.right);
        }
        if !old_font.is_null() { SelectObject(hdc, old_font); }
        ReleaseDC(control, hdc);
    }
    let mut rect: RECT = mem::zeroed();
    GetWindowRect(control, &mut rect);
    (widest + s(24)).max(rect.right - rect.left)
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
        // Checkboxes and selectors are owner-drawn and need no scheme.
        let scheme = if !dark { "" }
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
    let theme = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut UiTheme;
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
            let (text, brush) = (*theme).colors_for(lp as HWND);
            SetBkMode(wp as _, TRANSPARENT);
            SetBkColor(wp as _, (*theme).colors.field);
            SetTextColor(wp as _, text);
            brush as isize
        }
        WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => {
            // Borderless fields: the fill is the only affordance, so both
            // palettes paint it instead of leaving the stock white.
            if theme.is_null() { return DefWindowProcW(hwnd, msg, wp, lp); }
            SetBkColor(wp as _, (*theme).colors.field);
            SetTextColor(wp as _, (*theme).colors.text);
            (*theme).field_brush as isize
        }
        // Rows of a selector popup: painted here so the menu matches the form.
        WM_MEASUREITEM => {
            if theme.is_null() { return DefWindowProcW(hwnd, msg, wp, lp); }
            let item = &mut *(lp as *mut MEASUREITEMSTRUCT);
            if item.CtlType != ODT_MENU { return DefWindowProcW(hwnd, msg, wp, lp); }
            let s = |value: i32| value * (*theme).dpi / 96;
            item.itemWidth = (*theme).menu_width.max(0) as u32;
            item.itemHeight = s(CONTROL_H) as u32;
            1
        }
        WM_DRAWITEM => {
            if theme.is_null() { return DefWindowProcW(hwnd, msg, wp, lp); }
            let item = &*(lp as *const DRAWITEMSTRUCT);
            if item.CtlType == ODT_MENU {
                draw_menu(item, &*theme);
            } else if let Some(index) = (*theme).tab_index(item.hwndItem) {
                draw_tab(item, &*theme, index);
            } else if (*theme).shortcut_rows.contains(&(item.hwndItem as isize)) {
                draw_shortcut(item, &*theme, false);
            } else if item.hwndItem as isize == (*theme).shortcut_plus {
                draw_shortcut(item, &*theme, true);
            } else if (*theme).select_position(item.hwndItem).is_some() {
                draw_select(item, &*theme);
            } else if (*theme).buttons.contains(&(item.hwndItem as isize)) {
                draw_button(item, &*theme);
            } else {
                draw_checkbox(item, &*theme);
            }
            1
        }
        WM_PAINT => {
            // A single-line edit centres its text only inside its own short
            // format rectangle, so the rounded field is painted by the form and
            // the edit sits on top of it, centred by geometry.
            if !theme.is_null() && !(*theme).plates.is_empty() {
                let mut paint: PAINTSTRUCT = mem::zeroed();
                let hdc = BeginPaint(hwnd, &mut paint);
                for (field, plate) in &(*theme).plates {
                    if IsWindowVisible(*field as HWND) == 0 { continue; }
                    FillRect(hdc, plate, (*theme).bg_brush);
                    fill_round(hdc, plate, (*theme).field_brush, (*theme).scale(RADIUS));
                }
                EndPaint(hwnd, &paint);
                0
            } else {
                DefWindowProcW(hwnd, msg, wp, lp)
            }
        }
        // Clicking the background takes focus off a shortcut row, which is how a
        // row the user no longer wants is cleared (and then dropped).
        WM_LBUTTONDOWN => {
            // The edit is one line tall inside a taller plate, so a click on
            // the plate's own padding must still land in the edit.
            let point = POINT { x: (lp as i16) as i32, y: ((lp >> 16) as i16) as i32 };
            let mut target = hwnd;
            if !theme.is_null() {
                for (field, plate) in &(*theme).plates {
                    if point.x >= plate.left && point.x < plate.right
                        && point.y >= plate.top && point.y < plate.bottom
                        && IsWindowVisible(*field as HWND) != 0 {
                        target = *field as HWND;
                        break;
                    }
                }
            }
            SetFocus(target);
            0
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
                && matches!(id, ID_AUTOSTART | ID_AUTO_INSTALL | ID_AUTO_RESTART
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
            // Flat selectors open their menu on click; the picked row becomes
            // the control text, so the field always shows the live value.
            if notify as u32 == BN_CLICKED
                && matches!(id, ID_PROVIDER | ID_THEME | ID_UPDATE_INTERVAL) {
                if theme.is_null() { return 0; }
                let picked = pick_option(hwnd, control, &mut *theme);
                let Some((index, label)) = picked else { return 0 };
                let theme = theme as *mut UiTheme;
                (*theme).set_selection(control, index);
                set_text(control, &label);
                let main = super::CONTROL_HWND.load(std::sync::atomic::Ordering::Acquire) as HWND;
                if main.is_null() { return 0; }
                if id == ID_PROVIDER {
                    PostMessageW(main, WM_APP_SETTINGS_PROVIDER, 0, 0);
                } else if id == ID_THEME {
                    let palette = match index {
                        1 => Palette::Light,
                        2 => Palette::Dark,
                        _ => Palette::detect_system(),
                    };
                    (*theme).recolour(palette);
                    apply_window_theme(hwnd, &mut *theme);
                }
                PostMessageW(main, WM_APP_SETTINGS_SAVE, 0, 0);
                return 0;
            }
            // "+" reveals one more row and arms it. It is shown and focused by
            // the layout pass, because a hidden field cannot take focus.
            if notify as u32 == BN_CLICKED && id == ID_SHORTCUT_ADD {
                CAPTURE.with(|c| {
                    let mut c = c.borrow_mut();
                    c.armed = true;
                    c.extra = true;
                    c.pending = None;
                });
                let main = super::CONTROL_HWND.load(std::sync::atomic::Ordering::Acquire) as HWND;
                if !main.is_null() { PostMessageW(main, WM_APP_SETTINGS_SHORTCUT, 0, 0); }
                return 0;
            }
            // The project block only links out; nothing here is persisted.
            if notify as u32 == BN_CLICKED && id == ID_OPEN_REPO {
                let url = wide("https://github.com/gurio-wine/TransPop");
                ShellExecuteW(hwnd, ptr::null(), url.as_ptr(), ptr::null(), ptr::null(),
                    SW_SHOWNORMAL);
                return 0;
            }
            // Save-on-change: every committed edit reports the same message the
            // Save button used to send, so the main loop keeps one apply path.
            let committed = notify == EN_KILLFOCUS;
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
    pub auto_install: bool,
    pub auto_restart: bool,
    pub update_interval_minutes: String,
    pub theme: String,
}

/// Update check intervals offered in the settings window, in minutes.
pub const UPDATE_INTERVALS: [u32; 8] = [15, 30, 60, 180, 360, 720, 1440, 10080];

/// Sentinel pinned to the bottom of the interval list: check only when asked.
pub const MANUAL_UPDATE_INTERVAL: u32 = 0;

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

/// Modifiers a chord may carry. The shortcut field reads the real keyboard, so
/// this is the whole key surface: whatever the user presses is what gets saved.
fn modifier_mask(vk: u16) -> Option<u8> {
    match vk {
        0x10 | 0xA0 | 0xA1 => Some(MOD_SHIFT),
        0x11 | 0xA2 | 0xA3 => Some(MOD_CTRL),
        0x12 | 0xA4 | 0xA5 => Some(MOD_ALT),
        0x5B | 0x5C => Some(MOD_WIN),
        _ => None,
    }
}

/// Side-specific virtual key for a modifier that was just pressed, falling back
/// to the generic key when the scan code cannot be resolved.
unsafe fn side_of(vk: u16, scan: u32) -> u16 {
    if modifier_mask(vk).is_none() { return vk; }
    let resolved = MapVirtualKeyW(scan, MAPVK_VSC_TO_VK_EX);
    if resolved == 0 { vk } else { resolved as u16 }
}

/// What the shortcut field is doing right now. The field is a plain read-only
/// edit; pressing a key inside it is the only way to change the shortcut, and
/// the captured spec is kept here instead of being parsed back out of the text.
#[derive(Default)]
struct Capture {
    /// A click armed the field; leaving it without a completed key means off.
    armed: bool,
    /// "+" added a row that is still waiting for its key, so it must stay
    /// visible until the capture ends. Rows are otherwise exactly the set ones.
    extra: bool,
    /// Modifier held down while the field has focus, waiting for a main key.
    pending: Option<(u8, u32)>,
}

thread_local! {
    static CAPTURE: RefCell<Capture> = RefCell::new(Capture::default());
    /// The shortcut fields in display order, each with the spec it holds. An
    /// empty spec means the user cleared that field; the window prunes it.
    static SHORTCUTS: RefCell<Vec<(HWND, String)>> = RefCell::new(Vec::new());
}

fn set_row_spec(hwnd: HWND, spec: String) {
    SHORTCUTS.with(|rows| {
        if let Some((_, held)) = rows.borrow_mut().iter_mut().find(|(row, _)| *row == hwnd) {
            *held = spec;
        }
    });
}

fn reset_rows(rows: Vec<(HWND, String)>) {
    SHORTCUTS.with(|held| *held.borrow_mut() = rows);
}

/// The configured shortcuts, in display order, without the cleared ones.
fn row_specs() -> Vec<String> {
    SHORTCUTS.with(|rows| rows.borrow().iter()
        .map(|(_, spec)| spec.clone())
        .filter(|spec| !spec.is_empty() && spec != "none")
        .collect())
}

/// Live keyboard capture for one shortcut row. Whatever the user presses is
/// stored as-is; leaving a row without pressing a key clears it, which is how
/// "not set" is expressed and how an extra row is dropped again.
unsafe extern "system" fn shortcut_proc(hwnd: HWND, msg: u32, wp: usize, lp: isize,
    _id: usize, _data: usize) -> isize {
    // A row only edits while it was clicked on purpose, so a capture parks focus
    // back on the dialog: the next keystroke must not silently re-bind it.
    let dialog = GetParent(hwnd);
    match msg {
        WM_GETDLGCODE => return DLGC_WANTALLKEYS,
        WM_LBUTTONDOWN => {
            // A click arms the field and clears what it held: the key that
            // follows is the shortcut, so the old one must not linger.
            SetFocus(hwnd);
            CAPTURE.with(|c| { let mut c = c.borrow_mut(); c.armed = true; c.pending = None; });
            set_text(hwnd, SHORTCUT_HINT);
            return 0;
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            let vk = wp as u16;
            // Escape keeps meaning "close", so the dialog is never trapped.
            if vk == VK_ESCAPE {
                let main = super::CONTROL_HWND.load(std::sync::atomic::Ordering::Acquire) as HWND;
                if !main.is_null() { PostMessageW(main, WM_APP_SETTINGS_CANCEL, 0, 0); }
                return 0;
            }
            // Only a real press counts: the window messages that simulate a
            // keystroke have no previous-key state, and those must not edit.
            if GetKeyState(vk as i32) >= 0 { return 0; }
            // Tab still moves focus unless the field was clicked first, so
            // keyboard users are never trapped in it.
            if vk == VK_TAB && !CAPTURE.with(|c| c.borrow().armed) {
                return DefSubclassProc(hwnd, msg, wp, lp);
            }
            let scan = ((lp >> 16) & 0xff) as u32;
            let key = side_of(vk, scan) as u32;
            // The key being pressed must not count as its own modifier: a
            // shift press arrives as the generic VK_SHIFT while the down-state
            // query uses the side-specific keys, so both would be combined.
            let own = modifier_mask(vk).unwrap_or(0);
            let mods = current_modifiers(vk) & !own;
            if modifier_mask(vk).is_some() {
                // A modifier alone is only half a chord; hold it for the key.
                CAPTURE.with(|c| c.borrow_mut().pending = Some((mods, key)));
                let label = shortcut_label(&shortcut_spec(mods, key));
                set_text(hwnd, &label);
                return 0;
            }
            CAPTURE.with(|c| { let mut c = c.borrow_mut(); c.pending = None; c.armed = false; });
            commit_row(hwnd, dialog, shortcut_spec(mods, key));
            return 0;
        }
        // Releasing the only modifier commits it: a bare Shift is a shortcut
        // too, and waiting for a main key would silently drop the choice.
        WM_KEYUP | WM_SYSKEYUP => {
            if modifier_mask(wp as u16).is_some() {
                let pending = CAPTURE.with(|c| { let mut c = c.borrow_mut(); c.armed = false; c.pending.take() });
                if let Some((mods, key)) = pending {
                    commit_row(hwnd, dialog, shortcut_spec(mods, key));
                }
            }
            return 0;
        }
        // Swallow the characters a chord would otherwise type or forward; the
        // field never takes text input.
        WM_CHAR | WM_SYSCHAR => return 0,
        WM_KILLFOCUS => {
            // Clicked in, pressed nothing (or only a modifier), then left: the
            // user did not choose a key, which is how "off" is expressed.
            let armed = CAPTURE.with(|c| { let mut c = c.borrow_mut(); c.pending = None; c.armed });
            if armed {
                CAPTURE.with(|c| c.borrow_mut().armed = false);
                // Leaving without a key clears the row; the form then prunes it.
                commit_row(hwnd, dialog, String::new());
            }
        }
        _ => {}
    }
    DefSubclassProc(hwnd, msg, wp, lp)
}

/// Store one captured row, park focus on the dialog so the row stops editing,
/// and let the main loop persist the list and re-lay-out the form.
unsafe fn commit_row(hwnd: HWND, dialog: HWND, spec: String) {
    set_row_spec(hwnd, spec);
    CAPTURE.with(|c| c.borrow_mut().extra = false);
    compact_rows();
    // Park focus on the dialog: editing one shortcut must not leave the field
    // armed, or the next keystroke would silently re-bind it.
    if !dialog.is_null() { SetFocus(dialog); }
    let main = super::CONTROL_HWND.load(std::sync::atomic::Ordering::Acquire) as HWND;
    if !main.is_null() {
        PostMessageW(main, WM_APP_SETTINGS_SAVE, 0, 0);
        PostMessageW(main, WM_APP_SETTINGS_SHORTCUT, 0, 0);
    }
}

/// The row list keeps one invariant: every set shortcut sits on the first rows
/// in order and the rest are empty. Clearing a row therefore removes it, and
/// the fields never reshuffle behind the user's back.
unsafe fn compact_rows() {
    let mut specs: Vec<String> = SHORTCUTS.with(|rows| rows.borrow().iter()
        .map(|(_, spec)| spec.clone())
        .filter(|spec| !spec.is_empty() && spec != "none")
        .collect());
    specs.truncate(MAX_SHORTCUTS);
    SHORTCUTS.with(|rows| {
        for (index, (row, held)) in rows.borrow_mut().iter_mut().enumerate() {
            *held = specs.get(index).cloned().unwrap_or_default();
            set_text(*row, &row_text(held));
        }
    });
}

/// Modifier bits held down right now, ignoring the key being pressed: pressing
/// Alt alone must not turn into `alt+alt`.
unsafe fn current_modifiers(vk: u16) -> u8 {
    let down = |code: u16| code != vk && GetKeyState(code as i32) < 0;
    let mut mods = 0u8;
    if down(VK_SHIFT) || down(0xA0) || down(0xA1) { mods |= MOD_SHIFT; }
    if down(VK_CONTROL) || down(0xA2) || down(0xA3) { mods |= MOD_CTRL; }
    if down(VK_MENU) || down(0xA4) || down(0xA5) { mods |= MOD_ALT; }
    if down(VK_LWIN) || down(VK_RWIN) { mods |= MOD_WIN; }
    mods
}

unsafe fn set_text(control: HWND, text: &str) {
    if control.is_null() { return; }
    let text = wide(text);
    SetWindowTextW(control, text.as_ptr());
    // Owner-drawn rows paint their own text, so a text change is a repaint.
    InvalidateRect(control, ptr::null(), 0);
}

/// Canonical spelling of a shortcut. Anything unreadable becomes `none`, which
/// is the safe state: no shortcut at all rather than a surprise keystroke.
pub fn normalize_shortcut(spec: &str) -> String {
    match parse_shortcut(spec) {
        Some((mods, vk)) => shortcut_spec(mods, vk),
        None => "none".to_string(),
    }
}

/// Every shortcut in a stored value, canonical and de-duplicated, in the order
/// the user set them. A file written by an older build holds a single spec and
/// still parses as a one-entry list.
pub fn shortcut_list(value: &str) -> Vec<String> {
    let mut list: Vec<String> = Vec::new();
    for part in value.split(',') {
        let spec = normalize_shortcut(part);
        if spec != "none" && !list.contains(&spec) { list.push(spec); }
    }
    list.truncate(MAX_SHORTCUTS);
    list
}

/// Config spelling of a list: comma-separated specs, `none` when empty.
pub fn shortcuts_value(list: &[String]) -> String {
    let joined = list.join(",");
    let list = shortcut_list(&joined);
    if list.is_empty() { "none".to_string() } else { list.join(",") }
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

/// What a row shows: the shortcut, or the prompt while it waits for a key.
pub fn row_text(spec: &str) -> String {
    match parse_shortcut(spec) {
        Some(_) => shortcut_label(spec),
        None => SHORTCUT_HINT.to_string(),
    }
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
    if minutes == MANUAL_UPDATE_INTERVAL { "\u{4ec5}\u{624b}\u{52a8}".into() }
    else if minutes >= 1440 { format!("\u{6bcf} {} \u{5929}", minutes / 1440) }
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
            alt_output_shortcut: shortcuts_value(&shortcut_list(&get("alt_output_shortcut",
                if map.get("alt_output").is_some_and(|v| !truthy(v, true)) { "none" }
                else { DEFAULT_SHORTCUT }))),
            auto_install: flag("auto_install", true),
            auto_restart: flag("auto_restart", true),
            // Older files had an auto_update on/off switch instead of the
            // 仅手动 interval entry; keep a disabled switch disabled.
            update_interval_minutes: get("update_interval_minutes",
                &if map.get("auto_update").is_some_and(|v| !truthy(v, true)) {
                    MANUAL_UPDATE_INTERVAL.to_string()
                } else {
                    DEFAULT_UPDATE_INTERVAL.to_string()
                }),
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
# update_interval_minutes is the gap between background checks of GitHub\n\
# Releases (min 15, default 30); write 0 to check only when asked. auto_install=on\n\
# then downloads a newer release after verifying the digest GitHub publishes for\n\
# its asset, while auto_install=off only announces the release and waits for a\n\
# click. auto_restart=off installs without relaunching; the new build runs on\n\
# the next manual start.\n\
auto_install=on\n\
auto_restart=on\n\
update_interval_minutes=30\n\
# alt_output_shortcut holds one or more shortcuts separated by commas; each is\n\
# pressed once (no chord) to paste the translation instead of the Chinese\n\
# candidate. Write none to disable it.\n\
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
    pub auto_install: bool,
    pub auto_restart: bool,
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
            auto_install: file.auto_install,
            auto_restart: file.auto_restart,
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
        if !self.update_interval_minutes.parse::<u32>()
            .is_ok_and(|n| n >= 15 || n == MANUAL_UPDATE_INTERVAL) {
            return Err("检查间隔请设置为 15 分钟以上，或选择仅手动".into());
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
            ("auto_install", if values.auto_install { "on".to_string() } else { "off".to_string() }),
            ("auto_restart", if values.auto_restart { "on".to_string() } else { "off".to_string() }),
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
                // The retired auto_update switch lives on as the 仅手动 interval;
                // drop the dead key instead of carrying it forever.
                if key == "auto_update" { continue; }
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
    shortcut_keys: [HWND; MAX_SHORTCUTS],
    shortcut_add: HWND,
    auto_install: HWND,
    auto_restart: HWND,
    update_interval: HWND,
    update_intervals: Vec<u32>,
    update_button: HWND,
    startup_title: HWND,
    project_title: HWND,
    update_title: HWND,
    version_label: HWND,
    interval_caption: HWND,
    theme_label: HWND,
    theme_picker: HWND,
    tabs: [HWND; 5],
    about_text: HWND,
    open_repo: HWND,
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
        let title = wide("设置");
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
        // The form needs the font metrics: an edit cannot centre its own text
        // vertically, so it is sized to one line and centred by the layout.
        (*theme_ptr).font = font;
        let dc = GetDC(hwnd);
        if !dc.is_null() {
            let old = SelectObject(dc, font);
            let mut metrics: TEXTMETRICW = mem::zeroed();
            if GetTextMetricsW(dc, &mut metrics) != 0 { (*theme_ptr).line_h = metrics.tmHeight; }
            SelectObject(dc, old);
            ReleaseDC(hwnd, dc);
        }
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
        // Flat selectors instead of combo boxes: the stock closed field reads as
        // an editable text box, and Windows 10 has no dark combo theme at all.
        let mut selects: Vec<Select> = Vec::new();
        let provider_items: Vec<String> =
            ["Bing 翻译（无需密钥）", "Azure Translator", "OpenAI 兼容接口", "关闭翻译"]
                .iter().map(|label| label.to_string()).collect();
        let provider_index = match values.provider.as_str() { "bing" => 0, "azure" => 1, "openai" => 2, _ => 3 };
        let provider = create("BUTTON", &provider_items[provider_index], ID_PROVIDER,
            BS_OWNERDRAW | WS_TABSTOP);
        selects.push(Select { hwnd: provider as isize, items: provider_items, index: provider_index });

        // Service connection; only visible for providers that need credentials.
        let connection_divider = create("STATIC", "", 0, 0);
        let connection_title = create("STATIC", "服务连接", 0, 0);
        set_font(connection_title, section_font);
        let field = |label: &str, value: &str, secret_id: u16| -> (HWND, HWND) {
            let label_hwnd = create("STATIC", label, 0, 0);
            let password = if secret_id != 0 { ES_PASSWORD } else { 0 };
            let edit_hwnd = create("EDIT", value, secret_id,
                WS_TABSTOP | ES_AUTOHSCROLL | password);
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
            WS_TABSTOP | ES_AUTOHSCROLL);
        let timeout_unit = create("STATIC", "秒", 0, 0);
        let debounce_label = create("STATIC", "翻译防抖", 0, 0);
        let debounce = create("EDIT", &values.debounce_ms, 0,
            WS_TABSTOP | ES_AUTOHSCROLL);
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

        // Downloading and relaunching are two separate switches: a release can
        // be staged in the background without interrupting whatever is running.
        let auto_install = create("BUTTON", "自动下载", ID_AUTO_INSTALL,
            BS_OWNERDRAW | WS_TABSTOP);
        set_button_checked(auto_install, values.auto_install);
        let auto_restart = create("BUTTON", "自动重启", ID_AUTO_RESTART,
            BS_OWNERDRAW | WS_TABSTOP);
        set_button_checked(auto_restart, values.auto_restart);
        // Interval between background checks; the current value may be one
        // the presets no longer offer, so it is kept as an extra entry.
        let current: u32 = values.update_interval_minutes.parse().unwrap_or(DEFAULT_UPDATE_INTERVAL);
        let mut intervals = UPDATE_INTERVALS.to_vec();
        if !intervals.contains(&current) && current != MANUAL_UPDATE_INTERVAL {
            intervals.push(current);
            intervals.sort_unstable();
        }
        intervals.push(MANUAL_UPDATE_INTERVAL);
        let interval_items: Vec<String> =
            intervals.iter().map(|minutes| interval_label(*minutes)).collect();
        let interval_pos = intervals.iter().position(|m| *m == current).unwrap_or(0);
        let update_interval = create("BUTTON", &interval_items[interval_pos], ID_UPDATE_INTERVAL,
            BS_OWNERDRAW | WS_TABSTOP);
        selects.push(Select { hwnd: update_interval as isize, items: interval_items,
            index: interval_pos });
        let update_button = create("BUTTON", "检查更新", ID_UPDATE, BS_OWNERDRAW | WS_TABSTOP);
        let startup_title = create("STATIC", "启动", 0, 0);
        set_font(startup_title, section_font);
        // About page: a project block and an update block, nothing else.
        let project_title = create("STATIC", "项目", 0, 0);
        set_font(project_title, section_font);
        let open_repo = create("BUTTON", "打开仓库", ID_OPEN_REPO, BS_OWNERDRAW | WS_TABSTOP);
        let update_title = create("STATIC", "更新", 0, 0);
        set_font(update_title, section_font);
        let version_label = create("STATIC", &format!("当前版本 v{}", super::VERSION), 0, 0);
        let interval_caption = create("STATIC", "检查间隔", 0, 0);
        // In-app theme switch: system, light, dark.
        let theme_label = create("STATIC", "主题", 0, 0);
        set_font(theme_label, section_font);
        let theme_items: Vec<String> =
            ["跟随系统", "浅色", "深色"].iter().map(|label| label.to_string()).collect();
        let theme_pos = match values.theme.as_str() { "light" => 1, "dark" => 2, _ => 0 };
        let theme_picker = create("BUTTON", &theme_items[theme_pos], ID_THEME,
            BS_OWNERDRAW | WS_TABSTOP);
        selects.push(Select { hwnd: theme_picker as isize, items: theme_items, index: theme_pos });
        // No Save button and no footer: every change applies at once and the
        // caption bar already closes the window.
        let shortcut_caption = create("STATIC", "输入英文", 0, 0);
        set_font(shortcut_caption, section_font);
        // Shortcuts are captured from the real keyboard: each row is a
        // read-only field that swallows the next key press, and the "+" button
        // appends one. A row that ends up empty is pruned again on save.
        let stored = shortcut_list(&values.alt_output_shortcut);
        let mut shortcut_rows: Vec<(HWND, String)> = Vec::new();
        let mut shortcut_keys = [ptr::null_mut(); MAX_SHORTCUTS];
        for slot in 0..MAX_SHORTCUTS {
            let spec = stored.get(slot).cloned().unwrap_or_default();
            let row = create("BUTTON", "", ID_SHORTCUT_KEY, BS_OWNERDRAW | WS_TABSTOP);
            SetWindowSubclass(row, Some(shortcut_proc), SUBCLASS_ID, 0);
            set_text(row, &row_text(&spec));
            shortcut_keys[slot] = row;
            shortcut_rows.push((row, spec));
        }
        reset_rows(shortcut_rows);
        CAPTURE.with(|c| c.borrow_mut().pending = None);
        let shortcut_add = create("BUTTON", "", ID_SHORTCUT_ADD, BS_OWNERDRAW | WS_TABSTOP);
        let about_text = create("STATIC",
            "仓库：github.com/gurio-wine/TransPop\r\n许可：MIT", 0, 0);

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
        (*theme_ptr).selects = selects;
        // Borderless fields are painted by the theme, so it must know them.
        for edit in [azure[0].1, azure[1].1, azure[2].1, openai[0].1, openai[1].1, openai[2].1,
                     timeout, debounce] {
            if !edit.is_null() { (*theme_ptr).fields.push(edit as isize); }
        }
        // Plain owner-drawn buttons: the two action buttons and the two
        // "显示" toggles beside the key fields.
        for button in [update_button, open_repo, azure_reveal, openai_reveal] {
            (*theme_ptr).buttons.push(button as isize);
        }
        for row in shortcut_keys { (*theme_ptr).shortcut_rows.push(row as isize); }
        (*theme_ptr).shortcut_plus = shortcut_add as isize;
        for tab in [tab_translate, tab_appearance, tab_system, tab_shortcut, tab_about] {
            (*theme_ptr).tabs.push(tab as isize);
        }
        apply_window_theme(hwnd, &mut *theme_ptr);

        let this = Self { hwnd, base, dpi, font, title_font, section_font, note_font, theme,
            brand_mark: mark, heading, service_title, service_divider, provider,
            connection_title, connection_divider, azure, openai, azure_reveal, openai_reveal,
            behavior_title, behavior_divider, timeout_label, timeout, timeout_unit,
            debounce_label, debounce, debounce_unit, autostart,
            shortcut_label: shortcut_caption, shortcut_keys, shortcut_add,
            auto_install, auto_restart, update_interval, update_intervals: intervals,
            update_button, startup_title, project_title, update_title, version_label,
            interval_caption, theme_label, theme_picker,
            tabs: [tab_translate, tab_appearance, tab_system, tab_shortcut, tab_about],
            about_text, open_repo };
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
        // layout() only reads the window, but the plate list it rebuilds lives
        // in the paint state, so it is reached through a raw pointer.
        let theme = &*self.theme as *const UiTheme as *mut UiTheme;
        (*theme).plates.clear();
        let plates = &mut (*theme).plates;
        let mut place = |control: HWND, x: i32, y: i32, width: i32, height: i32| {
            if control.is_null() { return; }
            if (*self.theme).fields.contains(&(control as isize)) {
                // A field is a painted rounded plate with a one-line edit
                // floating on it: the edit cannot round its own corners and
                // cannot move its text down, so both are done by geometry.
                let line = (*self.theme).line_h.max(1);
                let plate = RECT {
                    left: s(x),
                    top: s(y),
                    right: s(x + width),
                    bottom: s(y + CONTROL_H),
                };
                plates.push((control as isize, plate));
                let inset = (s(CONTROL_H) - line) / 2;
                SetWindowPos(control, ptr::null_mut(), s(x), s(y) + inset, s(width), line,
                    SWP_NOZORDER | SWP_NOACTIVATE);
                return;
            }
            SetWindowPos(control, ptr::null_mut(), s(x), s(y), s(width), s(height),
                SWP_NOZORDER | SWP_NOACTIVATE);
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
        let selected = (*self.theme).chosen_index(self.provider).unwrap_or(0) as isize;
        let advanced = selected == 1 || selected == 2;
        let translating = (*self.theme).page == 0;
        let appearance = (*self.theme).page == 1;
        let system = (*self.theme).page == 2;
        let shortcuts = (*self.theme).page == 3;
        let about = (*self.theme).page == 4;

        // Translation page. The credential block only exists for providers that
        // need one, so 翻译行为 slides up instead of leaving a hole.
        place(self.service_title, MARGIN_X, top, 460, 22);
        place(self.provider, MARGIN_X, top + 28, CONTENT_W, CONTROL_H);
        let mut y = top + 74;
        place(self.connection_title, MARGIN_X, y, 460, 22);
        let rows = if selected == 1 { &self.azure } else { &self.openai };
        let reveal = if selected == 1 { self.azure_reveal } else { self.openai_reveal };
        for (row, (label, edit)) in rows.iter().enumerate() {
            let row_y = y + 28 + row as i32 * 36;
            place(*label, MARGIN_X, row_y + 4, LABEL_W, 22);
            let width = if row == 2 { SECRET_W } else { FIELD_W };
            place(*edit, FIELD_X, row_y, width, CONTROL_H);
            if row == 2 { place(reveal, 485, row_y + 4, 61, CONTROL_H - 8); }
        }
        if advanced { y += 28 + 2 * 36 + 28; }
        place(self.behavior_divider, MARGIN_X, y, CONTENT_W, 1);
        y += 18;
        place(self.behavior_title, MARGIN_X, y, 460, 22);
        y += 32;
        // Both numeric fields share one width, so their unit labels line up.
        place(self.timeout_label, MARGIN_X, y + 4, LABEL_W, 22);
        place(self.timeout, FIELD_X, y, SHORT_FIELD_W, CONTROL_H);
        place(self.timeout_unit, FIELD_X + SHORT_FIELD_W + 8, y + 4, 140, 22);
        y += 36;
        place(self.debounce_label, MARGIN_X, y + 4, LABEL_W, 22);
        place(self.debounce, FIELD_X, y, SHORT_FIELD_W, CONTROL_H);
        place(self.debounce_unit, FIELD_X + SHORT_FIELD_W + 8, y + 4, 60, 22);

        // Appearance page.
        place(self.theme_label, MARGIN_X, top, 460, 22);
        place(self.theme_picker, MARGIN_X, top + 28, CONTENT_W, CONTROL_H);

        // System page: startup only; updates live on 关于.
        place(self.startup_title, MARGIN_X, top, 460, 22);
        place(self.autostart, MARGIN_X, top + 28, CONTENT_W, 24);

        // 快捷键 page: one row per shortcut, each capturing whatever key is
        // pressed. The "+" sits beside the last row and disappears when the
        // list is full.
        place(self.shortcut_label, MARGIN_X, top + 4, LABEL_W, 22);
        let rows = self.visible_shortcuts();
        for (index, row) in rows.iter().enumerate() {
            place(*row, FIELD_X, top + index as i32 * (CONTROL_H + SHORTCUT_GAP),
                SHORTCUT_W, CONTROL_H);
        }
        if let Some(last) = rows.len().checked_sub(1) {
            place(self.shortcut_add, FIELD_X + SHORTCUT_W + 8,
                top + last as i32 * (CONTROL_H + SHORTCUT_GAP), SHORTCUT_ADD_W, CONTROL_H);
        }

        // About page: a project block (repo, licence, one button) and an
        // update block (version, check button, interval, install switch).
        place(self.project_title, MARGIN_X, top, 460, 22);
        place(self.about_text, MARGIN_X, top + 28, CONTENT_W, 56);
        place(self.open_repo, MARGIN_X, top + 90, 112, CONTROL_H);
        place(self.connection_divider, MARGIN_X, top + 138, CONTENT_W, 1);
        place(self.update_title, MARGIN_X, top + 156, 460, 22);
        place(self.version_label, MARGIN_X, top + 184, 200, 22);
        place(self.update_button, MARGIN_X, top + 212, 112, CONTROL_H);
        place(self.interval_caption, MARGIN_X, top + 252, LABEL_W, 22);
        place(self.update_interval, FIELD_X, top + 248, 160, CONTROL_H);
        place(self.auto_install, MARGIN_X, top + 286, CONTENT_W, 24);
        place(self.auto_restart, MARGIN_X, top + 314, CONTENT_W, 24);

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
        for control in [self.autostart, self.startup_title] { show(control, system); }
        show(self.shortcut_label, shortcuts);
        let rows = self.visible_shortcuts();
        for row in self.shortcut_keys { show(row, shortcuts && rows.contains(&row)); }
        show(self.shortcut_add, shortcuts && rows.len() < MAX_SHORTCUTS);
        for control in [self.project_title, self.about_text, self.open_repo,
                        self.connection_divider, self.update_title, self.version_label,
                        self.update_button, self.interval_caption, self.update_interval,
                        self.auto_install, self.auto_restart] {
            show(control, about);
        }

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
        // Windows 10 has no rounded frame, so the frame is cut to the same
        // radius as the controls inside it.
        round_window(self.hwnd, width, height, s(WINDOW_RADIUS) * 2);
        // The plate list was rebuilt: erase whatever the previous page painted.
        InvalidateRect(self.hwnd, ptr::null(), 1);
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
        // The plates of the page just left are still painted, and the new
        // page's fields are hidden, so the whole client is erased and repainted.
        InvalidateRect(self.hwnd, ptr::null(), 1);
        for tab in self.tabs {
            if !tab.is_null() { InvalidateRect(tab, ptr::null(), 0); }
        }
    }

    /// Shows the current shortcut rows and focuses the armed one, so "+" and
    /// clearing a row both take effect without reopening the dialog.
    pub unsafe fn refresh_shortcuts(&self) {
        self.apply_layout(false);
        if CAPTURE.with(|c| c.borrow().armed) {
            if let Some(row) = self.visible_shortcuts().last() { SetFocus(*row); }
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
        CAPTURE.with(|c| c.borrow_mut().pending = None);
        self.seed_shortcuts(&shortcut_list(&values.alt_output_shortcut));
        set_button_checked(self.auto_install, values.auto_install);
        set_button_checked(self.auto_restart, values.auto_restart);
        let current: u32 = values.update_interval_minutes.parse()
            .unwrap_or(DEFAULT_UPDATE_INTERVAL);
        if let Some(index) = self.update_intervals.iter().position(|m| *m == current) {
            let theme = self.theme.as_ref() as *const UiTheme as *mut UiTheme;
            (*theme).set_selection(self.update_interval, index);
            set_text(self.update_interval, &interval_label(current));
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
        let selected = (*self.theme).chosen_index(self.provider).unwrap_or(0);
        let provider = match selected {
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
            alt_output_shortcut: shortcuts_value(&row_specs()),
            auto_install: button_checked(self.auto_install),
            auto_restart: button_checked(self.auto_restart),
            update_interval_minutes: {
                let index = (*self.theme).chosen_index(self.update_interval).unwrap_or(0);
                let minutes = self.update_intervals.get(index)
                    .copied().unwrap_or(DEFAULT_UPDATE_INTERVAL);
                minutes.to_string()
            },
            theme: match (*self.theme).chosen_index(self.theme_picker).unwrap_or(0) {
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

    /// Rows the form shows right now: the set shortcuts, plus one row while "+"
    /// is waiting for its key. Never empty, and never more than the row count.
    fn visible_shortcuts(&self) -> Vec<HWND> {
        let filled = SHORTCUTS.with(|rows| rows.borrow().iter()
            .filter(|(_, spec)| !spec.is_empty() && spec != "none").count());
        let extra = if CAPTURE.with(|c| c.borrow().extra) { 1 } else { 0 };
        let shown = (filled + extra).max(1).min(MAX_SHORTCUTS);
        self.shortcut_keys[..shown].to_vec()
    }

    /// Writes a stored shortcut list back into the row fields.
    unsafe fn seed_shortcuts(&self, stored: &[String]) {
        let rows: Vec<(HWND, String)> = self.shortcut_keys.iter().enumerate()
            .map(|(index, row)| (*row, stored.get(index).cloned().unwrap_or_default()))
            .collect();
        reset_rows(rows);
        for (index, row) in self.shortcut_keys.iter().enumerate() {
            set_text(*row, &row_text(stored.get(index).map(String::as_str).unwrap_or("")));
        }
    }

    pub unsafe fn close(self) {
        // Clear the paint state pointer before children are destroyed.
        SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
        CAPTURE.with(|c| c.borrow_mut().pending = None);
        reset_rows(Vec::new());
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
        // The retired auto_update=off switch maps onto the 仅手动 interval.
        assert_eq!(file.update_interval_minutes, "0");
        assert_eq!(file.alt_output_shortcut, "ctrl+space");
        assert_eq!(file.openai_url, "http://127.0.0.1:8080/v1/chat/completions");
        assert_eq!(file.azure_url, DEFAULT_AZURE_URL);
        assert_eq!(file.openai_model, DEFAULT_OPENAI_MODEL);
        let values = SettingsValues::from_file(&base);
        assert_eq!(values.provider, file.provider);
        assert_eq!(values.update_interval_minutes, file.update_interval_minutes);
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
        assert!(text.contains("update_interval_minutes=30"), "sample missing interval: {text}");
        assert_eq!(ConfigFile::read(&base).update_interval_minutes, "30");
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
        assert!(values.auto_install, "installing defaults to on");
        let mut quiet = values.clone();
        quiet.provider = "bing".into();
        quiet.auto_install = false;
        quiet.save(&base).unwrap();
        let text = fs::read_to_string(base.join("translator.conf")).unwrap();
        assert!(text.contains("auto_install=off"), "auto_install not saved: {text}");
        assert!(!text.contains("auto_update="), "retired key must not come back: {text}");
        assert_eq!(SettingsValues::from_file(&base).update_interval_minutes, "30");
        let mut short = values.clone();
        short.provider = "bing".into();
        short.update_interval_minutes = "14".into();
        assert!(short.validate().is_err(), "14 minutes must be rejected");
        short.update_interval_minutes = "15".into();
        assert!(short.validate().is_ok(), "15 minutes must be accepted");
        short.update_interval_minutes = MANUAL_UPDATE_INTERVAL.to_string();
        assert!(short.validate().is_ok(), "0 means check only when asked");
        assert_eq!(interval_label(MANUAL_UPDATE_INTERVAL), "\u{4ec5}\u{624b}\u{52a8}");
        assert_eq!(interval_label(10080), "\u{6bcf} 7 \u{5929}");
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
        // A list keeps order, drops repeats and junk, and caps at the row count.
        assert_eq!(shortcut_list("lalt, Ctrl+Space , lalt, nope"),
            vec!["lalt".to_string(), "ctrl+space".to_string()]);
        assert_eq!(shortcut_list(""), Vec::<String>::new());
        assert_eq!(shortcuts_value(&["lalt".to_string(), "ctrl+space".to_string()]),
            "lalt,ctrl+space");
        assert_eq!(shortcuts_value(&[]), "none");
        assert_eq!(shortcuts_value(&["nope".to_string()]), "none");
        assert_eq!(normalize_theme(" Dark "), "dark");
        assert_eq!(normalize_theme("LIGHT"), "light");
        assert_eq!(normalize_theme("auto"), "system");
        let _ = fs::remove_dir_all(&base);
    }
}
