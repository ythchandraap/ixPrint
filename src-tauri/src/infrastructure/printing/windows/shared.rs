//! Shared Windows spooler thread and DEVMODE access.

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Graphics::Gdi::DEVMODEW;
use windows::Win32::Graphics::Printing::{ClosePrinter, DocumentPropertiesW, OpenPrinterW, PRINTER_HANDLE};

pub(super) type Task = Box<dyn FnOnce() + Send + 'static>;

fn spooler_thread() -> &'static std::sync::Mutex<std::sync::mpsc::Sender<Task>> {
    static TX: std::sync::OnceLock<std::sync::Mutex<std::sync::mpsc::Sender<Task>>> = std::sync::OnceLock::new();
    TX.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<Task>();
        std::thread::Builder::new()
            .name("ixprint-spooler".into())
            .spawn(move || {
                for task in rx {
                    // A panic must not end this thread.
                    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(task));
                }
            })
            .expect("spooler thread");
        std::sync::Mutex::new(tx)
    })
}

pub(super) fn on_spooler_thread<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    let task: Task = Box::new(move || {
        let _ = tx.send(f());
    });
    spooler_thread()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .send(task)
        .map_err(|_| "The printer thread stopped".to_string())?;
    rx.recv()
        .map_err(|_| "The printer driver failed unexpectedly".to_string())
}

// ---------------------------------------------------------------------------
// DEVMODE and capabilities

const DM_IN_BUFFER: u32 = 8;
const DM_OUT_BUFFER: u32 = 2;

/// A printer's DEVMODE as raw bytes (drivers append private data after DEVMODEW).
pub(super) struct DevMode(pub(super) Vec<u64>);

impl DevMode {
    pub(super) fn ptr(&self) -> *const DEVMODEW {
        self.0.as_ptr() as *const DEVMODEW
    }
    pub(super) fn get(&self) -> &DEVMODEW {
        unsafe { &*self.ptr() }
    }
    pub(super) fn get_mut(&mut self) -> &mut DEVMODEW {
        unsafe { &mut *(self.0.as_mut_ptr() as *mut DEVMODEW) }
    }
}

pub(super) struct PrinterHandle(pub(super) PRINTER_HANDLE);

impl PrinterHandle {
    pub(super) fn open(name: &HSTRING) -> Result<Self, String> {
        let mut handle = PRINTER_HANDLE::default();
        unsafe { OpenPrinterW(PCWSTR(name.as_ptr()), &mut handle, None) }
            .map_err(|_| format!("Printer '{}' was not found", name))?;
        Ok(PrinterHandle(handle))
    }

    pub(super) fn default_devmode(&self, name: &HSTRING) -> Result<DevMode, String> {
        let device = PCWSTR(name.as_ptr());
        let size = unsafe { DocumentPropertiesW(None, self.0, device, None, None, 0) };
        if size <= 0 {
            return Err(format!("Could not read the settings for printer '{}'", name));
        }
        let mut dm = DevMode(vec![0u64; (size as usize).div_ceil(8)]);
        let ptr = dm.get_mut() as *mut DEVMODEW;
        if unsafe { DocumentPropertiesW(None, self.0, device, Some(ptr), None, DM_OUT_BUFFER) } < 0 {
            return Err(format!("Could not read the settings for printer '{}'", name));
        }
        Ok(dm)
    }

    /// Lets the driver validate and complete a modified DEVMODE.
    pub(super) fn merge(&self, name: &HSTRING, dm: &mut DevMode) -> Result<(), String> {
        let ptr = dm.get_mut() as *mut DEVMODEW;
        let ret = unsafe {
            DocumentPropertiesW(
                None,
                self.0,
                PCWSTR(name.as_ptr()),
                Some(ptr),
                Some(ptr as *const DEVMODEW),
                DM_IN_BUFFER | DM_OUT_BUFFER,
            )
        };
        if ret < 0 {
            return Err(format!("Printer '{}' rejected the print settings", name));
        }
        Ok(())
    }
}

impl Drop for PrinterHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = ClosePrinter(self.0);
        }
    }
}
