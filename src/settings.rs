//! Tiny native Win32 settings window. Does not paint the translation bubble.
//! The window procedure only forwards form commands to the application's
//! existing message loop. No UIA or networking occurs in a Win32 callback.

use std::{collections::HashMap, fs, io::Write, mem, path::{Path, PathBuf}, ptr};
use windows_sys::Win32::{
    Foundation::HWND,
    Graphics::Gdi::{CreateFontW, DeleteObject, GetStockObject, SetBkMode, SetTextColor, HFONT},
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Input::KeyboardAndMouse::SetFocus,
        WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, GetSystemMetrics,
        GetWindowTextLengthW, GetWindowTextW, IsWindow, PostMessageW,
        RegisterClassW, SendMessageW, SetForegroundWindow, SetWindowPos,
        ShowWindow, WNDCLASSW, WS_BORDER, WS_CAPTION, WS_CHILD, WS_EX_APPWINDOW,
        WS_OVERLAPPED, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE, SW_HIDE, SW_SHOW,
        SWP_NOACTIVATE, SWP_NOZORDER, SWP_NOMOVE,
        WM_CLOSE, WM_COMMAND,
        },
    },
};

const CLASS: &str = "TransPopNativeSettings061";
const WM_SETFONT: u32 = 0x0030;
const ES_AUTOHSCROLL: u32 = 0x0080;
const BS_AUTOCHECKBOX: u32 = 0x0003;
const BM_GETCHECK: u32 = 0x00F0;
const BM_SETCHECK: u32 = 0x00F1;
const BST_CHECKED: usize = 1;
const ES_PASSWORD: u32 = 0x0020;
const CBS_DROPDOWNLIST: u32 = 0x0003;
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
const EM_SETPASSWORDCHAR: u32 = 0x00CC;
const WM_CTLCOLORSTATIC: u32 = 0x0138;
const SS_ICON: u32 = 0x0003;
const STM_SETICON: u32 = 0x0170;
const SS_ETCHEDHORZ: u32 = 0x0010;
const WM_APP_SETTINGS_SAVE: u32 = super::WM_APP + 30;
const WM_APP_SETTINGS_CANCEL: u32 = super::WM_APP + 31;
const WM_APP_SETTINGS_PROVIDER: u32 = super::WM_APP + 32;

pub const SAVE_MESSAGE: u32 = WM_APP_SETTINGS_SAVE;
pub const CANCEL_MESSAGE: u32 = WM_APP_SETTINGS_CANCEL;
pub const PROVIDER_MESSAGE: u32 = WM_APP_SETTINGS_PROVIDER;

