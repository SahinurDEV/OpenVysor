use std::time::Duration;

use super::Adb;
use crate::error::{AppError, Result};

const WIRELESS_TIMEOUT: Duration = Duration::from_secs(20);

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

#[derive(Debug, PartialEq, Eq)]
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
    validate_address(address)?;
    validate_secret(code)?;
    let output = adb.run_text(&["pair", address, code], WIRELESS_TIMEOUT)?;
    if output.contains("Successfully paired") {
        Ok(())
    } else {
        Err(AppError::AdbFailed(output.trim().to_string()))
    }
}

pub fn connect(adb: &Adb, address: &str) -> Result<String> {
    validate_address(address)?;
    check_connect_output(&adb.run_text(&["connect", address], WIRELESS_TIMEOUT)?)
}

pub fn disconnect(adb: &Adb, serial: &str) -> Result<()> {
    adb.run_text(&["disconnect", serial], WIRELESS_TIMEOUT).map(|_| ())
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

    let host = pairing.address.rsplit_once(':').map_or("", |(host, _)| host);
    let connectable = services.iter().find(|entry| {
        entry.service.contains("adb-tls-connect") && entry.address.rsplit_once(':').is_some_and(|(h, _)| h == host)
    });
    // Paired phones usually reconnect by themselves, so a miss here is not an error.
    if let Some(entry) = connectable {
        if let Err(error) = connect(adb, &entry.address) {
            log::warn!("connect after pairing: {error}");
        }
    }
    Ok(true)
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
    fn detects_connect_failure_despite_exit_code() {
        assert!(check_connect_output("connected to 10.0.0.2:5555\n").is_ok());
        assert!(check_connect_output("already connected to 10.0.0.2:5555\n").is_ok());
        assert!(check_connect_output("failed to connect to 10.0.0.2:5555\n").is_err());
        assert!(check_connect_output("cannot connect to 10.0.0.2:5555: refused\n").is_err());
    }
}
