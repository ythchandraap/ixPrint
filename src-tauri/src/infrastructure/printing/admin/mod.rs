//! OS printer administration adapter with input validation and actionable errors.

use crate::domain::printer_admin::{
    validate_connection, validate_name, validate_text, Device, Driver, NewPrinter, PrinterDetails, PrinterEdit,
};
use crate::infrastructure::printing::list_printers;

#[cfg(not(windows))]
use super::admin_cups as os;
#[cfg(windows)]
use super::admin_windows as os;

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
    list_printers().iter().any(|p| p.name == name)
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

#[cfg(test)]
mod tests {
    #[test]
    fn explains_permission_errors() {
        assert!(super::explain("lpadmin: Forbidden".into()).contains("Administrator rights"));
        assert_eq!(
            super::explain("lpadmin: Bad device-uri".into()),
            "lpadmin: Bad device-uri"
        );
    }
}

#[cfg(test)]
mod live_probe {
    #[test]
    #[ignore]
    fn manage_live() {
        for d in super::devices().unwrap() {
            println!("device {:?}", d);
        }
        println!("drivers(zebra) = {:?}", super::drivers("zebra").unwrap());
        for p in crate::infrastructure::printing::list_printers().into_iter().take(3) {
            println!("details {:?}", super::details(&p.name));
        }
    }
}
