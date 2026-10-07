// Pieces shared by the screens.

import { useEffect, useRef, type KeyboardEvent, type ReactNode } from "react";
import { open } from "@tauri-apps/plugin-dialog";

import { prettyBytes, type Format, type Inspection, type Kind } from "./api";
import { basename, trimSep } from "./paths";
import { useCoverGlow } from "./glow";
import { Icon, type IconName } from "./icons";

const KIND_LABEL: Record<Kind, string> = {
  folder: "Folder",
  exfat: ".exfat",
  ffpkg: ".ffpkg",
  ffpfs: ".ffpfs",
  ffpfsc: ".ffpfsc",
  pkg: ".fpkg", // the debug FPKG's display name; its file still ends in .pkg
};

/** The target formats, in picker order; every one is also a source kind. */
export const FORMATS: Format[] = ["folder", "exfat", "ffpkg", "ffpfs", "ffpfsc", "pkg"];

export function kindLabel(kind: string): string {
  return KIND_LABEL[kind as Kind] ?? kind;
}

/** A format as a small tinted tag (hues: `.fmt-*` in styles.css). */
export function FormatPill({ kind }: { kind: string }) {
  const known = kind in KIND_LABEL;
  return <span className={known ? `tag fmt fmt-${kind}` : "tag"}>{kindLabel(kind)}</span>;
}

/** A card's title row: icon + title, actions on the right. */
export function CardHead(props: {
  icon: IconName;
  title: string;
  id?: string;
  /** The heading can take focus from code (tabIndex -1), e.g. when the control that had it
   * goes away. */
  focusable?: boolean;
  children?: ReactNode;
}) {
  return (
    <header className="card-head">
      <span className="head-icon">
        <Icon name={props.icon} />
      </span>
      <h2 id={props.id} tabIndex={props.focusable ? -1 : undefined}>
        {props.title}
      </h2>
      {props.children && <div className="head-actions">{props.children}</div>}
    </header>
  );
}

/** Element ids of a tab and its panel. */
export function tabId(prefix: string, id: string): string {
  return `${prefix}-tab-${id}`;
}
export function panelId(prefix: string, id: string): string {
  return `${prefix}-panel-${id}`;
}

/**
 * A segmented row of tabs (WAI-ARIA tabs: one tab stop, arrow keys / Home / End move and
 * select). The caller renders each panel with `id={panelId(..)}`, `role="tabpanel"`,
 * `aria-labelledby={tabId(..)}` and `hidden` when not selected.
 */
export function TabList<T extends string>(props: {
  label: string;
  prefix: string;
  tabs: { id: T; label: ReactNode }[];
  value: T;
  onChange: (id: T) => void;
  className?: string;
}) {
  const ids = props.tabs.map((t) => t.id);
  const onKey = (e: KeyboardEvent) => {
    const i = ids.indexOf(props.value);
    const next =
      e.key === "ArrowRight" ? ids[(i + 1) % ids.length]
      : e.key === "ArrowLeft" ? ids[(i - 1 + ids.length) % ids.length]
      : e.key === "Home" ? ids[0]
      : e.key === "End" ? ids[ids.length - 1]
      : undefined;
    if (next === undefined) return;
    e.preventDefault();
    props.onChange(next);
    document.getElementById(tabId(props.prefix, next))?.focus();
  };
  return (
    <div
      className={`seg ${props.className ?? ""}`}
      role="tablist"
      aria-label={props.label}
      onKeyDown={onKey}
    >
      {props.tabs.map((t) => {
        const on = t.id === props.value;
        return (
          <button
            key={t.id}
            type="button"
            role="tab"
            id={tabId(props.prefix, t.id)}
            aria-selected={on}
            aria-controls={panelId(props.prefix, t.id)}
            tabIndex={on ? 0 : -1}
            className={on ? "seg-opt on" : "seg-opt"}
            onClick={() => props.onChange(t.id)}
          >
            {t.label}
          </button>
        );
      })}
    </div>
  );
}

/**
 * One-of-N format choice as a segmented group, each format in its own hue. Native radios
 * underneath (visually hidden), so Tab / arrow keys and radio semantics come from the
 * browser.
 */
