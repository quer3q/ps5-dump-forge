// The http build's transport (`npm run build:http`): a browser on the LAN talking to
// `ps5-dump-forge serve`. No Tauri code reaches this bundle (vite.config.ts aliases
// `forge-transport` here).

import { useSyncExternalStore } from "react";

import { call, getStatus, listen, watchStatus } from "./http-client";
import { setServerSeparator } from "./paths";
import { pick } from "./Picker";
import type { Transport } from "./transport";

// Paths are the server's: `/` until its session says otherwise (main-http.tsx asks it before
// the app renders), never the viewer's OS. The poller starts from main-http.tsx too.
setServerSeparator("/");

export const transport: Transport = {
  web: true,
  call,
  listen,
  pick,
  useOffline: () => useSyncExternalStore(watchStatus, getStatus).offline,
  lz4Patch: (source) => call("lz4_patch", { source }),
  lz4Unpatch: (source) => call("lz4_unpatch", { source }),
  // The server resolves the plan; the browser saves the TOML as a download.
  savePlanProfile: async <T,>(request: unknown): Promise<T | null> => {
    const saved = await call<{ file_name: string; toml: string }>("lz4_plan_profile", { request });
    const url = URL.createObjectURL(new Blob([saved.toml], { type: "application/toml" }));
    const a = document.createElement("a");
    a.href = url;
    a.download = saved.file_name;
    document.body.appendChild(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(url), 60_000);
    return saved as T;
  },
};
