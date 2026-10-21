//! Locate the Microsoft Pinyin candidate HWND, then inspect its UIA Raw View.
//! Runs only on the scanner thread: do not send COM elements across threads.
use std::{collections::{HashSet, VecDeque}, mem};

use uiautomation::{
    core::{UIAutomation, UIElement, UITreeWalker},
    types::{ControlType, Handle},
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, POINT, RECT},
    UI::WindowsAndMessaging::{
        ClientToScreen, EnumChildWindows, EnumWindows, GetClassNameW, GetGUIThreadInfo,
        GetWindowRect, IsWindowVisible, WindowFromPoint, GUITHREADINFO,
    },
};

#[derive(Clone, Debug)]
pub struct Candidate {
    pub text: String,
    pub x: i32,
    pub y: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Clone, Debug, Default)]
pub struct ScanReport {
    pub examined: usize,
    pub candidate_hints: String,
    pub candidate: Option<Candidate>,
    pub top_windows: usize,
    pub child_windows: usize,
    pub core_windows: usize,
    pub core_seen: usize,
    pub core_visible: usize,
    pub caret_available: bool,
    pub caret_hits: usize,
    pub uia_checked: usize,
    pub uia_errors: usize,
    pub panels: usize,
    pub offscreen_panels: usize,
}

#[derive(Clone)]
struct Probe {
    hwnd: isize,
    width: i32,
    height: i32,
    visible: bool,
    from_caret: bool,
}

#[derive(Default)]
struct Enumerated {
    top: usize,
    children: usize,
    core_seen: usize,
    core_visible: usize,
    caret_available: bool,
    caret_hits: usize,
    handles: Vec<Probe>,
}

fn class_name(hwnd: HWND) -> Option<String> {
    unsafe {
        let mut buffer = [0_u16; 192];
        let n = GetClassNameW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
        if n <= 0 { return None; }
        Some(String::from_utf16_lossy(&buffer[..n as usize]))
    }
}

fn is_ime_host_class(class: &str) -> bool {
    class == "Windows.UI.Core.CoreWindow" || class.contains("Candidate") || class.contains("IME")
}

fn append_handle(hwnd: HWND, state: &mut Enumerated, from_caret: bool) {
    if hwnd.is_null() { return; }
    let Some(class) = class_name(hwnd) else { return; };
    if !is_ime_host_class(&class) { return; }
    let is_core = class == "Windows.UI.Core.CoreWindow";
    let visible = unsafe { IsWindowVisible(hwnd) != 0 };
    // For diagnostics, count matching windows *before* applying any geometric/visible filter.
    if !from_caret && is_core {
        state.core_seen += 1;
        if visible { state.core_visible += 1; }
    }
    let mut rect: RECT = unsafe { mem::zeroed() };
    if unsafe { GetWindowRect(hwnd, &mut rect) } == 0 { return; }
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    // Do not silently discard a genuine IME HWND based on width, height or visibility.
    // The UIA pane itself will later be validated by its AutomationId and bounds.
    state.handles.push(Probe {
        hwnd: hwnd as isize, width, height, visible, from_caret,
    });
}

unsafe extern "system" fn child_cb(hwnd: HWND, param: LPARAM) -> i32 {
    let state = unsafe { &mut *(param as *mut Enumerated) };
    state.children += 1;
    append_handle(hwnd, state, false);
    1
}

unsafe extern "system" fn top_cb(hwnd: HWND, param: LPARAM) -> i32 {
    let state = unsafe { &mut *(param as *mut Enumerated) };
    state.top += 1;
    append_handle(hwnd, state, false);
    unsafe { EnumChildWindows(hwnd, Some(child_cb), param); }
    1
}

fn add_near_caret_windows(state: &mut Enumerated) {
    // An independent fallback: query windows near the *typing caret*, not the mouse.
    // Useful if modern TextInputHost windows are missed by regular enumeration.
    unsafe {
        let mut info: GUITHREADINFO = mem::zeroed();
        info.cbSize = mem::size_of::<GUITHREADINFO>() as u32;
        if GetGUIThreadInfo(0, &mut info) == 0 || info.hwndCaret.is_null() { return; }
        let mut point = POINT { x: info.rcCaret.left, y: info.rcCaret.bottom };
        if ClientToScreen(info.hwndCaret, &mut point) == 0 { return; }
        state.caret_available = true;
        // Candidate list can appear below or above the caret. Sample its likely area.
        // No cursor movement, hooks or keystroke monitoring are involved.
        for dy in [-115, -75, -35, 9, 27, 47, 67, 93] {
            for dx in [12, 48, 125, 250] {
                let hwnd = WindowFromPoint(POINT { x: point.x + dx, y: point.y + dy });
                if hwnd.is_null() { continue; }
                if class_name(hwnd).as_deref().is_some_and(is_ime_host_class) {
                    state.caret_hits += 1;
                    append_handle(hwnd, state, true);
                }
            }
        }
    }
}

