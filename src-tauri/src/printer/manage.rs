//! Adding, editing and removing printers in the operating system. Only the
//! ix.print window can call this (no HTTP endpoint), and the OS decides whether
//! the user may: CUPS needs an admin / lpadmin-group user, Windows usually
//! needs ix.print to run as administrator.

use serde::{Deserialize, Serialize};

#[cfg(not(windows))]
use super::manage_cups as os;
#[cfg(windows)]
use super::manage_windows as os;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum NetProtocol {
    /// IPP on port 631 — works driverless with most modern printers.
    Ipp,
    /// Raw TCP (JetDirect / AppSocket), usually port 9100.
    Raw,
    Lpd,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Connection {
    /// A printer reachable at an IP address or hostname.
    Network {
        host: String,
        protocol: NetProtocol,
        #[serde(default)]
        port: Option<u16>,
        /// IPP resource path or LPD queue name.
        #[serde(default)]
        queue: Option<String>,
    },
    /// A detected device (CUPS device URI / Windows port name) or a manual URI.
    Device { uri: String },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NewPrinter {
    pub name: String,
    pub connection: Connection,
    /// CUPS model / Windows driver name; `None` = driverless (IPP Everywhere).
    #[serde(default)]
    pub driver: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub shared: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PrinterEdit {
    /// New name (Windows only; CUPS cannot rename a queue).
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    pub shared: bool,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct PrinterDetails {
    pub name: String,
    pub description: Option<String>,
    pub location: Option<String>,
    /// Device URI (CUPS) or port name (Windows).
    pub connection: Option<String>,
    pub driver: Option<String>,
    pub shared: bool,
    /// Whether this OS can rename printers.
    pub can_rename: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Device {
    /// Value for `Connection::Device`.
    pub uri: String,
    /// "usb", "network", "port", ...
    pub kind: String,
    pub info: String,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Driver {
    /// Value for `NewPrinter::driver`.
    pub id: String,
    pub name: String,
}

pub fn devices() -> Result<Vec<Device>, String> {
    os::devices()
}

/// Installed drivers whose name contains `query` (case-insensitive), at most 200.
pub fn drivers(query: &str) -> Result<Vec<Driver>, String> {
    let q = query.trim().to_lowercase();
    let mut list: Vec<Driver> = os::drivers()?
        .into_iter()
        .filter(|d| q.is_empty() || d.name.to_lowercase().contains(&q) || d.id.to_lowercase().contains(&q))
        .collect();
    list.truncate(200);
    Ok(list)
}

pub fn details(name: &str) -> Result<PrinterDetails, String> {
    os::details(name)
}

pub fn add(printer: &NewPrinter) -> Result<(), String> {
    validate_name(&printer.name)?;
    validate_text(&printer.description)?;
    validate_text(&printer.location)?;
    validate_connection(&printer.connection)?;
    if let Some(d) = &printer.driver {
        if d.trim().is_empty() || d.chars().any(char::is_control) {
            return Err("Invalid driver".into());
        }
    }
    logged("add", &printer.name, os::add(printer))?;
    if !installed(&printer.name) {
        return Err(format!(
            "The add command completed, but printer '{}' did not appear in the system printer list.",
            printer.name
        ));
    }
    Ok(())
}

pub fn update(name: &str, edit: &PrinterEdit) -> Result<(), String> {
    validate_name(&edit.name)?;
    validate_text(&edit.description)?;
    validate_text(&edit.location)?;
    logged("update", name, os::update(name, edit))?;
    if !installed(&edit.name) {
        return Err(format!("Printer '{}' was not found after being updated.", edit.name));
    }
    Ok(())
}

pub fn remove(name: &str) -> Result<(), String> {
    if !installed(name) {
        return Err(format!("Printer '{}' is not installed on the system.", name));
    }
    logged("remove", name, os::remove(name))?;
    // Trust the OS list, not the command's exit code.
    if installed(name) {
        return Err(format!(
            "The remove command completed, but printer '{}' is still listed by the system. \
			 Try removing it from the operating system's printer settings.",
            name
        ));
    }
    Ok(())
}

fn installed(name: &str) -> bool {
    super::list_printers().iter().any(|p| p.name == name)
}

/// Logs a management action to stderr (visible in the `tauri dev` terminal)
/// and turns permission failures into an actionable message.
fn logged(action: &str, name: &str, result: Result<(), String>) -> Result<(), String> {
    match &result {
        Ok(()) => eprintln!("[ix.print] printer {} '{}': ok", action, name),
        Err(e) => eprintln!("[ix.print] printer {} '{}' failed: {}", action, name, e),
    }
    result.map_err(explain)
}

pub fn set_os_default(name: &str) -> Result<(), String> {
    logged("set as default", name, os::set_os_default(name))
}

/// Turns permission failures into an actionable message.
fn explain(e: String) -> String {
    let lower = e.to_lowercase();
    if [
        "forbidden",
        "not authorized",
        "access is denied",
        "akses ditolak",
        "permission",
        "unauthorized",
        "0x00000005",
    ]
    .iter()
    .any(|k| lower.contains(k))
    {
        let how = if cfg!(windows) {
            "run ix.print with \"Run as administrator\""
        } else {
            "use an administrator account (Linux: member of the lpadmin group)"
        };
        format!(
            "Administrator rights are required to change the printer: {}. ({})",
            how,
            e.trim()
        )
    } else {
        e
    }
}

pub fn validate_name(name: &str) -> Result<(), String> {
    let name_ok = !name.trim().is_empty()
        && name.trim() == name
        && name.chars().count() <= 127
        && name.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ' '))
        // CUPS queue names cannot contain spaces.
        && (cfg!(windows) || !name.contains(' '));
    if name_ok {
        Ok(())
    } else if cfg!(windows) {
        Err("Printer name: letters, numbers, spaces, _ - . (max. 127)".into())
    } else {
        Err("Printer name: letters, numbers, _ - . without spaces (max. 127)".into())
    }
}

fn validate_text(v: &Option<String>) -> Result<(), String> {
    match v {
        Some(t) if t.chars().count() > 200 || t.chars().any(char::is_control) => {
            Err("Description/location must be at most 200 characters with no line breaks".into())
        }
        _ => Ok(()),
    }
}

fn valid_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']'))
}

fn validate_connection(c: &Connection) -> Result<(), String> {
    match c {
        Connection::Network { host, queue, .. } => {
            if !valid_host(host) {
                return Err("Printer address is invalid (IP or hostname)".into());
            }
            if let Some(q) = queue {
                if q.chars()
                    .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '?' | '#'))
                {
                    return Err("Queue/path name is invalid".into());
                }
            }
            Ok(())
        }
        Connection::Device { uri } => {
            if uri.trim().is_empty() || uri.chars().any(|c| c.is_whitespace() || c.is_control()) {
                Err("Device URI/port is invalid".into())
            } else {
                Ok(())
            }
        }
    }
}

