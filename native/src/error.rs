use serde::ser::{Serialize, SerializeStruct, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("ADB was not found on this computer")]
    AdbNotFound,
    #[error("adb timed out running: {0}")]
    Timeout(String),
    #[error("adb failed: {0}")]
    AdbFailed(String),
    #[error("Device {0} is not available")]
    UnknownDevice(String),
    #[error("Device {0} is not ready. Check that USB debugging is authorized")]
    DeviceNotReady(String),
    #[error("A mirroring session is already running for {0}")]
    SessionExists(String),
    #[error("No mirroring session for {0}")]
    NoSession(String),
    #[error("The mirroring server failed to start: {0}")]
    ServerStart(String),
    #[error("Mirroring protocol error: {0}")]
    Protocol(String),
    #[error("Invalid input: {0}")]
    Invalid(String),
    #[error("{0}")]
    AdbSetup(String),
    #[error("{0}")]
    Wireless(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl AppError {
    fn code(&self) -> &'static str {
        match self {
            AppError::AdbNotFound => "adb_not_found",
            AppError::Timeout(_) => "timeout",
            AppError::AdbFailed(_) => "adb_failed",
            AppError::UnknownDevice(_) => "unknown_device",
            AppError::DeviceNotReady(_) => "device_not_ready",
            AppError::SessionExists(_) => "session_exists",
            AppError::NoSession(_) => "no_session",
            AppError::ServerStart(_) => "server_start",
            AppError::Protocol(_) => "protocol",
            AppError::Invalid(_) => "invalid",
            AppError::AdbSetup(_) => "adb_setup",
            AppError::Wireless(_) => "wireless",
            AppError::Io(_) => "io",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("AppError", 2)?;
        state.serialize_field("code", self.code())?;
        state.serialize_field("message", &self.to_string())?;
        state.end()
    }
}

pub type Result<T> = std::result::Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_code_and_message() {
        let value = serde_json::to_value(AppError::NoSession("abc".into())).unwrap();
        assert_eq!(value["code"], "no_session");
        assert_eq!(value["message"], "No mirroring session for abc");
    }
}
