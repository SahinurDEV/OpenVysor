use std::net::Ipv4Addr;
use std::thread;
use std::time::Duration;

use super::{Adb, DEFAULT_TIMEOUT};
use crate::error::{AppError, Result};

const WIRELESS_TIMEOUT: Duration = Duration::from_secs(20);
/// The port adb listens on after `adb tcpip`.
pub const TCPIP_PORT: u16 = 5555;
const CONNECT_ATTEMPTS: u32 = 10;
const CONNECT_RETRY_DELAY: Duration = Duration::from_secs(1);

/// Drops stray spaces, and adds adb's usual port when only a host was typed.
pub fn normalize_address(address: &str) -> String {
    let address: String = address.split_whitespace().collect();
    if address.contains(':') { address } else { format!("{address}:{TCPIP_PORT}") }
}

pub fn validate_address(address: &str) -> Result<()> {
    let invalid = || AppError::Invalid(format!("address {address}"));
    let (host, port) = address.rsplit_once(':').ok_or_else(invalid)?;
    let host_valid = !host.is_empty()
        && !host.starts_with('-')
        && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    let port: u16 = port.parse().map_err(|_| invalid())?;
    if host_valid && port != 0 { Ok(()) } else { Err(invalid()) }
}

fn validate_secret(value: &str) -> Result<()> {
    let valid = (6..=64).contains(&value.len())
        && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    if valid { Ok(()) } else { Err(AppError::Invalid("pairing details".into())) }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MdnsService {
    pub name: String,
    pub service: String,
    pub address: String,
}

/// Parses the output of `adb mdns services`.
pub fn parse_mdns_services(output: &str) -> Vec<MdnsService> {
    output
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let (name, service, address) = (parts.next()?, parts.next()?, parts.next()?);
            service.starts_with("_adb").then(|| MdnsService {
                name: name.to_string(),
                service: service.to_string(),
                address: address.to_string(),
            })
        })
        .collect()
}

/// `adb connect` exits successfully even when it fails, so the text decides.
pub fn check_connect_output(output: &str) -> Result<String> {
    let text = output.trim();
    if text.starts_with("connected to") || text.starts_with("already connected") {
        Ok(text.to_string())
    } else {
        Err(AppError::AdbFailed(text.to_string()))
    }
}

pub fn pair(adb: &Adb, address: &str, code: &str) -> Result<()> {
    let address: String = address.split_whitespace().collect();
    let code: String = code.split_whitespace().collect();
    validate_address(&address)?;
    validate_secret(&code)?;
    let output = adb.run_text(&["pair", &address, &code], WIRELESS_TIMEOUT)?;
    if output.contains("Successfully paired") {
        Ok(())
    } else {
        Err(AppError::AdbFailed(output.trim().to_string()))
    }
}

/// Connects and returns the address used, which gains port 5555 if none was given.
pub fn connect(adb: &Adb, address: &str) -> Result<String> {
    let address = normalize_address(address);
    validate_address(&address)?;
    check_connect_output(&adb.run_text(&["connect", &address], WIRELESS_TIMEOUT)?)?;
    Ok(address)
}

pub fn disconnect(adb: &Adb, serial: &str) -> Result<()> {
    adb.run_text(&["disconnect", serial], WIRELESS_TIMEOUT).map(|_| ())
}

fn host_of(address: &str) -> &str {
    address.rsplit_once(':').map_or(address, |(host, _)| host)
}

/// Phones on the network that accept a connection: paired phones with Wireless
/// debugging on, and phones switched to Wi-Fi with `adb tcpip`.
pub fn discover(adb: &Adb) -> Result<Vec<MdnsService>> {
    let services = parse_mdns_services(&adb.run_text(&["mdns", "services"], WIRELESS_TIMEOUT)?);
    Ok(services
        .into_iter()
        .filter(|entry| entry.service.contains("adb-tls-connect") || entry.service.starts_with("_adb._tcp"))
        .filter(|entry| validate_address(&entry.address).is_ok())
        .collect())
}

/// The connect port differs from the pairing port, so it is found on the network.
fn connect_paired_host(adb: &Adb, host: &str, services: &[MdnsService]) -> Option<String> {
    let entry = services
        .iter()
        .find(|entry| entry.service.contains("adb-tls-connect") && host_of(&entry.address) == host)?;
    connect(adb, &entry.address).inspect_err(|error| log::warn!("connect after pairing: {error}")).ok()
}

/// Pairs, then connects without asking for the second port. Returns the connected
/// address, or None when the phone did not announce itself in time; paired phones
/// usually reconnect by themselves soon after.
pub fn pair_and_connect(adb: &Adb, address: &str, code: &str) -> Result<Option<String>> {
    pair(adb, address, code)?;
    let address: String = address.split_whitespace().collect();
    let host = host_of(&address);
    for _ in 0..CONNECT_ATTEMPTS / 2 {
        let services = parse_mdns_services(&adb.run_text(&["mdns", "services"], WIRELESS_TIMEOUT)?);
        if let Some(connected) = connect_paired_host(adb, host, &services) {
            return Ok(Some(connected));
        }
        thread::sleep(CONNECT_RETRY_DELAY);
    }
    Ok(None)
}

