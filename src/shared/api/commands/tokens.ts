import { invoke } from "../client";
import type { Config, TokenUse } from "../types";

export const tokenCommands = {
  /** Returns the plain token — the only time it is available. */
  createToken: (name: string) => invoke<{ token: string; config: Config }>("create_token", { name }),
  revokeToken: (id: string) => invoke<Config>("revoke_token", { id }),
  setRequireToken: (required: boolean) => invoke<Config>("set_require_token", { required }),
  setAllowedOrigins: (origins: string[]) => invoke<Config>("set_allowed_origins", { origins }),
  tokenUsage: () => invoke<Record<string, TokenUse>>("token_usage"),
};
