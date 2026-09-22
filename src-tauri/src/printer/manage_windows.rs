//! Printer management on Windows via the PrintManagement PowerShell module.
//! Scripts are fixed text; user values only travel as `FXP_*` environment
//! variables, so a printer name can never become PowerShell code.

use super::manage::{Connection, Device, Driver, NetProtocol, NewPrinter, PrinterDetails, PrinterEdit};
use base64::Engine;
use std::os::windows::process::CommandExt;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Runs `script` with `env` as `$env:NAME`; returns stdout, or the error text.
fn ps(script: &str, env: &[(&str, String)]) -> Result<String, String> {
    let full = format!(
        "$ErrorActionPreference = 'Stop'; $ProgressPreference = 'SilentlyContinue'; \
         [Console]::OutputEncoding = [Text.Encoding]::UTF8; \
         try {{ {} }} catch {{ [Console]::Error.WriteLine($_.Exception.Message); exit 1 }}",
        script
    );
    let utf16: Vec<u8> = full.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    let encoded = base64::engine::general_purpose::STANDARD.encode(utf16);
    let mut cmd = Command::new("powershell.exe");
    cmd.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-EncodedCommand", &encoded])
        .creation_flags(CREATE_NO_WINDOW);
    for (k, v) in env {
        cmd.env(format!("FXP_{}", k), v);
    }
	let out = cmd.output().map_err(|e| format!("Failed to run PowerShell: {}", e))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
		Err(if err.is_empty() { "PowerShell failed".into() } else { err })
    }
}

/// ConvertTo-Json yields an object for one item and an array for several.
fn json_list(text: &str) -> Result<Vec<serde_json::Value>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(vec![]);
    }
    match serde_json::from_str::<serde_json::Value>(text).map_err(|e| e.to_string())? {
        serde_json::Value::Array(a) => Ok(a),
        v => Ok(vec![v]),
    }
}

fn str_of(v: &serde_json::Value, key: &str) -> Option<String> {
    v.get(key).and_then(|x| x.as_str()).map(str::to_string).filter(|s| !s.is_empty())
}

pub fn devices() -> Result<Vec<Device>, String> {
    let out = ps(
        "Get-PrinterPort | Select-Object Name, Description, PrinterHostAddress | ConvertTo-Json -Compress",
        &[],
    )?;
    Ok(json_list(&out)?
        .iter()
        .filter_map(|p| {
            let name = str_of(p, "Name")?;
            let host = str_of(p, "PrinterHostAddress");
            let kind = if name.to_uppercase().starts_with("USB") { "usb" } else if host.is_some() { "network" } else { "port" };
            let info = match (&host, str_of(p, "Description")) {
                (Some(h), _) => format!("{} ({})", name, h),
                (None, Some(d)) => format!("{} — {}", name, d),
                _ => name.clone(),
            };
            Some(Device { uri: name, kind: kind.into(), info })
        })
        .collect())
}

pub fn drivers() -> Result<Vec<Driver>, String> {
    let out = ps("Get-PrinterDriver | Select-Object Name | ConvertTo-Json -Compress", &[])?;
    Ok(json_list(&out)?
        .iter()
        .filter_map(|d| str_of(d, "Name"))
        .map(|n| Driver { id: n.clone(), name: n })
        .collect())
}

pub fn details(name: &str) -> Result<PrinterDetails, String> {
    let out = ps(
        "Get-Printer -Name $env:FXP_NAME | Select-Object Name, DriverName, PortName, Comment, Location, Shared | ConvertTo-Json -Compress",
        &[("NAME", name.to_string())],
    )?;
	let p = json_list(&out)?.into_iter().next().ok_or_else(|| format!("Printer '{}' was not found", name))?;
    Ok(PrinterDetails {
        name: name.to_string(),
        description: str_of(&p, "Comment"),
        location: str_of(&p, "Location"),
        connection: str_of(&p, "PortName"),
        driver: str_of(&p, "DriverName"),
        shared: p.get("Shared").and_then(|v| v.as_bool()).unwrap_or(false),
        can_rename: true,
    })
}

