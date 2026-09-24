import { invoke } from "../client";
import type { DiagnosticEntry } from "../types";

export const diagnosticCommands = {
  errorLog: () => invoke<DiagnosticEntry[]>("error_log"),
  recordError: (source: string, message: string, details?: string) =>
    invoke<void>("record_error", { source, message, details: details ?? null }),
  clearErrorLog: () => invoke<void>("clear_error_log"),
  openSystemLogs: () => invoke<void>("open_system_logs"),
};
