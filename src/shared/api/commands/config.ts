import { invoke } from "../client";
import type { Config } from "../types";

export const configCommands = {
  config: () => invoke<Config>("get_config"),
  setDefaultPrinter: (printer: string | null) => invoke<Config>("set_default_printer", { printer }),
  setDefaultPreset: (name: string | null) => invoke<Config>("set_default_preset", { name }),
};
