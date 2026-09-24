import { invoke } from "../client";
import type { RemoteView } from "../types";

export const remoteCommands = {
  remotes: () => invoke<RemoteView[]>("remote_list"),
  remoteAdd: (name: string, url: string, token: string, preset?: string) =>
    invoke<RemoteView[]>("remote_add", { name, url, token, preset: preset ?? null }),
  remoteUpdate: (id: string, name: string, url: string, token?: string, preset?: string) =>
    invoke<RemoteView[]>("remote_update", { id, name, url, token: token || null, preset: preset ?? null }),
  remoteRemove: (id: string) => invoke<RemoteView[]>("remote_remove", { id }),
  remoteTest: (id: string) => invoke<RemoteView>("remote_test", { id }),
};
