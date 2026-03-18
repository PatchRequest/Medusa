//! Helper traits for converting Rust strings to Windows `UNICODE_STRING`.

use alloc::vec::Vec;
use wdk_sys::UNICODE_STRING;

/// Trait to convert a string-like type into a null-terminated `Vec<u16>`.
pub trait ToUnicodeString {
    /// Converts self to a null-terminated UTF-16 vector.
    fn to_u16_vec(&self) -> Vec<u16>;
}

impl ToUnicodeString for &str {
    fn to_u16_vec(&self) -> Vec<u16> {
        let mut buf = Vec::with_capacity(self.len() + 1);
        for c in self.chars() {
            let mut c_buf = [0; 2];
            let encoded = c.encode_utf16(&mut c_buf);
            buf.extend_from_slice(encoded);
        }
        buf.push(0); // null terminator
        buf
    }
}

/// Trait to convert a `Vec<u16>` into a Windows `UNICODE_STRING`.
pub trait ToWindowsUnicodeString {
    /// Converts self into an `Option<UNICODE_STRING>`.
    fn to_windows_unicode_string(&self) -> Option<UNICODE_STRING>;
}

impl ToWindowsUnicodeString for Vec<u16> {
    fn to_windows_unicode_string(&self) -> Option<UNICODE_STRING> {
        create_unicode_string(self)
    }
}

/// Creates a Windows `UNICODE_STRING` from a `u16` slice.
///
/// Returns `None` if the input is empty.
///
/// # Notes
/// - `Length` excludes the null terminator (in bytes).
/// - `MaximumLength` includes it (in bytes).
/// - The returned `UNICODE_STRING` borrows the slice — caller must ensure
///   the slice outlives the `UNICODE_STRING`.
pub fn create_unicode_string(s: &[u16]) -> Option<UNICODE_STRING> {
    if s.is_empty() {
        return None;
    }

    let len = s.len();

    // Exclude null terminator from Length if present
    let len_checked = if len > 0 && s[len - 1] == 0 {
        len - 1
    } else {
        len
    };

    Some(UNICODE_STRING {
        Length: (len_checked * 2) as u16,
        MaximumLength: (len * 2) as u16,
        Buffer: s.as_ptr() as *mut u16,
    })
}
