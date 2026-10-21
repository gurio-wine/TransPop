//! Opt-in per-user startup registration. No administrator rights required.
//! Only HKCU\Software\Microsoft\Windows\CurrentVersion\Run\TransPop is touched.

use std::{env, path::PathBuf, ptr};
use windows_sys::Win32::System::Registry::{
    HKEY_CURRENT_USER, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
    REG_SZ, RRF_RT_REG_SZ,
};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "TransPop";
const ERROR_FILE_NOT_FOUND: u32 = 2;
const ERROR_PATH_NOT_FOUND: u32 = 3;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn expected_command() -> Result<String, String> {
    let path: PathBuf = env::current_exe()
        .map_err(|e| format!("无法确定当前 exe 路径：{e}"))?;
    let text = path.to_string_lossy();
    if text.contains('"') || text.contains('\n') || text.contains('\r') {
        return Err("exe 路径包含不支持的字符".into());
    }
    let command = format!("\"{text}\"");
    // Microsoft documents a 260-character limit for Run value commands.
    if command.encode_utf16().count() > 260 {
        return Err("exe 路径过长，超过 Windows 启动命令的长度限制".into());
    }
    Ok(command)
}

fn existing_command() -> Result<Option<String>, String> {
    let key = wide(RUN_KEY);
    let name = wide(VALUE_NAME);
    unsafe {
        let mut byte_count = 0u32;
        let mut value_type = 0u32;
        let status = RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(),
            RRF_RT_REG_SZ, &mut value_type, ptr::null_mut(), &mut byte_count);
        if status == ERROR_FILE_NOT_FOUND || status == ERROR_PATH_NOT_FOUND { return Ok(None); }
        if status != 0 { return Err(format!("无法读取启动项（错误码 {status}）")); }
        if byte_count == 0 || byte_count > 8192 || byte_count % 2 != 0 {
            return Err("启动项内容格式不正确".into());
        }
        let mut data = vec![0u8; byte_count as usize];
        let status = RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(),
            RRF_RT_REG_SZ, &mut value_type, data.as_mut_ptr() as *mut _, &mut byte_count);
        if status != 0 { return Err(format!("读取启动项内容失败（错误码 {status}）")); }
        if byte_count as usize > data.len() || byte_count % 2 != 0 {
            return Err("启动项数据长度异常".into());
        }
        let utf16: Vec<u16> = data[..byte_count as usize]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .take_while(|v| *v != 0)
            .collect();
        String::from_utf16(&utf16)
            .map(Some)
            .map_err(|_| "启动项包含无效的 Unicode 字符".into())
    }
}

/// Returns true only when the entry points to the current executable.
/// If the exe has moved, the checkbox is unchecked; saving it again updates the path.
pub fn enabled_for_current_exe() -> Result<bool, String> {
    let registered = existing_command()?;
    let expected = expected_command()?;
    Ok(registered.is_some_and(|v| v.eq_ignore_ascii_case(&expected)))
}

/// Called *only* when the user clicks Save in the settings window.
pub fn set_enabled(enabled: bool) -> Result<(), String> {
    let key = wide(RUN_KEY);
    let name = wide(VALUE_NAME);
    unsafe {
        if enabled {
            let cmd = expected_command()?;
            let data = wide(&cmd);
            let bytes = (data.len() * std::mem::size_of::<u16>()) as u32;
            let status = RegSetKeyValueW(HKEY_CURRENT_USER,
                key.as_ptr(), name.as_ptr(), REG_SZ,
                data.as_ptr() as *const _, bytes);
            if status != 0 { return Err(format!("注册开机自启失败（错误码 {status}）")); }
        } else {
            let status = RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr());
            if status != 0 && status != ERROR_FILE_NOT_FOUND && status != ERROR_PATH_NOT_FOUND {
                return Err(format!("关闭开机自启失败（错误码 {status}）"));
            }
        }
    }
    Ok(())
}
