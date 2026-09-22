//! Printer management on macOS / Linux via CUPS tools (`lpinfo`, `lpadmin`,
//! `lpoptions`). Arguments are passed directly, never through a shell.

use super::manage::{network_uri, Connection, Device, Driver, NewPrinter, PrinterDetails, PrinterEdit};
use std::collections::HashMap;
use std::process::Command;

fn run(program: &str, args: &[String]) -> Result<String, String> {
    let out = Command::new(program)
        .args(args)
        .env("LC_ALL", "C")
        .output()
		.map_err(|e| format!("Failed to run {}: {}", program, e))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
		Err(if err.is_empty() { format!("{} failed", program) } else { err })
    }
}

fn s(v: &str) -> String {
    v.to_string()
}

/// Parses `lpinfo -l -v` blocks into devices; backend-only entries (no `://`) are skipped.
fn parse_devices(text: &str) -> Vec<Device> {
    let mut out = Vec::new();
    let mut current: Option<(String, String, String)> = None; // uri, class, info
    let flush = |c: Option<(String, String, String)>, out: &mut Vec<Device>| {
        if let Some((uri, class, info)) = c {
            if uri.contains("://") {
                let scheme = uri.split("://").next().unwrap_or_default();
                let kind = if scheme == "usb" || class == "direct" && !scheme.contains("net") {
                    "usb"
                } else {
                    "network"
                };
                out.push(Device { info: if info.is_empty() { uri.clone() } else { info }, uri, kind: kind.into() });
            }
        }
    };
    for line in text.lines() {
        let line = line.trim();
        if let Some(uri) = line.strip_prefix("Device: uri = ") {
            flush(current.take(), &mut out);
            current = Some((uri.trim().to_string(), String::new(), String::new()));
        } else if let Some(c) = current.as_mut() {
            if let Some(v) = line.strip_prefix("class = ") {
                c.1 = v.trim().to_string();
            } else if let Some(v) = line.strip_prefix("info = ") {
                c.2 = v.trim().to_string();
            }
        }
    }
    flush(current, &mut out);
    out
}

pub fn devices() -> Result<Vec<Device>, String> {
    run("lpinfo", &[s("-l"), s("-v"), s("--timeout"), s("8")]).map(|t| parse_devices(&t))
}

/// `lpinfo -m` lines: `<model id> <description>`.
fn parse_drivers(text: &str) -> Vec<Driver> {
    text.lines()
        .filter_map(|l| {
            let (id, name) = l.trim().split_once(' ')?;
            Some(Driver { id: id.to_string(), name: name.trim().to_string() })
        })
        .collect()
}

pub fn drivers() -> Result<Vec<Driver>, String> {
    run("lpinfo", &[s("-m")]).map(|t| parse_drivers(&t))
}

/// Parses `lpoptions -p` output: `key=value key='value with spaces' key="…"`.
fn parse_options(text: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut chars = text.trim().chars().peekable();
    loop {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        let key: String = std::iter::from_fn(|| chars.next_if(|c| *c != '=' && !c.is_whitespace())).collect();
        if key.is_empty() {
            break;
        }
        let mut value = String::new();
        if chars.next_if_eq(&'=').is_some() {
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() {
                    break;
                }
                chars.next();
                match c {
                    '\'' | '"' => {
                        for q in chars.by_ref() {
                            if q == c {
                                break;
                            }
                            value.push(q);
                        }
                    }
                    '\\' => {
                        if let Some(n) = chars.next() {
                            value.push(n);
                        }
                    }
                    _ => value.push(c),
                }
            }
        }
        out.insert(key, value);
    }
    out
}

pub fn details(name: &str) -> Result<PrinterDetails, String> {
    let opts = parse_options(&run("lpoptions", &[s("-p"), s(name)])?);
    let get = |k: &str| opts.get(k).cloned().filter(|v| !v.is_empty());
    Ok(PrinterDetails {
        name: name.to_string(),
        description: get("printer-info"),
        location: get("printer-location"),
        connection: get("device-uri"),
        driver: get("printer-make-and-model"),
        shared: get("printer-is-shared").as_deref() == Some("true"),
        can_rename: false,
    })
}

