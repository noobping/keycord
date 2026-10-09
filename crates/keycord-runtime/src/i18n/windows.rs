//! Follow the Windows display language unless the user supplied a locale override.

use std::ffi::{c_char, c_int, CString};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetUserDefaultUILanguage() -> u16;
    fn LCIDToLocaleName(locale: u32, name: *mut u16, length: c_int, flags: u32) -> c_int;
}

unsafe extern "C" {
    fn _putenv_s(name: *const c_char, value: *const c_char) -> c_int;
}

pub(super) fn configure_language() -> Result<(), String> {
    if ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
    {
        return Ok(());
    }

    // LOCALE_NAME_MAX_LENGTH includes the terminating NUL. A LANGID with the
    // default sort order is also an LCID; use the UI language, not the region.
    let mut buffer = [0u16; 85];
    let length = unsafe {
        LCIDToLocaleName(
            u32::from(GetUserDefaultUILanguage()),
            buffer.as_mut_ptr(),
            buffer.len() as c_int,
            0,
        )
    };
    let locale = if length > 1 {
        String::from_utf16_lossy(&buffer[..length as usize - 1]).replace('-', "_")
    } else {
        "en".to_string()
    };
    let value = CString::new(locale.as_str()).map_err(|error| error.to_string())?;

    // Called during single-threaded startup, before GTK or workers. libintl
    // reads the C runtime environment; Rust updates the Win32 environment only.
    unsafe {
        let error = _putenv_s(c"LC_MESSAGES".as_ptr(), value.as_ptr());
        if error != 0 {
            return Err(std::io::Error::from_raw_os_error(error).to_string());
        }
        std::env::set_var("LC_MESSAGES", locale);
    }
    Ok(())
}
