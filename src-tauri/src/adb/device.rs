use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DeviceState {
    Online,
    Offline,
    Unauthorized,
    Authorizing,
    Connecting,
    NoPermissions,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ConnectionType {
    Usb,
    Wireless,
    Emulator,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceDetails {
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub android_version: Option<String>,
    pub sdk: Option<String>,
    pub resolution: Option<String>,
    pub battery: Option<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub serial: String,
    pub state: DeviceState,
    pub connection: ConnectionType,
    pub model: Option<String>,
    pub details: Option<DeviceDetails>,
}

pub fn connection_type(serial: &str) -> ConnectionType {
    if serial.starts_with("emulator-") {
        ConnectionType::Emulator
    } else if serial.contains(':') || serial.contains("._adb-tls-connect") {
        ConnectionType::Wireless
    } else {
        ConnectionType::Usb
    }
}

fn parse_state(state: &str) -> DeviceState {
    match state {
        "device" => DeviceState::Online,
        "offline" => DeviceState::Offline,
        "unauthorized" => DeviceState::Unauthorized,
        "authorizing" => DeviceState::Authorizing,
        "connecting" => DeviceState::Connecting,
        "no" => DeviceState::NoPermissions,
        _ => DeviceState::Other,
    }
}

/// Parses the output of `adb devices -l`.
pub fn parse_devices(output: &str) -> Vec<Device> {
    output
        .lines()
        .skip_while(|line| !line.starts_with("List of devices"))
        .skip(1)
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let serial = parts.next()?;
            let state = parts.next()?;
            let model = parts
                .find_map(|part| part.strip_prefix("model:"))
                .map(|model| model.replace('_', " "));
            Some(Device {
                serial: serial.to_string(),
                state: parse_state(state),
                connection: connection_type(serial),
                model,
                details: None,
            })
        })
        .collect()
}

/// Reads one property from the `[key]: [value]` output of `adb shell getprop`.
pub fn getprop_value(output: &str, key: &str) -> Option<String> {
    let prefix = format!("[{key}]: [");
    output.lines().find_map(|line| {
        let value = line.trim().strip_prefix(&prefix)?.strip_suffix(']')?;
        (!value.is_empty()).then(|| value.to_string())
    })
}

/// Parses `adb shell wm size`, preferring an override over the physical size.
pub fn parse_wm_size(output: &str) -> Option<String> {
    let find = |label: &str| {
        output
            .lines()
            .find_map(|line| line.trim().strip_prefix(label))
            .map(|value| value.trim().to_string())
    };
    find("Override size:").or_else(|| find("Physical size:"))
}

/// Parses the battery level from `adb shell dumpsys battery`.
pub fn parse_battery(output: &str) -> Option<u8> {
    output
        .lines()
        .find_map(|line| line.trim().strip_prefix("level:"))
        .and_then(|value| value.trim().parse().ok())
        .filter(|level| *level <= 100)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEVICES: &str = "* daemon started successfully\n\
List of devices attached\n\
FAKESERIAL00001        device product:AsteroidsIND model:A059 device:Asteroids transport_id:5\n\
192.168.1.20:5555      offline transport_id:6\n\
R58M12345              unauthorized usb:1-2 transport_id:7\n\
adb-ABC-xyz._adb-tls-connect._tcp device product:p model:Pixel_8 device:d transport_id:8\n\
emulator-5554          device product:sdk model:sdk_phone device:emu transport_id:9\n\
0123456789             no permissions (missing udev rules); see [http://x] usb:1-1 transport_id:1\n\n";

    #[test]
    fn parses_every_state_and_connection() {
        let devices = parse_devices(DEVICES);
        assert_eq!(devices.len(), 6);
        assert_eq!(devices[0].serial, "FAKESERIAL00001");
        assert_eq!(devices[0].state, DeviceState::Online);
        assert_eq!(devices[0].connection, ConnectionType::Usb);
        assert_eq!(devices[0].model.as_deref(), Some("A059"));
        assert_eq!(devices[1].state, DeviceState::Offline);
        assert_eq!(devices[1].connection, ConnectionType::Wireless);
        assert_eq!(devices[2].state, DeviceState::Unauthorized);
        assert_eq!(devices[3].connection, ConnectionType::Wireless);
        assert_eq!(devices[3].model.as_deref(), Some("Pixel 8"));
        assert_eq!(devices[4].connection, ConnectionType::Emulator);
        assert_eq!(devices[5].state, DeviceState::NoPermissions);
    }

    #[test]
    fn empty_list_has_no_devices() {
        assert!(parse_devices("List of devices attached\n\n").is_empty());
        assert!(parse_devices("").is_empty());
    }

    #[test]
    fn reads_getprop_values() {
        let props = "[ro.product.model]: [A059]\r\n[ro.empty]: []\r\n";
        assert_eq!(getprop_value(props, "ro.product.model").as_deref(), Some("A059"));
        assert_eq!(getprop_value(props, "ro.empty"), None);
        assert_eq!(getprop_value(props, "ro.missing"), None);
    }

    #[test]
    fn prefers_override_size() {
        assert_eq!(parse_wm_size("Physical size: 1080x2392\n").as_deref(), Some("1080x2392"));
        assert_eq!(
            parse_wm_size("Physical size: 1080x2392\nOverride size: 720x1600\n").as_deref(),
            Some("720x1600")
        );
    }

    #[test]
    fn parses_battery_level() {
        assert_eq!(parse_battery("  AC powered: false\n  level: 87\n  scale: 100\n"), Some(87));
        assert_eq!(parse_battery("level: 900\n"), None);
        assert_eq!(parse_battery("nothing here"), None);
    }
}
