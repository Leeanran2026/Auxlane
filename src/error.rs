use windows::core::Error as WinError;

#[derive(Clone, Debug)]
pub struct CommandError {
    pub code: String,
    pub hresult: Option<u32>,
    pub message: String,
}

impl CommandError {
    pub fn new(code: &str, hresult: Option<u32>, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            hresult,
            message: message.into(),
        }
    }

    pub fn windows(error: WinError) -> Self {
        Self::new("windows", Some(error.code().0 as u32), error.message())
    }

    pub fn unsupported(error: WinError) -> Self {
        Self::new(
            "unsupported",
            Some(error.code().0 as u32),
            "This device does not support audio playback connections.",
        )
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        Self::new("timeout", None, message)
    }

    pub fn denied() -> Self {
        Self::new("denied", None, "Windows denied the connection request.")
    }

    pub fn unknown_failure(extended_error: i32) -> Self {
        let hresult = (extended_error != 0).then_some(extended_error as u32);
        Self::new("unknownFailure", hresult, "The connection attempt failed.")
    }

    pub fn not_ready() -> Self {
        Self::new(
            "notReady",
            None,
            "The connection opened but did not become ready.",
        )
    }

    pub fn state_unavailable() -> Self {
        Self::new("stateUnavailable", None, "Connection state is unavailable.")
    }

    pub fn shutting_down() -> Self {
        Self::new("shuttingDown", None, "The application is shutting down.")
    }

    pub fn settings(error_code: isize) -> Self {
        Self::new(
            "settings",
            None,
            format!("Could not open Bluetooth settings (error {error_code})."),
        )
    }
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.hresult {
            Some(code) => write!(formatter, "{} (0x{code:08X}): {}", self.code, self.message),
            None => write!(formatter, "{}: {}", self.code, self.message),
        }
    }
}

impl std::error::Error for CommandError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_failure_keeps_hresult() {
        let error = CommandError::unknown_failure(0x8007_001F_u32 as i32);
        assert_eq!(error.code, "unknownFailure");
        assert_eq!(error.hresult, Some(0x8007_001F));
    }

    #[test]
    fn zero_extended_error_is_omitted() {
        let error = CommandError::unknown_failure(0);
        assert_eq!(error.code, "unknownFailure");
        assert_eq!(error.hresult, None);
    }
}
