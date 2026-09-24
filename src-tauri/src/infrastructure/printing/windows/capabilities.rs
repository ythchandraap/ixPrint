//! Windows paper and tray capability queries.

use super::shared::{on_spooler_thread, DevMode, PrinterHandle};
use crate::domain::printer::{Capabilities, PaperInfo};
use windows::core::{HSTRING, PCWSTR, PWSTR};
use windows::Win32::Graphics::Gdi::{
    CreateICW, DeleteDC, GetDeviceCaps, DMORIENT_PORTRAIT, DM_DEFAULTSOURCE, DM_ORIENTATION, DM_PAPERSIZE, LOGPIXELSX,
    LOGPIXELSY, PHYSICALHEIGHT, PHYSICALWIDTH,
};
use windows::Win32::Storage::Xps::{
    DeviceCapabilitiesW, DC_BINNAMES, DC_BINS, DC_COLORDEVICE, DC_DUPLEX, DC_MAXEXTENT, DC_PAPERNAMES, DC_PAPERS,
    DC_PAPERSIZE, PRINTER_DEVICE_CAPABILITIES,
};

const PAPER_NAME_LEN: usize = 64;
const BIN_NAME_LEN: usize = 24;

/// Calls DeviceCapabilitiesW twice (count, then data) with `item_size` elements of `T`.
fn device_caps<T: Default + Clone>(name: &HSTRING, cap: PRINTER_DEVICE_CAPABILITIES, item_len: usize) -> Vec<T> {
    let device = PCWSTR(name.as_ptr());
    let count = unsafe { DeviceCapabilitiesW(device, PCWSTR::null(), cap, None, None) };
    if count <= 0 {
        return vec![];
    }
    let mut buf = vec![T::default(); count as usize * item_len];
    let got = unsafe {
        DeviceCapabilitiesW(
            device,
            PCWSTR::null(),
            cap,
            Some(PWSTR(buf.as_mut_ptr() as *mut u16)),
            None,
        )
    };
    if got <= 0 {
        return vec![];
    }
    buf.truncate(got as usize * item_len);
    buf
}

fn wide_names(buf: &[u16], len: usize) -> Vec<String> {
    buf.chunks(len)
        .map(|c| {
            let end = c.iter().position(|&ch| ch == 0).unwrap_or(c.len());
            String::from_utf16_lossy(&c[..end])
        })
        .collect()
}

pub(super) struct PaperEntry {
    pub(super) id: i16,
    pub(super) info: PaperInfo,
}

pub(super) fn papers(name: &HSTRING) -> Vec<PaperEntry> {
    let ids: Vec<u16> = device_caps(name, DC_PAPERS, 1);
    let sizes: Vec<i32> = device_caps(name, DC_PAPERSIZE, 2); // POINT, tenths of a mm
    let names = wide_names(&device_caps::<u16>(name, DC_PAPERNAMES, PAPER_NAME_LEN), PAPER_NAME_LEN);
    ids.iter()
        .enumerate()
        .filter_map(|(i, &id)| {
            let (w, h) = (*sizes.get(i * 2)?, *sizes.get(i * 2 + 1)?);
            Some(PaperEntry {
                id: id as i16,
                info: PaperInfo {
                    name: names.get(i).cloned().unwrap_or_else(|| format!("#{}", id)),
                    width_mm: w as f32 / 10.0,
                    height_mm: h as f32 / 10.0,
                },
            })
        })
        .collect()
}

pub(super) fn trays(name: &HSTRING) -> Vec<(i16, String)> {
    let ids: Vec<u16> = device_caps(name, DC_BINS, 1);
    let names = wide_names(&device_caps::<u16>(name, DC_BINNAMES, BIN_NAME_LEN), BIN_NAME_LEN);
    ids.iter().zip(names).map(|(&id, n)| (id as i16, n)).collect()
}

pub fn capabilities(printer: &str) -> Result<Capabilities, String> {
    let printer = printer.to_string();
    on_spooler_thread(move || capabilities_impl(&printer))?
}

fn capabilities_impl(printer: &str) -> Result<Capabilities, String> {
    let name = HSTRING::from(printer);
    let handle = PrinterHandle::open(&name)?;
    let dm = handle.default_devmode(&name)?;
    let papers = papers(&name);
    let trays = trays(&name);

    let fields = dm.get().dmFields;
    let settings = unsafe { dm.get().Anonymous1.Anonymous1 };
    let default_paper = papers
        .iter()
        .find(|p| fields.contains(DM_PAPERSIZE) && p.id == settings.dmPaperSize)
        .map(|p| p.info.clone())
        .or_else(|| physical_paper(&name, &dm));
    let default_tray = trays
        .iter()
        .find(|(id, _)| fields.contains(DM_DEFAULTSOURCE) && *id == settings.dmDefaultSource)
        .map(|(_, n)| n.clone());

    let device = PCWSTR(name.as_ptr());
    let flag = |cap| unsafe { DeviceCapabilitiesW(device, PCWSTR::null(), cap, None, None) } == 1;

    // LOWORD = max dmPaperWidth, HIWORD = max dmPaperLength, tenths of a mm.
    let extent = unsafe { DeviceCapabilitiesW(device, PCWSTR::null(), DC_MAXEXTENT, None, None) };
    let (max_width_mm, max_height_mm) = if extent > 0 {
        let (w, h) = ((extent & 0xFFFF) as f32 / 10.0, ((extent >> 16) & 0xFFFF) as f32 / 10.0);
        ((w > 0.0).then_some(w), (h > 0.0).then_some(h))
    } else {
        (None, None)
    };

    Ok(Capabilities {
        papers: papers.into_iter().map(|p| p.info).collect(),
        default_paper,
        trays: trays.into_iter().map(|(_, n)| n).collect(),
        default_tray,
        duplex: flag(DC_DUPLEX),
        color: flag(DC_COLORDEVICE),
        max_width_mm,
        max_height_mm,
    })
}

/// Physical paper size of the default DEVMODE (portrait), read from an information context.
fn physical_paper(name: &HSTRING, dm: &DevMode) -> Option<PaperInfo> {
    let mut dm = DevMode(dm.0.clone());
    {
        let d = dm.get_mut();
        d.dmFields |= DM_ORIENTATION;
        d.Anonymous1.Anonymous1.dmOrientation = DMORIENT_PORTRAIT as i16;
    }
    let ic = unsafe { CreateICW(&HSTRING::from("WINSPOOL"), name, PCWSTR::null(), Some(dm.ptr())) };
    if ic.is_invalid() {
        return None;
    }
    let caps = |i| unsafe { GetDeviceCaps(Some(ic), i) } as f32;
    let info = PaperInfo {
        name: "default".into(),
        width_mm: caps(PHYSICALWIDTH) / caps(LOGPIXELSX) * 25.4,
        height_mm: caps(PHYSICALHEIGHT) / caps(LOGPIXELSY) * 25.4,
    };
    unsafe {
        let _ = DeleteDC(ic);
    }
    (info.width_mm > 0.0 && info.height_mm > 0.0).then_some(info)
}