fn wide(s: &str) -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() }

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: usize, lp: isize) -> isize {
    if msg == WM_CTLCOLORSTATIC {
        // Stock white brush: no custom paint routine, no owned HBRUSH lifecycle.
        SetBkMode(wp as _, 1); // TRANSPARENT
        SetTextColor(wp as _, 0x003E3330); // subtle charcoal, COLORREF BGR
        return GetStockObject(0) as isize; // WHITE_BRUSH
    }
    if msg == WM_CLOSE {
        let main = super::CONTROL_HWND.load(std::sync::atomic::Ordering::Acquire) as HWND;
        if !main.is_null() { PostMessageW(main, WM_APP_SETTINGS_CANCEL, 0, 0); }
        return 0;
    }
    if msg == WM_COMMAND {
        let id = (wp & 0xffff) as u16;
        let notify = ((wp >> 16) & 0xffff) as u16;
        let outgoing = if id == ID_SAVE { WM_APP_SETTINGS_SAVE }
            else if id == ID_CANCEL { WM_APP_SETTINGS_CANCEL }
            else if id == ID_PROVIDER && notify == CBN_SELCHANGE { WM_APP_SETTINGS_PROVIDER }
            else { 0 };
        if id == ID_AZURE_REVEAL || id == ID_OPENAI_REVEAL {
            // Password visibility is local to this settings dialog; it never
            // persists secrets or changes the saved values by itself.
            let secret_id = if id == ID_AZURE_REVEAL { ID_AZURE_SECRET } else { ID_OPENAI_SECRET };
            let edit = windows_sys::Win32::UI::WindowsAndMessaging::GetDlgItem(hwnd, secret_id as i32);
            let checkbox = windows_sys::Win32::UI::WindowsAndMessaging::GetDlgItem(hwnd, id as i32);
            if !edit.is_null() && !checkbox.is_null() {
                let shown = SendMessageW(checkbox, BM_GETCHECK, 0, 0) as usize == BST_CHECKED;
                SendMessageW(edit, EM_SETPASSWORDCHAR, if shown { 0 } else { '*' as usize }, 0);
                windows_sys::Win32::Graphics::Gdi::InvalidateRect(edit, ptr::null(), 1);
            }
            return 0;
        }
        if outgoing != 0 {
            let main = super::CONTROL_HWND.load(std::sync::atomic::Ordering::Acquire) as HWND;
            if !main.is_null() { PostMessageW(main, outgoing, 0, 0); }
            return 0;
        }
    }
    DefWindowProcW(hwnd, msg, wp, lp)
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
}
impl SettingsValues {
    pub fn from_file(base: &Path) -> Self {
        let mut map = HashMap::new();
        if let Ok(content) = fs::read_to_string(base.join("translator.conf")) {
            for line in content.lines() {
                let line = line.trim();
                if line.starts_with('#') { continue; }
                if let Some((k, v)) = line.split_once('=') {
                    map.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
                }
            }
        }
        let get = |key: &str, default: &str| map.get(key).cloned().unwrap_or_else(|| default.into());
        Self {
            provider: get("provider", "off").to_ascii_lowercase(),
            azure_url: get("azure_url", "https://api.cognitive.microsofttranslator.com/translate?api-version=3.0&to=en"),
            azure_region: get("azure_region", ""),
            azure_key: super::secret::open(&get("azure_key", "")),
            openai_url: get("openai_url", "https://api.openai.com/v1/chat/completions"),
            openai_model: get("openai_model", "gpt-4o-mini"),
            openai_key: super::secret::open(&get("openai_key", "")),
            timeout_seconds: get("timeout_seconds", "8"),
            debounce_ms: get("debounce_ms", "300"),
            alt_output: get("alt_output", "false").eq_ignore_ascii_case("true"),
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
        for v in [ &self.azure_url, &self.azure_region, &self.azure_key,
                   &self.openai_url, &self.openai_model, &self.openai_key ] {
            if v.contains('\n') || v.contains('\r') { return Err("配置值不能包含换行".into()); }
        }
        Ok(())
    }

    pub fn save(&self, base: &Path) -> Result<(), String> {
        self.validate()?;
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
        let sealed_azure = super::secret::seal(&self.azure_key)
            .map_err(|e| format!("无法加密保存 Azure API Key（{e}）"))?;
        let sealed_openai = super::secret::seal(&self.openai_key)
            .map_err(|e| format!("无法加密保存 OpenAI API Key（{e}）"))?;
        let entries = [
            ("provider", self.provider.clone()),
            ("azure_url", self.azure_url.clone()),
            ("azure_region", self.azure_region.clone()),
            ("azure_key", sealed_azure),
            ("openai_url", self.openai_url.clone()),
            ("openai_model", self.openai_model.clone()),
            ("openai_key", sealed_openai),
            ("timeout_seconds", self.timeout_seconds.clone()),
            ("debounce_ms", self.debounce_ms.clone()),
            ("alt_output", if self.alt_output { "true".to_string() } else { "false".to_string() }),
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
    provider: HWND,
    azure: [(HWND, HWND); 3],
    openai: [(HWND, HWND); 3],
    azure_reveal: HWND,
    openai_reveal: HWND,
    connection_title: HWND,
    connection_divider: HWND,
    behavior_title: HWND,
    behavior_divider: HWND,
    timeout_label: HWND,
    debounce_label: HWND,
    timeout_unit: HWND,
    debounce_unit: HWND,
    timeout: HWND,
    debounce: HWND,
    autostart: HWND,
    alt_output: HWND,
    autostart_help: HWND,
    footer_divider: HWND,
    footer_note: HWND,
    save_button: HWND,
    cancel_button: HWND,
    hint: HWND,
}

impl SettingsWindow {
    pub unsafe fn open(base: PathBuf, dpi: i32) -> Option<Self> {
        let instance = GetModuleHandleW(ptr::null());
        let class = wide(CLASS);
        let mut wc: WNDCLASSW = mem::zeroed();
        wc.lpfnWndProc = Some(proc);
        wc.hInstance = instance;
        wc.lpszClassName = class.as_ptr();
        wc.hbrBackground = GetStockObject(0) as _; // white, native Windows surface
        wc.hIcon = super::load_brand_icon(false);
        let _ = RegisterClassW(&wc);
        let scale = |v: i32| v * dpi / 96;
        let width = scale(586);
        let height = scale(548);
        let x = (GetSystemMetrics(0) - width).max(0) / 2;
        let y = (GetSystemMetrics(1) - height).max(0) / 2;
        let title = wide("TransPop · 偏好设置");
        let hwnd = CreateWindowExW(WS_EX_APPWINDOW, class.as_ptr(), title.as_ptr(),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
            x, y, width, height, ptr::null_mut(), ptr::null_mut(), instance, ptr::null());
        if hwnd.is_null() { return None; }

        let font_face = wide("Segoe UI");
        let font = CreateFontW(-scale(14), 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 0, font_face.as_ptr());
        let title_font = CreateFontW(-scale(22), 0, 0, 0, 600, 0, 0, 0, 1, 0, 0, 5, 0, font_face.as_ptr());
        let section_font = CreateFontW(-scale(15), 0, 0, 0, 600, 0, 0, 0, 1, 0, 0, 5, 0, font_face.as_ptr());
        let note_font = CreateFontW(-scale(12), 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 0, font_face.as_ptr());
        let mut create = |kind: &str, text: &str, id: u16,
                          xpos: i32, ypos: i32, w: i32, h: i32, style: u32| -> HWND {
            let c = wide(kind); let t = wide(text);
            let child = CreateWindowExW(0, c.as_ptr(), t.as_ptr(),
                WS_CHILD | WS_VISIBLE | style, scale(xpos), scale(ypos),
                scale(w), scale(h), hwnd, id as usize as HWND, instance, ptr::null());
            if !child.is_null() && !font.is_null() { SendMessageW(child, WM_SETFONT, font as usize, 1); }
            child
        };
        let set_font = |child: HWND, font: HFONT| {
            if !child.is_null() && !font.is_null() { SendMessageW(child, WM_SETFONT, font as usize, 1); }
        };
        let values = SettingsValues::from_file(&base);

        // Identity / information hierarchy; no oversized decorative icon cards.
        let mark = create("STATIC", "", 0, 26, 19, 40, 40, SS_ICON);
        let brand_icon = super::load_brand_icon(false);
        if !mark.is_null() && !brand_icon.is_null() {
            SendMessageW(mark, STM_SETICON, brand_icon as usize, 0);
        }
        let heading = create("STATIC", "TransPop", 0, 77, 19, 450, 30, 0);
        set_font(heading, title_font);
        let subtitle = create("STATIC", "偏好设置 · 让翻译恰到好处", 0, 78, 50, 460, 21, 0);
        set_font(subtitle, note_font);
        create("STATIC", "", 0, 26, 83, 520, 2, SS_ETCHEDHORZ);

        let service_title = create("STATIC", "翻译服务", 0, 26, 101, 480, 23, 0);
        set_font(service_title, section_font);
        let provider = create("COMBOBOX", "", ID_PROVIDER, 26, 133, 520, 200,
            CBS_DROPDOWNLIST | WS_TABSTOP);
        for label in ["Bing 翻译（无需密钥）", "Azure Translator", "OpenAI 兼容接口", "关闭翻译"] {
            let item = wide(label);
            SendMessageW(provider, CB_ADDSTRING, 0, item.as_ptr() as isize);
        }
        let pos = match values.provider.as_str() { "bing"=>0, "azure"=>1, "openai"=>2, _=>3 };
        SendMessageW(provider, CB_SETCURSEL, pos, 0);
        let hint = create("STATIC", "", 0, 26, 170, 515, 38, 0);
        set_font(hint, note_font);

        let connection_divider = create("STATIC", "", 0, 26, 201, 520, 2, SS_ETCHEDHORZ);
        let connection_title = create("STATIC", "服务连接", 0, 26, 216, 500, 23, 0);
        set_font(connection_title, section_font);
        let field = |create: &mut dyn FnMut(&str,&str,u16,i32,i32,i32,i32,u32)->HWND,
                     label: &str, y: i32, value: &str, secret_id: u16| -> (HWND, HWND) {
            let label_hwnd = create("STATIC", label, 0, 26, y + 5, 118, 24, 0);
            let width = if secret_id != 0 { 323 } else { 392 };
            let edit_hwnd = create("EDIT", value, secret_id, 154, y, width, 30,
                WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL |
                (if secret_id != 0 { ES_PASSWORD } else { 0 }));
            (label_hwnd, edit_hwnd)
        };
        let azure = [
            field(&mut create, "翻译地址", 245, &values.azure_url, 0),
            field(&mut create, "资源区域", 282, &values.azure_region, 0),
            field(&mut create, "API Key", 319, &values.azure_key, ID_AZURE_SECRET),
        ];
        let openai = [
            field(&mut create, "接口地址", 245, &values.openai_url, 0),
            field(&mut create, "模型名称", 282, &values.openai_model, 0),
            field(&mut create, "API Key", 319, &values.openai_key, ID_OPENAI_SECRET),
        ];
        let azure_reveal = create("BUTTON", "显示", ID_AZURE_REVEAL, 485, 321, 61, 26,
            BS_AUTOCHECKBOX | WS_TABSTOP);
        let openai_reveal = create("BUTTON", "显示", ID_OPENAI_REVEAL, 485, 321, 61, 26,
            BS_AUTOCHECKBOX | WS_TABSTOP);

        // This entire block reflows up if no credentials are required.
        let behavior_divider = create("STATIC", "", 0, 26, 205, 520, 2, SS_ETCHEDHORZ);
        let behavior_title = create("STATIC", "翻译行为", 0, 26, 220, 500, 23, 0);
        set_font(behavior_title, section_font);
        let timeout_label = create("STATIC", "请求超时", 0, 26, 261, 132, 24, 0);
        let timeout = create("EDIT", &values.timeout_seconds, 0, 154, 256, 85, 30,
            WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL);
        let timeout_unit = create("STATIC", "秒（2–30）", 0, 247, 261, 124, 24, 0);
        let debounce_label = create("STATIC", "翻译防抖", 0, 327, 261, 110, 24, 0);
        let debounce = create("EDIT", &values.debounce_ms, 0, 421, 256, 75, 30,
            WS_BORDER | WS_TABSTOP | ES_AUTOHSCROLL);
        let debounce_unit = create("STATIC", "ms", 0, 501, 261, 48, 24, 0);
        let autostart = create("BUTTON", "登录 Windows 时自动启动", 0,
            26, 317, 510, 25, BS_AUTOCHECKBOX | WS_TABSTOP);
        // An unreadable registry state must not look like "disabled": the next
        // Save would then delete a working autostart entry. Assume enabled.
        let previously_enabled = match super::startup::enabled_for_current_exe() {
            Ok(enabled) => enabled,
            Err(reason) => {
                super::log_global(format!("AUTOSTART_STATE_UNREADABLE {reason}"));
                true
            }
        };
        SendMessageW(autostart, BM_SETCHECK, if previously_enabled { BST_CHECKED } else { 0 }, 0);
        let alt_output = create("BUTTON", "轻按左 Alt：取消拼音并输入英文（实验性）", 0,
            26, 370, 510, 25, BS_AUTOCHECKBOX | WS_TABSTOP);
        SendMessageW(alt_output, BM_SETCHECK, if values.alt_output { BST_CHECKED } else { 0 }, 0);
        let autostart_help = create("STATIC", "仅当前用户 · 移动程序后需要重新启用", 0,
            49, 348, 492, 23, 0);
        set_font(autostart_help, note_font);
        let footer_divider = create("STATIC", "", 0, 26, 432, 520, 2, SS_ETCHEDHORZ);
        let footer_note = create("STATIC", "在线翻译会将候选文字发送给服务商", 0,
            26, 449, 298, 23, 0);
        set_font(footer_note, note_font);
        let save_button = create("BUTTON", "保存设置", ID_SAVE, 422, 402, 125, 34,
            WS_TABSTOP | 0x00000001); // BS_DEFPUSHBUTTON
        let cancel_button = create("BUTTON", "取消", ID_CANCEL, 334, 402, 78, 34, WS_TABSTOP);
        let this = Self { hwnd, base, dpi, font, title_font, section_font, note_font,
                          provider, azure, openai, azure_reveal, openai_reveal,
                          connection_title, connection_divider, behavior_title, behavior_divider,
                          timeout_label, debounce_label, timeout_unit, debounce_unit, timeout, debounce,
                          autostart, alt_output, autostart_help, footer_divider, footer_note,
                          save_button, cancel_button, hint };
        this.update_provider();
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

    pub unsafe fn update_provider(&self) {
        let index = SendMessageW(self.provider, CB_GETCURSEL, 0, 0);
        let hint = match index {
            0 => "无需 API Key · 使用微软 Edge 非公开翻译端点，可能失效或限流。",
            1 => "Azure 官方翻译服务 · 需要密钥，部分资源需要填写区域。",
            2 => "支持 OpenAI 兼容的 Chat Completions 接口 · 远程 HTTPS／本机 HTTP。",
            _ => "关闭在线翻译 · 不向服务商发送输入文字。",
        };
        windows_sys::Win32::UI::WindowsAndMessaging::SetWindowTextW(self.hint, wide(hint).as_ptr());
        let advanced = index == 1 || index == 2;
        for (label, edit) in self.azure {
            ShowWindow(label, if index == 1 { SW_SHOW } else { SW_HIDE });
            ShowWindow(edit, if index == 1 { SW_SHOW } else { SW_HIDE });
        }
        for (label, edit) in self.openai {
            ShowWindow(label, if index == 2 { SW_SHOW } else { SW_HIDE });
            ShowWindow(edit, if index == 2 { SW_SHOW } else { SW_HIDE });
        }
        ShowWindow(self.azure_reveal, if index == 1 { SW_SHOW } else { SW_HIDE });
        ShowWindow(self.openai_reveal, if index == 2 { SW_SHOW } else { SW_HIDE });
        ShowWindow(self.connection_divider, if advanced { SW_SHOW } else { SW_HIDE });
        ShowWindow(self.connection_title, if advanced { SW_SHOW } else { SW_HIDE });

        // Preserve current screen location: resize the dialog only, not the IME.
        let delta = if advanced { 164 } else { 0 };
        let scaled = |v: i32| v * self.dpi / 96;
        let move_to = |h: HWND, x: i32, y: i32, w: i32, height: i32| {
            if !h.is_null() {
                SetWindowPos(h, ptr::null_mut(), scaled(x), scaled(y), scaled(w), scaled(height),
                    SWP_NOZORDER | SWP_NOACTIVATE);
            }
        };
        move_to(self.behavior_divider, 26, 205 + delta, 520, 2);
        move_to(self.behavior_title, 26, 220 + delta, 500, 23);
        move_to(self.timeout_label, 26, 261 + delta, 132, 24);
        move_to(self.timeout, 154, 256 + delta, 85, 30);
        move_to(self.debounce_label, 327, 261 + delta, 110, 24);
        move_to(self.timeout_unit, 247, 261 + delta, 124, 24);
        move_to(self.debounce_unit, 501, 261 + delta, 48, 24);
        move_to(self.debounce, 421, 256 + delta, 75, 30);
        move_to(self.autostart, 26, 317 + delta, 510, 25);
        move_to(self.autostart_help, 49, 348 + delta, 492, 23);
        move_to(self.alt_output, 26, 370 + delta, 510, 25);
        move_to(self.footer_divider, 26, 432 + delta, 520, 2);
        move_to(self.footer_note, 26, 449 + delta, 328, 23);
        move_to(self.save_button, 422, 450 + delta, 125, 34);
        move_to(self.cancel_button, 334, 450 + delta, 78, 34);
        SetWindowPos(self.hwnd, ptr::null_mut(), 0, 0, scaled(586), scaled(548 + delta),
            SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOMOVE);
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
            alt_output: SendMessageW(self.alt_output, BM_GETCHECK, 0, 0) as usize == BST_CHECKED,
        }
    }
    pub unsafe fn autostart_checked(&self) -> bool {
        SendMessageW(self.autostart, BM_GETCHECK, 0, 0) as usize == BST_CHECKED
    }
    pub unsafe fn set_autostart_checked(&self, checked: bool) {
        SendMessageW(self.autostart, BM_SETCHECK, if checked { BST_CHECKED } else { 0 }, 0);
    }
    pub fn base(&self) -> &Path { &self.base }
    pub unsafe fn close(self) {
        DestroyWindow(self.hwnd);
        for font in [self.font, self.title_font, self.section_font, self.note_font] {
            if !font.is_null() { DeleteObject(font); }
        }
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
}

