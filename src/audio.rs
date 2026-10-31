use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use windows::Devices::Enumeration::DeviceInformation;
use windows::Media::Audio::{
    AudioPlaybackConnection, AudioPlaybackConnectionOpenResultStatus, AudioPlaybackConnectionState,
};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
use windows::core::{HSTRING, RuntimeType};
use windows_future::{IAsyncAction, IAsyncOperation};

use crate::error::CommandError;

const START_TIMEOUT: Duration = Duration::from_secs(10);
const OPEN_TIMEOUT: Duration = Duration::from_secs(30);
const READY_TIMEOUT: Duration = Duration::from_secs(5);
const READY_POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Clone, Debug)]
pub struct DeviceDto {
    pub id: String,
    pub name: String,
    pub is_enabled: bool,
}

#[derive(Default)]
pub struct AppState {
    connections: Mutex<HashMap<String, AudioPlaybackConnection>>,
    shutting_down: AtomicBool,
}

impl AppState {
    pub fn close_all(&self) {
        self.shutting_down.store(true, Ordering::SeqCst);
        if let Ok(mut connections) = self.connections.lock() {
            for (_, connection) in connections.drain() {
                let _ = connection.Close();
            }
        }
    }
}

pub fn init_com() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

fn join_operation<T: RuntimeType + Send + 'static>(
    operation: IAsyncOperation<T>,
    timeout: Duration,
    operation_name: &str,
) -> Result<T, CommandError> {
    let (sender, receiver) = std::sync::mpsc::channel();
    let cancel = operation.clone();

    std::thread::spawn(move || {
        let _ = sender.send(operation.join().map_err(CommandError::windows));
    });

    match receiver.recv_timeout(timeout) {
        Ok(result) => result,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            let _ = cancel.Cancel();
            log::warn!("{operation_name} timed out after {timeout:?}");
            Err(CommandError::timeout(format!(
                "{operation_name} did not complete in time."
            )))
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(CommandError::new(
            "stateUnavailable",
            None,
            format!("{operation_name} worker stopped unexpectedly."),
        )),
    }
}

fn join_action(
    operation: IAsyncAction,
    timeout: Duration,
    operation_name: &str,
) -> Result<(), CommandError> {
    let (sender, receiver) = std::sync::mpsc::channel();
    let cancel = operation.clone();

    std::thread::spawn(move || {
        let _ = sender.send(operation.join().map_err(CommandError::windows));
    });

    match receiver.recv_timeout(timeout) {
        Ok(result) => result,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            let _ = cancel.Cancel();
            log::warn!("{operation_name} timed out after {timeout:?}");
            Err(CommandError::timeout(format!(
                "{operation_name} did not complete in time."
            )))
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(CommandError::new(
            "stateUnavailable",
            None,
            format!("{operation_name} worker stopped unexpectedly."),
        )),
    }
}

fn open_status_error(
    status: AudioPlaybackConnectionOpenResultStatus,
    extended_error: i32,
) -> CommandError {
    if status == AudioPlaybackConnectionOpenResultStatus::RequestTimedOut {
        CommandError::timeout("The device did not respond in time.")
    } else if status == AudioPlaybackConnectionOpenResultStatus::DeniedBySystem {
        CommandError::denied()
    } else if status == AudioPlaybackConnectionOpenResultStatus::UnknownFailure {
        CommandError::unknown_failure(extended_error)
    } else {
        CommandError::unknown_failure(extended_error)
    }
}

pub fn list_devices() -> Result<Vec<DeviceDto>, CommandError> {
    init_com();
    let selector = AudioPlaybackConnection::GetDeviceSelector().map_err(CommandError::windows)?;
    let operation =
        DeviceInformation::FindAllAsyncAqsFilter(&selector).map_err(CommandError::windows)?;
    let collection = join_operation(operation, Duration::from_secs(10), "Device enumeration")?;

    let mut devices = Vec::with_capacity(collection.Size().unwrap_or(0) as usize);
    for info in &collection {
        let id = info.Id().map_err(CommandError::windows)?.to_string();
        let name = info.Name().map_err(CommandError::windows)?.to_string();
        let is_enabled = info.IsEnabled().map_err(CommandError::windows)?;
        devices.push(DeviceDto {
            id,
            name,
            is_enabled,
        });
    }
    devices.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(devices)
}