/// Checks whether the phone that scanned the QR code has appeared, and if so
/// pairs with it. Returns false while it is still waiting for the scan.
pub fn poll_qr_pairing(adb: &Adb, name: &str, password: &str) -> Result<bool> {
    validate_secret(name)?;
    validate_secret(password)?;
    let services = parse_mdns_services(&adb.run_text(&["mdns", "services"], WIRELESS_TIMEOUT)?);
    let Some(pairing) = services
        .iter()
        .find(|entry| entry.name == name && entry.service.contains("adb-tls-pairing"))
    else {
        return Ok(false);
    };
    pair(adb, &pairing.address, password)?;
    // Paired phones usually reconnect by themselves, so a miss here is not an error.
    connect_paired_host(adb, host_of(&pairing.address), &services);
    Ok(true)
}

/// Finds the phone's Wi-Fi address in the output of `ip -f inet -o addr show`.
pub fn parse_wifi_ip(output: &str) -> Option<Ipv4Addr> {
    output
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let interface = parts.nth(1)?.trim_end_matches(':');
            parts.find(|part| *part == "inet")?;
            let ip: Ipv4Addr = parts.next()?.split('/').next()?.parse().ok()?;
            interface.starts_with("wlan").then_some(ip)
        })
        .find(|ip| !ip.is_loopback() && !ip.is_link_local())
}

/// Moves a USB-connected phone to Wi-Fi with `adb tcpip`, which works on every
/// Android version. Returns the address it connected to.
pub fn switch_to_wifi(adb: &Adb, serial: &str) -> Result<String> {
    let addresses = adb.device_text(serial, &["shell", "ip", "-f", "inet", "-o", "addr", "show"], DEFAULT_TIMEOUT)?;
    let ip = parse_wifi_ip(&addresses).ok_or_else(|| {
        AppError::Wireless("The phone is not on Wi-Fi. Join the same Wi-Fi as this computer, then try again.".into())
    })?;
    adb.device_text(serial, &["tcpip", &TCPIP_PORT.to_string()], WIRELESS_TIMEOUT)?;
    let address = format!("{ip}:{TCPIP_PORT}");
    // The phone restarts adb before it listens on the new port.
    for _ in 0..CONNECT_ATTEMPTS {
        thread::sleep(CONNECT_RETRY_DELAY);
        match connect(adb, &address) {
            Ok(address) => return Ok(address),
            Err(error) => log::info!("connect to {address}: {error}"),
        }
    }
    Err(AppError::Wireless(format!(
        "Could not reach the phone at {address}. Check that the phone and this computer are on the same Wi-Fi."
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_addresses() {
        assert!(validate_address("192.168.1.20:5555").is_ok());
        assert!(validate_address("phone.local:37000").is_ok());
        for bad in ["", "192.168.1.20", ":5555", "host:0", "host:99999", "-s:5555", "a b:5555", "a;b:1"] {
            assert!(validate_address(bad).is_err(), "{bad} was accepted");
        }
    }

    #[test]
    fn validates_pairing_secrets() {
        assert!(validate_secret("123456").is_ok());
        assert!(validate_secret("studio-abc123").is_ok());
        assert!(validate_secret("12345").is_err());
        assert!(validate_secret("abc def").is_err());
    }

    #[test]
    fn parses_mdns_services() {
        let output = "List of discovered mdns services\n\
adb-R58-abc\t_adb-tls-connect._tcp\t192.168.1.5:37011\n\
studio-x1y2z3\t_adb-tls-pairing._tcp\t192.168.1.5:41022\n";
        let services = parse_mdns_services(output);
        assert_eq!(services.len(), 2);
        assert_eq!(services[1].name, "studio-x1y2z3");
        assert_eq!(services[1].address, "192.168.1.5:41022");
    }

    #[test]
    fn adds_the_default_port_and_drops_spaces() {
        assert_eq!(normalize_address(" 192.168.1.20 "), "192.168.1.20:5555");
        assert_eq!(normalize_address("192.168.1.20 : 41234"), "192.168.1.20:41234");
        assert!(validate_address(&normalize_address("phone.local")).is_ok());
    }

    #[test]
    fn finds_the_wifi_address() {
        let output = "1: lo    inet 127.0.0.1/8 scope host lo\\       valid_lft forever preferred_lft forever\n\
21: rmnet_data0    inet 10.120.4.7/30 scope global rmnet_data0\\       valid_lft forever\n\
30: wlan0    inet 192.168.0.105/24 brd 192.168.0.255 scope global wlan0\\       valid_lft forever\n";
        assert_eq!(parse_wifi_ip(output), Some(Ipv4Addr::new(192, 168, 0, 105)));
    }

    #[test]
    fn mobile_data_alone_is_not_wifi() {
        let output = "1: lo    inet 127.0.0.1/8 scope host lo\n21: rmnet_data0    inet 10.120.4.7/30 scope global rmnet_data0\n";
        assert_eq!(parse_wifi_ip(output), None);
        assert_eq!(parse_wifi_ip(""), None);
    }

    #[test]
    fn detects_connect_failure_despite_exit_code() {
        assert!(check_connect_output("connected to 10.0.0.2:5555\n").is_ok());
        assert!(check_connect_output("already connected to 10.0.0.2:5555\n").is_ok());
        assert!(check_connect_output("failed to connect to 10.0.0.2:5555\n").is_err());
        assert!(check_connect_output("cannot connect to 10.0.0.2:5555: refused\n").is_err());
    }
}