export function FormatPicker<F extends Format>(props: {
  name: string;
  label: string;
  value: F;
  onChange: (f: F) => void;
  disabled?: (f: F) => boolean;
  /** Default: every target format. */
  formats?: F[];
  className?: string;
}) {
  const formats = props.formats ?? (FORMATS as F[]);
  return (
    <div
      className={`seg formats ${props.className ?? ""}`}
      role="radiogroup"
      aria-label={props.label}
    >
      {formats.map((f) => (
        // `on` from state, not `input:checked + …`: WebKit misses that restyle when React
        // sets `checked` by code (a new source switching the format).
        <label key={f} className={`seg-choice fmt-${f}${props.value === f ? " on" : ""}`}>
          <input
            type="radio"
            className="visually-hidden"
            name={props.name}
            value={f}
            checked={props.value === f}
            disabled={props.disabled?.(f) ?? false}
            onChange={() => props.onChange(f)}
          />
          <span className="seg-opt">{KIND_LABEL[f]}</span>
        </label>
      ))}
    </div>
  );
}

export const IMAGE_EXTENSIONS = ["exfat", "ffpkg", "ffpfs", "ffpfsc", "pkg"];

export { basename, dirname, joinPath, SEPARATORS } from "./paths";

/**
 * A path on one line that never hides its last part: the folder part ellipsizes, the name
 * stays. The full path is the tooltip.
 */
export function PathLine({ path, prefix }: { path: string; prefix?: string }) {
  const name = basename(path);
  const dir = path.slice(0, trimSep(path).length - name.length);
  return (
    <p className="pathline" title={path}>
      <span className="dir">
        {prefix}
        {dir}
      </span>
      <span className="base">{name}</span>
    </p>
  );
}

/** "Choose folder…" / "Choose image…": a game folder or an image file. */
function PickButtons(props: { onPick: (path: string, isImage: boolean) => void; big?: boolean }) {
  const pickFolder = async () => {
    const p = await open({ directory: true, title: "Game folder" });
    if (typeof p === "string") props.onPick(p, false);
  };
  const pickImage = async () => {
    const p = await open({
      title: "Game image",
      filters: [{ name: "PS5 image or package", extensions: IMAGE_EXTENSIONS }],
    });
    if (typeof p === "string") props.onPick(p, true);
  };
  return (
    <>
      <button className={props.big ? "primary" : "small"} onClick={pickFolder}>
        <Icon name="folder" />
        Choose folder…
      </button>
      <button className={props.big ? undefined : "small"} onClick={pickImage}>
        <Icon name="disc" />
        Choose image…
      </button>
    </>
  );
}

/**
 * The Source card: an empty state with the pickers until a source is chosen, then the game
 * it holds (or why it can't be read). `full` adds what only Inspect shows.
 */
export function SourceCard(props: {
  path: string | null;
  onPick: (path: string, isImage: boolean) => void;
  busy: boolean;
  error: string | null;
  ins: Inspection | null;
  full?: boolean;
}) {
  const { path, ins } = props;
  const headId = `${props.full ? "i" : "c"}-source`;
  const glow = useCoverGlow(!path ? null : ins ? ins.cover : props.busy ? undefined : null);

  // The first pick unmounts the empty state's buttons, which had focus: hand it to the
  // card's heading instead of letting it fall to the page.
  const hadPath = useRef(path !== null);
  useEffect(() => {
    const a = document.activeElement;
    if (path && !hadPath.current && (!a || a === document.body)) {
      document.getElementById(headId)?.focus();
    }
    hadPath.current = path !== null;
  }, [path, headId]);

  return (
    <section
      className={`card hero${props.full ? " full" : ""}${glow.ready ? " lit" : ""}`}
      style={glow.style}
      aria-labelledby={headId}
    >
      <CardHead icon="disc" title="Source" id={headId} focusable>
        {path && <PickButtons onPick={props.onPick} />}
      </CardHead>
      {!path ? (
        <div className="empty">
          <span className="empty-icon">
            <Icon name="disc" />
          </span>
          <p className="empty-title">Choose a game folder or image</p>
          <p className="muted">
            A dumped PS5 game folder (sce_sys/param.json and eboot.bin at its root), or an image
            or package of one.
          </p>
          <div className="kinds" aria-label="Accepted sources">
            {FORMATS.map((k) => (
              <FormatPill key={k} kind={k} />
            ))}
          </div>
          <div className="row center">
            <PickButtons onPick={props.onPick} big />
          </div>
        </div>
      ) : (
        <>
          <GameHead path={path} ins={ins} busy={props.busy} full={props.full} />
          {props.error && <p className="bad">{props.error}</p>}
          {ins && <InspectSummary ins={ins} full={props.full} />}
        </>
      )}
    </section>
  );
}

