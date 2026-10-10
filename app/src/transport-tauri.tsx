// The desktop app's transport: Tauri IPC, Tauri events and the native file dialog.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";

import { joinPath } from "./paths";

import type { PickOptions, Transport, Unlisten } from "./transport";

async function pick(o: PickOptions): Promise<string | null> {
  const p = o.directory
    ? await open({ directory: true, title: o.title })
    : await open({
        title: o.title,
        filters: [{ name: o.filterName ?? o.title, extensions: o.extensions ?? [] }],
      });
  return typeof p === "string" ? p : null;
}

export const transport: Transport = {
  web: false,
  call: (cmd, args) => invoke(cmd, args),
  listen: <T,>(event: string, f: (payload: T) => void): Promise<Unlisten> =>
    // The restore event is the http poller's; nothing in the app sends it.
    event === "jobs://restore" ? Promise.resolve(() => {}) : listen<T>(event, (e) => f(e.payload)),
  pick,
  useOffline: () => false,
  address: () => null,
  lz4Patch: (path) => invoke("lz4_patch", { path }),
  lz4Unpatch: (path) => invoke("lz4_unpatch", { path }),
  savePlanProfile: async (request, name, dir) => {
    const dest = await save({
      title: "Save as profile",
      defaultPath: dir ? joinPath(dir, name) : name,
      filters: [{ name: "LZ4 profile (TOML)", extensions: ["toml"] }],
    });
    return dest ? invoke("lz4_save_plan_profile", { request, dest }) : null;
  },
};
