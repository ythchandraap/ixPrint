import { invoke } from "../client";
import type { PrinterDevice, PrinterDriver, PrinterDetails, NewPrinter, PrinterEdit, Config } from "../types";

export const printerAdminCommands = {
	printerDevices: () => invoke<PrinterDevice[]>("printer_devices"),
	printerDrivers: (query: string) => invoke<PrinterDriver[]>("printer_drivers", { query }),
	printerDetails: (name: string) => invoke<PrinterDetails>("printer_details", { name }),
	printerAdd: (printer: NewPrinter) => invoke<void>("printer_add", { printer }),
	printerUpdate: (name: string, edit: PrinterEdit) => invoke<Config>("printer_update", { name, edit }),
	printerRemove: (name: string) => invoke<Config>("printer_remove", { name }),
	printerSetOsDefault: (name: string) => invoke<void>("printer_set_os_default", { name }),
};
