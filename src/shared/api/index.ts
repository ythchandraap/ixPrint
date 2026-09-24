import { appCommands } from "./commands/app";
import { configCommands } from "./commands/config";
import { printerCommands } from "./commands/printers";
import { printerAdminCommands } from "./commands/printerAdmin";
import { presetCommands } from "./commands/presets";
import { tokenCommands } from "./commands/tokens";
import { remoteCommands } from "./commands/remotes";
import { historyCommands } from "./commands/history";
import { diagnosticCommands } from "./commands/diagnostics";
import { printCommands } from "./commands/print";

export const api = {
  ...appCommands,
  ...configCommands,
  ...printerCommands,
  ...printerAdminCommands,
  ...presetCommands,
  ...tokenCommands,
  ...remoteCommands,
  ...historyCommands,
  ...diagnosticCommands,
  ...printCommands,
};

export { errorText, isInApp } from "./client";
export * from "./types";
