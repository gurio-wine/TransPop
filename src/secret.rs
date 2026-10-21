//! Windows DPAPI wrapping for API keys stored in translator.conf.
//!
//! Stored form is `dpapi:` followed by lowercase hex, encrypted for the current
//! Windows user. Legacy plaintext values are still accepted so an upgrade never
//! makes a configured key disappear; the app re-encrypts them on the next save
//! and once at startup.

use std::ptr;
use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN,
    },
};

const PREFIX: &str = "dpapi:";
// Bound to this application: another program running as the same Windows user
// cannot reuse a copied blob without the same entropy.
const ENTROPY: &[u8] = b"TransPop/translator.conf/api-keys/v1";

pub fn is_sealed(value: &str) -> bool {
    value.starts_with(PREFIX)
}

/// Encrypt a key for the current user. Empty stays empty; an already sealed
/// value passes through so a save cannot double-encrypt an unreadable key.
pub fn seal(value: &str) -> Result<String, String> {
    if value.is_empty() {
        return Ok(String::new());
    }
    if is_sealed(value) {
        return Ok(value.to_string());
    }
    let encrypted = crypt(true, value.as_bytes())?;
    Ok(format!("{PREFIX}{}", to_hex(&encrypted)))
}

/// Read a stored key. Legacy plaintext passes through unchanged. A key that
/// cannot be decrypted becomes empty and is logged: sending ciphertext to a
/// translation service would be worse than asking the user again.
pub fn open(stored: &str) -> String {
    if stored.is_empty() {
        return String::new();
    }
    let Some(encoded) = stored.strip_prefix(PREFIX) else {
        return stored.to_string();
    };
    let plain = from_hex(encoded)
        .ok_or_else(|| "invalid hex".to_string())
        .and_then(|blob| crypt(false, &blob))
        .and_then(|bytes| String::from_utf8(bytes).map_err(|_| "not UTF-8".to_string()));
    match plain {
        Ok(value) => value,
        Err(reason) => {
            super::log_global(format!("SECRET_DECRYPT_FAILED {reason}"));
            String::new()
        }
    }
}

fn crypt(protect: bool, data: &[u8]) -> Result<Vec<u8>, String> {
    let mut input = data.to_vec();
    let mut entropy = ENTROPY.to_vec();
    let in_blob = CRYPT_INTEGER_BLOB { cbData: input.len() as u32, pbData: input.as_mut_ptr() };
    let entropy_blob = CRYPT_INTEGER_BLOB {
        cbData: entropy.len() as u32,
        pbData: entropy.as_mut_ptr(),
    };
    let mut out = CRYPT_INTEGER_BLOB::default();
    let ok = unsafe {
        if protect {
            CryptProtectData(
                &in_blob,
                ptr::null(),
                &entropy_blob,
                ptr::null(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        } else {
            CryptUnprotectData(
                &in_blob,
                ptr::null_mut(),
                &entropy_blob,
                ptr::null(),
                ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        }
    };
    if ok == 0 {
        return Err(format!("DPAPI {}", std::io::Error::last_os_error()));
    }
    let result = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec() };
    unsafe { LocalFree(out.pbData as *mut core::ffi::c_void); }
    Ok(result)
}

fn to_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        text.push(DIGITS[(byte >> 4) as usize] as char);
        text.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    text
}

fn from_hex(text: &str) -> Option<Vec<u8>> {
    let digits = text.as_bytes();
    if digits.len() % 2 != 0 {
        return None;
    }
    let value = |byte: u8| (byte as char).to_digit(16).map(|digit| digit as u8);
    let mut bytes = Vec::with_capacity(digits.len() / 2);
    for pair in digits.chunks(2) {
        bytes.push((value(pair[0])? << 4) | value(pair[1])?);
    }
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sealed_values_round_trip_and_hide_plaintext() {
        let stored = seal("sk-test-1234").unwrap();
        assert!(stored.starts_with("dpapi:"));
        assert!(!stored.contains("sk-test-1234"));
        assert_eq!(open(&stored), "sk-test-1234");
        assert_eq!(seal(&stored).unwrap(), stored, "seal must not double-encrypt");
    }

    #[test]
    fn plaintext_and_empty_values_pass_through() {
        assert_eq!(open("legacy-plain-key"), "legacy-plain-key");
        assert_eq!(open(""), "");
        assert_eq!(seal("").unwrap(), "");
        assert!(!is_sealed("legacy-plain-key"));
    }

    #[test]
    fn corrupt_ciphertext_fails_closed_to_empty() {
        assert_eq!(open("dpapi:not-hex"), "");
        assert_eq!(open("dpapi:00112233"), "");
    }
}
