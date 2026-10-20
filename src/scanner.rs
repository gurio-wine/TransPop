//! Native HWND discovery + Microsoft UI Automation Raw View candidate reading.
//! Everything in this module runs on a dedicated MTA thread. COM objects never cross threads.
use std::{collections::HashSet, mem};

use uiautomation::{
    core::{UIAutomation, UIElement, UITreeWalker},
    types::{ControlType, Handle},
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, RECT},
    UI::WindowsAndMessaging::{EnumChildWindows, EnumWindows, GetClassNameW, GetWindowRect, IsWindowVisible},
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
    pub candidate: Option<Candidate>,
    pub top_windows: usize,
    pub child_windows: usize,
    pub core_windows: usize,
    pub uia_checked: usize,
    pub uia_errors: usize,
    pub panels: usize,
}

#[derive(Default)]
struct Enumerated {
    top: usize,
    children: usize,
    handles: Vec<isize>,
}

unsafe extern "system" fn child_cb(hwnd: HWND, param: LPARAM) -> i32 {
    // The parameter points into a Vec that lives for the duration of EnumChildWindows.
    let state = unsafe { &mut *(param as *mut Enumerated) };
    state.children += 1;
    add_candidate_hwnd(hwnd, state);
    1
}

unsafe extern "system" fn top_cb(hwnd: HWND, param: LPARAM) -> i32 {
    let state = unsafe { &mut *(param as *mut Enumerated) };
    state.top += 1;
    add_candidate_hwnd(hwnd, state);
    // Crucial for Win10: TextInputHost's CoreWindow is nested below ApplicationFrameWindow.
    // EnumWindows alone cannot find this nested HWND.
    unsafe { EnumChildWindows(hwnd, Some(child_cb), param); }
    1
}

fn add_candidate_hwnd(hwnd: HWND, state: &mut Enumerated) {
    unsafe {
        if IsWindowVisible(hwnd) == 0 { return; }
        let mut buffer = [0_u16; 192];
        let n = GetClassNameW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
        if n <= 0 { return; }
        let class = String::from_utf16_lossy(&buffer[..n as usize]);
        // Known Win10 Microsoft Pinyin host and possible other IME host classes.
        if !(class == "Windows.UI.Core.CoreWindow"
            || class.contains("Candidate")
            || class.contains("IME")) { return; }
        let mut bounds: RECT = mem::zeroed();
        if GetWindowRect(hwnd, &mut bounds) == 0 { return; }
        let width = bounds.right - bounds.left;
        let height = bounds.bottom - bounds.top;
        // Ignore invisible, zero-area and unusually large UWP application windows.
        if !(80..=2800).contains(&width) || !(20..=850).contains(&height) { return; }
        state.handles.push(hwnd as isize);
    }
}

fn enumerate_hwnds() -> Enumerated {
    let mut state = Enumerated::default();
    unsafe { EnumWindows(Some(top_cb), &mut state as *mut _ as isize); }
    let mut dedup = HashSet::new();
    state.handles.retain(|h| dedup.insert(*h));
    state
}

fn is_panel(id: &str) -> bool {
    matches!(id, "IME_Prediction_Window" | "IME_Candidate_Window")
}

fn list_item_first(panel: &UIElement, walker: &UITreeWalker) -> Option<String> {
    // In the user's Win10 v0.3 trace, candidates are direct ListItem children
    // of IME_Prediction_Window. Keep that order: item 1 is the first candidate.
    if let Some(children) = walker.get_children(panel) {
        for child in &children {
            if child.get_control_type().ok() == Some(ControlType::ListItem) {
                if let Ok(s) = child.get_name() {
                    if !s.trim().is_empty() { return Some(s); }
                }
                // Some IMEs place candidate text in a nested TextBlock.
                if let Some(grandchildren) = walker.get_children(child) {
                    for grandchild in grandchildren {
                        if grandchild.get_automation_id().unwrap_or_default()
                            == "TEMPLATE_PART_CandidateItemText" {
                            if let Ok(s) = grandchild.get_name() {
                                if !s.trim().is_empty() { return Some(s); }
                            }
                        }
                    }
                }
            }
        }
        // Fallback: UIA providers that expose a candidate TextBlock but not ListItem.
        for child in &children {
            if child.get_automation_id().unwrap_or_default() == "TEMPLATE_PART_CandidateItemText" {
                if let Ok(s) = child.get_name() {
                    if !s.trim().is_empty() { return Some(s); }
                }
            }
        }
    }
    None
}

fn find_panel(root: &UIElement, walker: &UITreeWalker) -> Option<UIElement> {
    // BFS limited to 5 levels and 96 nodes; never traverse the whole desktop UIA tree.
    let mut queue = std::collections::VecDeque::from([(root.clone(), 0_usize)]);
    let mut visited = 0;
    while let Some((node, depth)) = queue.pop_front() {
        visited += 1;
        if visited > 96 { break; }
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
        ..ScanReport::default()
    };
    // Limit UIA calls, which can cross process boundaries and be slow.
    for handle in all.handles.iter().take(32) {
        report.uia_checked += 1;
        let root = match uia.element_from_handle(Handle::from(*handle)) {
            Ok(root) => root,
            Err(_) => { report.uia_errors += 1; continue; }
        };
        let Some(panel) = find_panel(&root, walker) else { continue; };
        report.panels += 1;
        if panel.is_offscreen().unwrap_or(false) { continue; }
        let Some(text) = list_item_first(&panel, walker) else { continue; };
        let Ok(rect) = panel.get_bounding_rectangle() else { continue; };
        let left = rect.get_left();
        let top = rect.get_top();
        let right = rect.get_right();
        let bottom = rect.get_bottom();
        if right <= left || bottom <= top { continue; }
        report.candidate = Some(Candidate {
            text: text.chars().take(120).collect(), x: left, y: top, right, bottom,
        });
        break;
    }
    report
}