/// Device URI for a network connection, as CUPS writes it.
pub fn network_uri(host: &str, protocol: NetProtocol, port: Option<u16>, queue: Option<&str>) -> String {
    let queue = queue.map(|q| q.trim_start_matches('/')).filter(|q| !q.is_empty());
    match protocol {
        NetProtocol::Ipp => format!(
            "ipp://{}:{}/{}",
            host,
            port.unwrap_or(631),
            queue.unwrap_or("ipp/print")
        ),
        NetProtocol::Raw => format!("socket://{}:{}", host, port.unwrap_or(9100)),
        NetProtocol::Lpd => format!("lpd://{}/{}", host, queue.unwrap_or("queue")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation() {
        assert!(validate_name("Kasir_1").is_ok());
        assert!(validate_name("").is_err());
        assert!(validate_name("a/b").is_err());
        assert!(validate_name(" x").is_err());
        assert_eq!(validate_name("Printer Kasir").is_ok(), cfg!(windows));
        let net = |host: &str| Connection::Network {
            host: host.into(),
            protocol: NetProtocol::Ipp,
            port: None,
            queue: None,
        };
        assert!(validate_connection(&net("192.168.1.20")).is_ok());
        assert!(validate_connection(&net("printer.local")).is_ok());
        assert!(validate_connection(&net("1.2.3.4/../x")).is_err());
        assert!(validate_connection(&net("a;rm -rf")).is_err());
        assert!(validate_connection(&Connection::Device {
            uri: "usb://Zebra/ZD230?serial=1".into()
        })
        .is_ok());
        assert!(validate_connection(&Connection::Device {
            uri: "usb://x y".into()
        })
        .is_err());
    }

    #[test]
    fn uris() {
        assert_eq!(
            network_uri("10.0.0.5", NetProtocol::Ipp, None, None),
            "ipp://10.0.0.5:631/ipp/print"
        );
        assert_eq!(
            network_uri("10.0.0.5", NetProtocol::Raw, None, None),
            "socket://10.0.0.5:9100"
        );
        assert_eq!(
            network_uri("10.0.0.5", NetProtocol::Lpd, None, Some("/lp1")),
            "lpd://10.0.0.5/lp1"
        );
    }

    #[test]
    fn explains_permission_errors() {
        assert!(explain("lpadmin: Forbidden".into()).contains("Administrator rights"));
        assert_eq!(explain("lpadmin: Bad device-uri".into()), "lpadmin: Bad device-uri");
    }
}

#[cfg(test)]
mod live_probe {
    /// Read-only: `cargo test --lib manage_live -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn manage_live() {
        for d in super::devices().unwrap() {
            println!("device {:?}", d);
        }
        println!("drivers(zebra) = {:?}", super::drivers("zebra").unwrap());
        for p in crate::printer::list_printers().into_iter().take(3) {
            println!("details {:?}", super::details(&p.name));
        }
    }
}
