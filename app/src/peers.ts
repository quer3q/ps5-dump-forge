// Saved devices' names for backend text: a leaf module (no imports), since api and useConnectState import each other.

/** How a saved NAS is reached; core spells its paths `<protocol>://host[:port]/...`. */
export type NasProtocol = "smb" | "ftp" | "nfs";
/** Each protocol's own port: an address or a path names the port only when it is another one. */
export const NAS_PORT: Record<NasProtocol, number> = { smb: 445, ftp: 21, nfs: 2049 };

type SavedNas = { name: string; host: string; port: number; protocol?: NasProtocol };

let devices: { key_id: string; name: string }[] = [];
let nas: SavedNas[] = [];

export function setDevices(next: { key_id: string; name: string }[], saved: SavedNas[] = []): void {
  devices = next;
  nas = saved;
}

/** The saved NAS a `smb://`, `ftp://` or `nfs://` address names (no port: the protocol's own). */
function savedName(scheme: string, host: string, port: string | undefined, list: SavedNas[]): string | null {
  const p = scheme.toLowerCase() as NasProtocol;
  const h = host.replace(/^\[|\]$/g, "").toLowerCase();
  const at = port ? Number(port) : NAS_PORT[p];
  return list.find((n) => (n.protocol ?? "smb") === p && n.host.toLowerCase() === h && n.port === at)?.name ?? null;
}

/** The NAS a path is on: its saved name, else its address; null for any other path. */
export function nasName(path: string, list: SavedNas[] = nas): string | null {
  const m = /^(smb|ftp|nfs):\/\/(\[[^\]\s]*\]|[^/\\\s:[\]]+)(?::(\d+))?/i.exec(path);
  return m ? (savedName(m[1], m[2], m[3], list) ?? m[0].slice(m[1].length + 3)) : null;
}

/** A saved NAS path as "<saved name>: /<path>"; null for any other path or an unsaved NAS. */
export function nasPath(path: string, list: SavedNas[] = nas): string | null {
  const m = /^(smb|ftp|nfs):\/\/(\[[^\]\s]*\]|[^/\\\s:[\]]+)(?::(\d+))?(?=$|[/\\])/i.exec(path);
  const name = m && savedName(m[1], m[2], m[3], list);
  return m && name ? `${name}: ${path.slice(m[0].length) || "/"}` : null;
}

/** Backend text with each saved device's address shown as its name ("<name>: /<path>"); an unknown one stays. */
export function readable(text: string): string {
  if (devices.length) {
    text = text.replace(/flink:\/\/([0-9a-f]{32})(?=([/\\]?))/gi, (whole, key: string, sep: string) => {
      const d = devices.find((x) => x.key_id === key);
      return d ? (sep ? `${d.name}: ` : d.name) : whole;
    });
  }
  if (!nas.length) return text;
  return text.replace(
    /\b(smb|ftp|nfs):\/\/(\[[^\]\s]*\]|[^/\\\s:[\]]+)(?::(\d+))?(?=([/\\]?))/gi,
    (whole, scheme: string, host: string, port: string | undefined, sep: string) => {
      const name = savedName(scheme, host, port, nas);
      return name ? (sep ? `${name}: ` : name) : whole;
    },
  );
}
