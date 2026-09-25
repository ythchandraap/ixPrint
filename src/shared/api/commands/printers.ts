import { invoke } from "../client";
import type { PrinterView, PrinterQueue, JobStatus, Capabilities } from "../types";

export const printerCommands = {
  printers: () => invoke<PrinterView[]>("get_printers"),
  queue: (printer: string) => invoke<PrinterQueue>("get_queue", { printer }),
  jobStatus: (jobId: string) => invoke<JobStatus>("job_status", { jobId }),
  capabilities: (printer: string) => invoke<Capabilities>("get_capabilities", { printer }),
};