fn enumerate_hwnds() -> Enumerated {
    let mut state = Enumerated::default();
    unsafe { EnumWindows(Some(top_cb), &mut state as *mut _ as isize); }
    add_near_caret_windows(&mut state);
    let mut seen = HashSet::new();
    // Prefer a real visible host found next to the typing caret. Next prefer
    // visible IME-sized windows, then other CoreWindows for diagnostic inspection.
    state.handles.sort_by_key(|h| {
        let typical = (32..=300).contains(&h.height) && h.width >= 80;
        (!h.from_caret, !h.visible, !typical, h.height, h.width)
    });
    state.handles.retain(|h| seen.insert(h.hwnd));
    state
}

fn is_panel(id: &str) -> bool {
    matches!(id, "IME_Prediction_Window" | "IME_Candidate_Window")
}

fn list_item_first(panel: &UIElement, walker: &UITreeWalker) -> Option<String> {
    // The confirmed Windows 10 host exposes first candidate as the first ListItem.
    let children = walker.get_children(panel)?;
    for child in &children {
        if child.get_control_type().ok() == Some(ControlType::ListItem) {
            if let Ok(text) = child.get_name() {
                if !text.trim().is_empty() { return Some(text); }
            }
            if let Some(grandchildren) = walker.get_children(child) {
                for g in grandchildren {
                    if g.get_automation_id().unwrap_or_default() == "TEMPLATE_PART_CandidateItemText" {
                        if let Ok(text) = g.get_name() {
                            if !text.trim().is_empty() { return Some(text); }
                        }
                    }
                }
            }
        }
    }
    for child in &children {
        if child.get_automation_id().unwrap_or_default() == "TEMPLATE_PART_CandidateItemText" {
            if let Ok(text) = child.get_name() {
                if !text.trim().is_empty() { return Some(text); }
            }
        }
    }
    None
}

fn find_panel(root: &UIElement, walker: &UITreeWalker) -> Option<UIElement> {
    let mut queue = VecDeque::from([(root.clone(), 0_usize)]);
    let mut visited = 0;
    while let Some((node, depth)) = queue.pop_front() {
        visited += 1;
        if visited > 128 { break; }
        if let Ok(id) = node.get_automation_id() {
            if is_panel(&id) { return Some(node); }
        }
        if depth >= 5 { continue; }
        if let Some(children) = walker.get_children(&node) {
            for child in children { queue.push_back((child, depth + 1)); }
        }
    }
    None
}

pub fn scan(uia: &UIAutomation, walker: &UITreeWalker) -> ScanReport {
    let all = enumerate_hwnds();
    let mut report = ScanReport {
        top_windows: all.top,
        child_windows: all.children,
        core_windows: all.handles.len(),
        core_seen: all.core_seen,
        core_visible: all.core_visible,
        caret_available: all.caret_available,
        caret_hits: all.caret_hits,
        ..ScanReport::default()
    };
    report.candidate_hints = all.handles.iter().take(10)
        .map(|h| format!("0x{:X}({}x{},{},{})", h.hwnd, h.width, h.height,
            if h.visible { "V" } else { "H" }, if h.from_caret { "caret" } else { "enum" }))
        .collect::<Vec<_>>().join(" ");
    // Inspect only visible hosts; retain hidden hosts in stats for diagnosis.
    for probe in all.handles.iter().filter(|p| p.visible).take(96) {
        report.examined += 1;
        report.uia_checked += 1;
        let root = match uia.element_from_handle(Handle::from(probe.hwnd)) {
            Ok(root) => root,
            Err(_) => { report.uia_errors += 1; continue; }
        };
        let Some(panel) = find_panel(&root, walker) else { continue; };
        report.panels += 1;
        if panel.is_offscreen().unwrap_or(false) {
            report.offscreen_panels += 1;
            continue;
        }
        let Some(text) = list_item_first(&panel, walker) else { continue; };
        let Ok(rect) = panel.get_bounding_rectangle() else { continue; };
        let (left, top, right, bottom) = (
            rect.get_left(), rect.get_top(), rect.get_right(), rect.get_bottom(),
        );
        if right <= left || bottom <= top { continue; }
        report.candidate = Some(Candidate {
            text: text.chars().take(120).collect(), x: left, y: top, right, bottom,
        });
        break;
    }
    report
}