/// `lpadmin` arguments for a new printer.
pub(crate) fn add_args(p: &NewPrinter) -> Vec<String> {
    let uri = match &p.connection {
        Connection::Network { host, protocol, port, queue } => network_uri(host, *protocol, *port, queue.as_deref()),
        Connection::Device { uri } => uri.clone(),
    };
    let mut args = vec![
        s("-p"),
        p.name.clone(),
        s("-E"),
        s("-v"),
        uri,
        s("-m"),
        p.driver.clone().unwrap_or_else(|| s("everywhere")),
        s("-o"),
        format!("printer-is-shared={}", p.shared),
    ];
    if let Some(d) = p.description.as_ref().filter(|d| !d.is_empty()) {
        args.extend([s("-D"), d.clone()]);
    }
    if let Some(l) = p.location.as_ref().filter(|l| !l.is_empty()) {
        args.extend([s("-L"), l.clone()]);
    }
    args
}

pub fn add(p: &NewPrinter) -> Result<(), String> {
    run("lpadmin", &add_args(p)).map(|_| ())
}

pub(crate) fn update_args(name: &str, e: &PrinterEdit) -> Vec<String> {
    vec![
        s("-p"),
        name.to_string(),
        s("-D"),
        e.description.clone().unwrap_or_default(),
        s("-L"),
        e.location.clone().unwrap_or_default(),
        s("-o"),
        format!("printer-is-shared={}", e.shared),
    ]
}

pub fn update(name: &str, e: &PrinterEdit) -> Result<(), String> {
    if e.name != name {
		return Err("CUPS cannot rename a printer. Remove it and add it again with a new name.".into());
    }
    run("lpadmin", &update_args(name, e)).map(|_| ())
}

pub fn remove(name: &str) -> Result<(), String> {
    run("lpadmin", &[s("-x"), s(name)]).map(|_| ())
}

/// The user's default printer (what `lpstat -d` reports); no admin needed.
pub fn set_os_default(name: &str) -> Result<(), String> {
    run("lpoptions", &[s("-d"), s(name)]).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::printer::manage::NetProtocol;

    #[test]
    fn parses_lpinfo_and_lpoptions() {
        let text = "Device: uri = usb://Zebra%20Technologies/ZTC?serial=1\n        class = direct\n        info = Zebra ZD230\nDevice: uri = smb\n        class = network\n        info = Windows printer via spoolss\nDevice: uri = dnssd://HP._ipp._tcp.local/\n        class = network\n        info = HP LaserJet\n";
        let d = parse_devices(text);
        assert_eq!(d.len(), 2);
        assert_eq!((d[0].kind.as_str(), d[0].info.as_str()), ("usb", "Zebra ZD230"));
        assert_eq!(d[1].kind, "network");

        let o = parse_options("device-uri=usb://EPSON/LX-310?serial=0 printer-info='EPSON LX-310' printer-location=\"Ruang 1\" printer-is-shared=false x=a\\ b");
        assert_eq!(o["printer-info"], "EPSON LX-310");
        assert_eq!(o["printer-location"], "Ruang 1");
        assert_eq!(o["device-uri"], "usb://EPSON/LX-310?serial=0");
        assert_eq!(o["x"], "a b");

        let drivers = parse_drivers("drv:///sample.drv/zebra.ppd Zebra ZPL Label Printer\neverywhere IPP Everywhere\n");
        assert_eq!(drivers[0], Driver { id: "drv:///sample.drv/zebra.ppd".into(), name: "Zebra ZPL Label Printer".into() });
    }

    #[test]
    fn lpadmin_arguments() {
        let p = NewPrinter {
            name: "Kasir".into(),
            connection: Connection::Network { host: "10.0.0.9".into(), protocol: NetProtocol::Raw, port: None, queue: None },
            driver: Some("drv:///sample.drv/zebra.ppd".into()),
            description: Some("Printer kasir; rm -rf /".into()),
            location: None,
            shared: false,
        };
        let args = add_args(&p);
        assert_eq!(args[..7], ["-p", "Kasir", "-E", "-v", "socket://10.0.0.9:9100", "-m", "drv:///sample.drv/zebra.ppd"]);
        // Passed as one argument, never interpreted by a shell.
        assert!(args.contains(&"Printer kasir; rm -rf /".to_string()));
        let driverless = NewPrinter { driver: None, ..p };
        assert!(add_args(&driverless).windows(2).any(|w| w == ["-m", "everywhere"]));
    }
}