/** Cover, title, kind tags and the source's path. */
function GameHead(props: { path: string; ins: Inspection | null; busy: boolean; full?: boolean }) {
  const { ins } = props;
  const title = ins
    ? (ins.title_name ?? ins.title_id ?? "Unknown title")
    : props.busy
      ? "Reading the source…"
      : "Unreadable source";
  return (
    <div className={props.full ? "game full" : "game"}>
      {ins?.cover ? (
        <img className="cover" src={ins.cover} alt="" />
      ) : (
        <div className="cover none" aria-hidden="true">
          <Icon name={ins ? "disc" : "folder"} />
        </div>
      )}
      <div className="game-text">
        <div className="title-row">
          <h3 className={ins ? "title" : "title pending"}>{title}</h3>
          {ins && <FormatPill kind={ins.kind} />}
          {ins && ins.backport.length > 0 && <span className="tag orange">Backport</span>}
          {ins && ins.dlcs.length > 0 && <span className="tag violet">DLC</span>}
        </div>
        <PathLine path={props.path} />
      </div>
    </div>
  );
}

/** The game's facts as inset tiles. */
function Facts({ ins, full }: { ins: Inspection; full: boolean }) {
  const bp = ins.backport.length > 0;
  // A backport lowers the firmware the game needs to what its executables allow.
  const minFw = bp && ins.backport_firmware ? ins.backport_firmware : ins.firmware;
  const fwNote: string[] = [];
  if (bp && ins.backport_firmware && ins.firmware) fwNote.push(`declares ${ins.firmware}`);
  if (bp && !ins.backport_firmware)
    fwNote.push("backport: firmware unknown (no readable SDK in its executables)");
  if (bp) fwNote.push(backportLib(ins));
  if (full && ins.sdk) fwNote.push(`SDK ${ins.sdk} (declared)`);
  return (
    <dl className="tiles">
      <div className="tile">
        <dt>Title ID</dt>
        <dd className={ins.title_id ? "value mono" : "value"}>{ins.title_id ?? "—"}</dd>
      </div>
      <div className="tile">
        <dt>Version</dt>
        <dd className="value">{ins.version ?? "—"}</dd>
      </div>
      <div className="tile">
        <dt>Firmware</dt>
        <dd>
          <span className="value">{minFw ? `${minFw}+` : "—"}</span>
          {minFw && (
            <span className={bp && ins.backport_firmware ? "tag orange" : "tag blue"}>
              {bp && ins.backport_firmware ? "backport" : "declared"}
            </span>
          )}
          {fwNote.length > 0 && <span className="note">{fwNote.join(" · ")}</span>}
        </dd>
      </div>
      <div className="tile">
        <dt>Size</dt>
        <dd>
          <span className="value">{prettyBytes(ins.total_bytes)}</span>
          <span className="tag">
            {ins.files.length.toLocaleString()} file{ins.files.length === 1 ? "" : "s"}
          </span>
        </dd>
      </div>
      {full && (
        <div className="tile wide">
          <dt>Content ID</dt>
          <dd className={ins.content_id ? "value mono" : "value"}>{ins.content_id ?? "—"}</dd>
        </div>
      )}
    </dl>
  );
}