const ADD_SCRIPT: &str = r#"
$port = $env:FXP_PORT
switch ($env:FXP_MODE) {
  'ipp' {
    Add-Printer -Name $env:FXP_NAME -IppURL $env:FXP_URL
  }
  'raw' {
    if (-not (Get-PrinterPort -Name $port -ErrorAction SilentlyContinue)) {
      Add-PrinterPort -Name $port -PrinterHostAddress $env:FXP_HOST -PortNumber ([int]$env:FXP_TCP)
    }
    Add-Printer -Name $env:FXP_NAME -DriverName $env:FXP_DRIVER -PortName $port
  }
  'lpd' {
    if (-not (Get-PrinterPort -Name $port -ErrorAction SilentlyContinue)) {
      Add-PrinterPort -Name $port -LprHostAddress $env:FXP_HOST -LprQueueName $env:FXP_QUEUE
    }
    Add-Printer -Name $env:FXP_NAME -DriverName $env:FXP_DRIVER -PortName $port
  }
  default {
    Add-Printer -Name $env:FXP_NAME -DriverName $env:FXP_DRIVER -PortName $port
  }
}
Set-Printer -Name $env:FXP_NAME -Comment $env:FXP_COMMENT -Location $env:FXP_LOCATION
if ($env:FXP_SHARED -eq 'true') { Set-Printer -Name $env:FXP_NAME -Shared $true -ShareName $env:FXP_NAME }
"#;

/// Environment for `ADD_SCRIPT`.
pub(crate) fn add_env(p: &NewPrinter) -> Result<Vec<(&'static str, String)>, String> {
    let driver = || {
        p.driver
            .clone()
            .filter(|d| !d.trim().is_empty())
			.ok_or_else(|| "Windows needs a driver: choose an installed driver (or use IPP)".to_string())
    };
    let mut env = vec![
        ("NAME", p.name.clone()),
        ("COMMENT", p.description.clone().unwrap_or_default()),
        ("LOCATION", p.location.clone().unwrap_or_default()),
        ("SHARED", p.shared.to_string()),
    ];
    match &p.connection {
        Connection::Network { host, protocol: NetProtocol::Ipp, port, queue } if p.driver.is_none() => {
            env.push(("MODE", "ipp".into()));
            let url = super::manage::network_uri(host, NetProtocol::Ipp, *port, queue.as_deref())
                .replacen("ipp://", "http://", 1);
            env.push(("URL", url));
        }
        Connection::Network { host, protocol, port, queue } => {
            let (mode, tcp) = match protocol {
                NetProtocol::Lpd => ("lpd", 515),
                // IPP with a chosen driver goes over raw TCP to the same host.
                NetProtocol::Raw | NetProtocol::Ipp => ("raw", port.unwrap_or(9100)),
            };
            env.push(("MODE", mode.into()));
            env.push(("HOST", host.clone()));
            env.push(("TCP", tcp.to_string()));
            env.push(("QUEUE", queue.clone().unwrap_or_else(|| "queue".into())));
            env.push(("PORT", format!("ixPrint_{}_{}", host, if mode == "lpd" { "lpd".to_string() } else { tcp.to_string() })));
            env.push(("DRIVER", driver()?));
        }
        Connection::Device { uri } => {
            env.push(("MODE", "port".into()));
            env.push(("PORT", uri.clone()));
            env.push(("DRIVER", driver()?));
        }
    }
    Ok(env)
}

pub fn add(p: &NewPrinter) -> Result<(), String> {
    ps(ADD_SCRIPT, &add_env(p)?).map(|_| ())
}

pub fn update(name: &str, e: &PrinterEdit) -> Result<(), String> {
    ps(
        r#"
$name = $env:FXP_OLD
if ($env:FXP_NEW -ne $name) { Rename-Printer -Name $name -NewName $env:FXP_NEW; $name = $env:FXP_NEW }
Set-Printer -Name $name -Comment $env:FXP_COMMENT -Location $env:FXP_LOCATION
if ($env:FXP_SHARED -eq 'true') { Set-Printer -Name $name -Shared $true -ShareName $name }
else { Set-Printer -Name $name -Shared $false }
"#,
        &[
            ("OLD", name.to_string()),
            ("NEW", e.name.clone()),
            ("COMMENT", e.description.clone().unwrap_or_default()),
            ("LOCATION", e.location.clone().unwrap_or_default()),
            ("SHARED", e.shared.to_string()),
        ],
    )
    .map(|_| ())
}

pub fn remove(name: &str) -> Result<(), String> {
    ps("Remove-Printer -Name $env:FXP_NAME", &[("NAME", name.to_string())]).map(|_| ())
}

pub fn set_os_default(name: &str) -> Result<(), String> {
    ps(
        "(New-Object -ComObject WScript.Network).SetDefaultPrinter($env:FXP_NAME)",
        &[("NAME", name.to_string())],
    )
    .map(|_| ())
}
