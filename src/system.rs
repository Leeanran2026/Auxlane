use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{HSTRING, PCWSTR};

use crate::error::CommandError;

pub fn open_bluetooth_settings() -> Result<(), CommandError> {
    let uri = HSTRING::from("ms-settings:bluetooth");
    let result = unsafe {
        ShellExecuteW(
            None,
            windows::core::w!("open"),
            PCWSTR(uri.as_ptr()),
            windows::core::w!(""),
            windows::core::w!(""),
            SW_SHOWNORMAL,
        )
    };

    let code = result.0 as isize;
    if code <= 32 {
        Err(CommandError::settings(code))
    } else {
        Ok(())
    }
}