/** Which fakelib folder a backport ships, and how many files it has. */
function backportLib(ins: Inspection): string {
  const exclusive = ins.backport.some((p) => /^fakelib2\//i.test(p));
  const libs = ins.backport.filter((p) => /^fakelib2?\//i.test(p)).length;
  // Libraries only: "Backport files (N)" also counts ampr_emu.index next to them.
  return `${exclusive ? "fakelib2/, mounted alone" : "fakelib/"}, ${libs} librar${libs === 1 ? "y" : "ies"}`;
}

/** Preflight findings, each with a status icon; one green line when there are none. */
export function Findings({ lines }: { lines: string[] }) {
  if (lines.length === 0)
    return (
      <p className="status-line good">
        <Icon name="checkCircle" />
        No preflight findings
      </p>
    );
  return (
    <ul className="findings" aria-label="Preflight findings">
      {lines.map((l, i) => {
        const { kind } = classify(l);
        return (
          <li key={i} className={kind}>
            <Icon name={kind === "info" ? "info" : kind === "block" ? "alert" : "warn"} />
            <span>{l}</span>
          </li>
        );
      })}
    </ul>
  );
}

/**
 * What a finding from core means for a build: `block` fails every target, `warn` may fail
 * (`formats`: the targets it fails), `info` is only news.
 */
// ponytail: core reports findings as plain text, so this matches its wording (preflight.rs
// `input`, inspect.rs). Ceiling: core should tag each finding with a severity and formats.
export function classify(line: string): { kind: "block" | "warn" | "info"; formats?: Format[] } {
  if (/^(eboot\.bin is missing|sce_sys\/param\.json)/.test(line)) return { kind: "block" };
  // Core says "(refused by .exfat/.ffpkg)", but a .pkg build refuses them too (package.rs).
  // .ffpfs takes ASCII names only, and a .ffpfsc's inner image refuses them too.
  if (/^name not in NFC/.test(line))
    return { kind: "warn", formats: ["exfat", "ffpkg", "ffpfs", "ffpfsc", "pkg"] };
  // PFS holds ASCII names only; a .ffpfsc defaults to exFAT inside, which takes them.
  if (/^name not ASCII/.test(line)) return { kind: "warn", formats: ["ffpfs"] };
  if (/^extraction:/.test(line)) return { kind: "warn", formats: ["folder"] };
  if (/^(already 512-byte sectors|geometry differs from the SMP fast path)/.test(line))
    return { kind: "info" };
  return { kind: "warn" };
}

/** The DLC embedded in the dump, collapsed until asked for. */
function DlcList({ dlcs }: { dlcs: Inspection["dlcs"] }) {
  if (dlcs.length === 0) return null;
  const notes = dlcs.map(dlcNote);
  // One shared explanation goes on top once instead of under every row.
  const shared = notes.every((n) => n === notes[0]) ? notes[0] : null;
  return (
    <details className="fold">
      <summary>Embedded DLC ({dlcs.length})</summary>
      {shared && <p className="muted fold-note">All {shared}.</p>}
      <ul className="rows scroll" tabIndex={0} aria-label="Embedded DLC">
        {dlcs.map((d, i) => (
          <li key={d.content_id}>
            <span className="row-title">
              {d.name && <strong>{d.name} </strong>}
              <span className="path">
                {d.content_id.slice(0, d.content_id.length - d.label.length)}
                <strong className="cid-label">{d.label}</strong>
              </span>
            </span>
            {!shared && <span className="muted">{notes[i]}</span>}
          </li>
        ))}
      </ul>
    </details>
  );
}

/** Where a DLC's content comes from, as one phrase. */
function dlcNote(d: Inspection["dlcs"][number]): string {
  if (d.folder) return `in ${d.folder}/, ${prettyBytes(d.bytes)}`;
  if (d.emulated === "NO_EXTRA_DATA")
    return "unlocked by the DLC emulator (dlc_emu.ini): entitlement only, no data of its own";
  if (d.emulated !== null)
    return `unlocked by the DLC emulator (dlc_emu.ini), status ${d.emulated || "not set"}`;
  return "merged into the game's folders";
}

/** The game's facts, its embedded DLC (and backport files, `full`), then the findings. */
export function InspectSummary({ ins, full = false }: { ins: Inspection; full?: boolean }) {
  return (
    <div className="summary">
      <Facts ins={ins} full={full} />
      {(ins.dlcs.length > 0 || (full && ins.backport.length > 0)) && (
        <div className="folds">
          <DlcList dlcs={ins.dlcs} />
          {full && ins.backport.length > 0 && (
            <details className="fold">
              <summary>Backport files ({ins.backport.length})</summary>
              <ul className="rows scroll" tabIndex={0} aria-label="Backport files">
                {ins.backport.map((p) => (
                  <li key={p} className="path">
                    {p}
                  </li>
                ))}
              </ul>
            </details>
          )}
        </div>
      )}
      <Findings lines={ins.findings} />
    </div>
  );
}