pub fn open_connection(state: &AppState, device_id: &str) -> Result<(), CommandError> {
    if state.shutting_down.load(Ordering::SeqCst) {
        return Err(CommandError::shutting_down());
    }

    init_com();
    log::info!("Opening audio playback connection for {device_id}");
    let id = HSTRING::from(device_id);
    let connection =
        AudioPlaybackConnection::TryCreateFromId(&id).map_err(CommandError::unsupported)?;

    if let Err(error) = join_action(
        connection.StartAsync().map_err(CommandError::windows)?,
        START_TIMEOUT,
        "StartAsync",
    ) {
        let _ = connection.Close();
        return Err(error);
    }

    let result = match join_operation(
        connection.OpenAsync().map_err(CommandError::windows)?,
        OPEN_TIMEOUT,
        "OpenAsync",
    ) {
        Ok(result) => result,
        Err(error) => {
            let _ = connection.Close();
            return Err(error);
        }
    };

    let status = match result.Status() {
        Ok(status) => status,
        Err(error) => {
            let _ = connection.Close();
            return Err(CommandError::windows(error));
        }
    };
    if status != AudioPlaybackConnectionOpenResultStatus::Success {
        let extended_error = result.ExtendedError().map(|error| error.0).unwrap_or(0);
        let error = open_status_error(status, extended_error);
        let _ = connection.Close();
        return Err(error);
    }

    let deadline = Instant::now() + READY_TIMEOUT;
    loop {
        match connection.State() {
            Ok(AudioPlaybackConnectionState::Opened) => break,
            Ok(_) => {
                if Instant::now() >= deadline {
                    let _ = connection.Close();
                    return Err(CommandError::not_ready());
                }
                std::thread::sleep(READY_POLL_INTERVAL);
            }
            Err(error) => {
                let _ = connection.Close();
                return Err(CommandError::windows(error));
            }
        }
    }

    let mut connections = state
        .connections
        .lock()
        .map_err(|_| CommandError::state_unavailable())?;

    if state.shutting_down.load(Ordering::SeqCst) {
        drop(connections);
        let _ = connection.Close();
        return Err(CommandError::shutting_down());
    }

    if let Some(previous) = connections.insert(device_id.to_string(), connection) {
        let _ = previous.Close();
    }
    drop(connections);

    log::info!("Audio playback connection ready for {device_id}");
    Ok(())
}

pub fn close_connection(state: &AppState, device_id: &str) -> Result<(), CommandError> {
    let connection = {
        let mut connections = state
            .connections
            .lock()
            .map_err(|_| CommandError::state_unavailable())?;
        connections.remove(device_id)
    };

    if let Some(connection) = connection {
        init_com();
        connection.Close().map_err(CommandError::windows)?;
    }
    Ok(())
}

pub fn connection_is_open(state: &AppState, device_id: &str) -> Result<bool, CommandError> {
    let connections = state
        .connections
        .lock()
        .map_err(|_| CommandError::state_unavailable())?;
    init_com();
    match connections.get(device_id) {
        Some(connection) => Ok(connection.State().map_err(CommandError::windows)?
            == AudioPlaybackConnectionState::Opened),
        None => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_status_maps_to_stable_codes() {
        let timeout =
            open_status_error(AudioPlaybackConnectionOpenResultStatus::RequestTimedOut, 0);
        assert_eq!(timeout.code, "timeout");

        let denied = open_status_error(AudioPlaybackConnectionOpenResultStatus::DeniedBySystem, 0);
        assert_eq!(denied.code, "denied");

        let unknown = open_status_error(
            AudioPlaybackConnectionOpenResultStatus::UnknownFailure,
            0x8007_001F_u32 as i32,
        );
        assert_eq!(unknown.code, "unknownFailure");
        assert_eq!(unknown.hresult, Some(0x8007_001F));
    }
}
