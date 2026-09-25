import { invoke } from "../client";
import type { Config } from "../types";

export const appCommands = {
  /** null when the computer has no network connection. */
  localIp: () => invoke<string | null>("this_local_ip"),
  /** Opens the /docs page in its own window, or in the browser when `external`. */
  /** Opens the /docs page in its own window, or in the browser when `external`. */
  openDocs: (external = false) => invoke<void>("open_docs", { external }),
  dataFolderPath: () => invoke<string>("data_folder_path"),
  openDataFolder: () => invoke<void>("open_data_folder"),
  os: () => invoke<string>("os_detected"),
  engineStatus: () => invoke<void>("engine_status"),
  /** Rejects with the reason when the HTTP server is not running. */
  /** Rejects with the reason when the HTTP server is not running. */
  serverStatus: () => invoke<void>("server_status"),
  bridgeEnabled: () => invoke<boolean>("bridge_enabled"),
  setBridgeEnabled: (enabled: boolean) => invoke<Config>("set_bridge_enabled", { enabled }),
  setServerPort: (port: number) => invoke<Config>("set_server_port", { port }),
  setLanAccess: (enabled: boolean) => invoke<Config>("set_lan_access", { enabled }),
};
