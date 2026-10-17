// The Edit tab's file tree, pure (no runtime imports: `scripts/check-explorer.mjs` runs it in
// plain node). Built from an inspection's `files` and `empty_dirs`; game paths are relative and
// `/`-separated, as core lists them.

/** What a pending change did to an entry (the explorer's badges): added where nothing was, put
 * over a file, or moved or renamed (`from`: its path in the game as it is now). */
export type Mark = "new" | "replaced" | "renamed";

export interface FileNode {
  kind: "file";
  name: string;
  size: number;
  mark?: Mark;
  from?: string;
}

export interface DirNode {
  kind: "dir";
  name: string;
  children: Map<string, TreeNode>;
  mark?: Mark;
  from?: string;
  /** Added from a listing that was cut short: more of its contents arrive on Apply. */
  host?: boolean;
}

export type TreeNode = FileNode | DirNode;

/** One row of a listing: the node and its game path. */
export interface Entry {
  path: string;
  node: TreeNode;
}

export function emptyDir(name: string): DirNode {
  return { kind: "dir", name, children: new Map() };
}

/** The game's tree. A path through a file (core never lists one) keeps the file and drops the
 * deeper path. */
export function buildTree(files: { path: string; size: number }[], emptyDirs: string[]): DirNode {
  const root = emptyDir("");
  for (const d of emptyDirs) ensureDir(root, split(d));
  for (const f of files) {
    const parts = split(f.path);
    const name = parts.pop();
    if (name === undefined) continue;
    const dir = ensureDir(root, parts);
    if (dir && !dir.children.has(name)) dir.children.set(name, { kind: "file", name, size: f.size });
  }
  return root;
}

/** A game path's components (empty ones dropped). */
export function split(path: string): string[] {
  return path.split("/").filter((p) => p !== "");
}

export function join(dir: string, name: string): string {
  return dir === "" ? name : `${dir}/${name}`;
}

/** The folder at `path` ("" = the root); null when it is missing or a file. */
export function findDir(root: DirNode, path: string): DirNode | null {
  let dir = root;
  for (const p of split(path)) {
    const next = dir.children.get(p);
    if (!next || next.kind !== "dir") return null;
    dir = next;
  }
  return dir;
}

function ensureDir(root: DirNode, parts: string[]): DirNode | null {
  let dir = root;
  for (const p of parts) {
    let next = dir.children.get(p);
    if (!next) {
      next = emptyDir(p);
      dir.children.set(p, next);
    }
    if (next.kind !== "dir") return null;
    dir = next;
  }
  return dir;
}

/** Folders first, then files; by name, numbers in order ("2" before "10"), case only breaking
 * ties so the order is stable. */
export function compareNodes(a: TreeNode, b: TreeNode): number {
  if (a.kind !== b.kind) return a.kind === "dir" ? -1 : 1;
  return COLLATOR.compare(a.name, b.name) || (a.name < b.name ? -1 : a.name > b.name ? 1 : 0);
}
const COLLATOR = new Intl.Collator("en", { numeric: true, sensitivity: "base" });

/** The folder's own entries, in display order. */
export function listDir(dir: DirNode, path: string): Entry[] {
  return [...dir.children.values()].sort(compareNodes).map((node) => ({ path: join(path, node.name), node }));
}

/** Folders first, then files; within each, by path one component at a time, as `compareNodes`
 * orders names. */
export function compareEntries(a: Entry, b: Entry): number {
  if (a.node.kind !== b.node.kind) return a.node.kind === "dir" ? -1 : 1;
  const pa = a.path.split("/");
  const pb = b.path.split("/");
  for (let i = 0; i < Math.min(pa.length, pb.length); i++) {
    const c = COLLATOR.compare(pa[i], pb[i]) || (pa[i] < pb[i] ? -1 : pa[i] > pb[i] ? 1 : 0);
    if (c !== 0) return c;
  }
  return pa.length - pb.length;
}

/**
 * Everything at or below the folder `path` whose path from that folder contains `query`
 * (case-insensitive): folders first, then files, each by path (`compareEntries`).
 */
export function search(dir: DirNode, path: string, query: string): Entry[] {
  const q = query.trim().toLowerCase();
  const out: Entry[] = [];
  if (q === "") return out;
  const walk = (d: DirNode, at: string, rel: string) => {
    for (const node of [...d.children.values()].sort(compareNodes)) {
      const r = rel === "" ? node.name : `${rel}/${node.name}`;
      if (r.toLowerCase().includes(q)) {
        out.push({ path: join(at, node.name), node });
      }
      if (node.kind === "dir") walk(node, join(at, node.name), r);
    }
  };
  walk(dir, path, "");
  return out.sort(compareEntries);
}

/**
 * Where keyboard focus goes after the explorer shows a folder: the row of `want` ("" = the
 * first row; a path not listed = the first row), or null when the folder has no rows, so the
 * grid itself takes focus and its keys (Backspace, Alt+Up) still work.
 */
export function focusIndex(entries: Entry[], want: string): number | null {
  if (entries.length === 0) return null;
  const i = want === "" ? 0 : entries.findIndex((e) => e.path === want);
  return Math.max(i, 0);
}

/** Files and bytes in a folder and everything under it. */
export function totals(dir: DirNode): { files: number; bytes: number } {
  let files = 0;
  let bytes = 0;
  const walk = (d: DirNode) => {
    for (const n of d.children.values()) {
      if (n.kind === "dir") walk(n);
      else {
        files++;
        bytes += n.size;
      }
    }
  };
  walk(dir);
  return { files, bytes };
}

/** The path's parent ("" for a top-level entry or the root). */
export function parentOf(path: string): string {
  const i = path.lastIndexOf("/");
  return i < 0 ? "" : path.slice(0, i);
}

// ---------- Pending changes ----------
// The Edit tab's changes fold over the inspected tree the way core's `edit.rs` folds `Edit`s
// (same order, same refusals); core's preflight stays the authority, this is the preview.

/** What an add brings in: one file, or a folder's files and empty folders (paths relative to
 * it; junk left out, as core's scanner does). `truncated`: the listing was cut short, so more
 * of the folder arrives on Apply than the preview shows. */
export type Payload =
  | { kind: "file"; size: number }
  | { kind: "dir"; files: { path: string; size: number }[]; emptyDirs: string[]; truncated?: boolean };

/** One pending change, as core's `Edit` (an add says what it brings, not where from). */
export type Change =
  | { op: "add"; to: string; what: Payload }
  | { op: "delete"; path: string }
  | { op: "rename"; from: string; to: string }
  | { op: "mkdir"; path: string };

/** Why a change does not apply (it then changes nothing), as core's preflight says it:
 * `invalid` not a game path, `missing` not in the game, `exists` already there, `file` a file is
 * in the way (the path or a parent), `folder` a folder is where a file goes, `into` a folder
 * moved into itself. */
export interface Finding {
  why: "invalid" | "missing" | "exists" | "file" | "folder" | "into";
  path: string;
}

export interface Folded {
  tree: DirNode;
  /** Per change, in order: why it does not apply (empty: it does). */
  findings: Finding[][];
}

/** A game path core takes: relative, `/`-separated, no empty, `.` or `..` component, no `\` or
 * NUL. */
export function validPath(path: string): boolean {
  return !/[\\\0]/.test(path) && path.split("/").every((c) => c !== "" && c !== "." && c !== "..");
}

/** One entry's name (Rename, New folder): a game path of one component. */
export function validName(name: string): boolean {
  return !name.includes("/") && validPath(name);
}

/** `origin`: a base entry's path before any change; `added`: put by an add. */
interface FState {
  size: number;
  origin?: string;
  added?: "new" | "replaced";
}
interface DState {
  origin?: string;
  host?: boolean;
}

/** Every proper ancestor of `path`, outermost first ("a/b/c": "a", "a/b"). */
function ancestors(path: string): string[] {
  const parts = path.split("/");
  const out: string[] = [];
  for (let i = 1; i < parts.length; i++) out.push(parts.slice(0, i).join("/"));
  return out;
}

const baseName = (path: string) => path.slice(path.lastIndexOf("/") + 1);

/** core's `State`: every file by game path, every folder (each file's and folder's parents too). */
class State {
  files = new Map<string, FState>();
  dirs = new Map<string, DState>();

  /** `dir` and its parents; `st` is merged into `dir`'s own state. */
  addDir(dir: string, st: DState = {}) {
    for (const a of ancestors(dir)) if (!this.dirs.has(a)) this.dirs.set(a, {});
    this.dirs.set(dir, { ...this.dirs.get(dir), ...st });
  }

  addFile(path: string, st: FState) {
    for (const a of ancestors(path)) if (!this.dirs.has(a)) this.dirs.set(a, {});
    this.files.set(path, st);
  }

  exists(path: string): boolean {
    return this.files.has(path) || this.dirs.has(path);
  }

  /** A file at a parent of `path` (or at `path` itself, with `itself`). */
  blocked(path: string, itself: boolean): Finding | null {
    const at = itself ? [...ancestors(path), path] : ancestors(path);
    const a = at.find((p) => this.files.has(p));
    return a === undefined ? null : { why: "file", path: a };
  }

  /** Everything under folder `dir`, taken out (`dir` itself too); paths relative to it. */
  takeDir(dir: string): { files: [string, FState][]; dirs: [string, DState][] } {
    const pre = `${dir}/`;
    const files: [string, FState][] = [...this.files].filter(([k]) => k.startsWith(pre)).map(([k, v]) => [k.slice(pre.length), v]);
    const dirs: [string, DState][] = [...this.dirs].filter(([k]) => k.startsWith(pre)).map(([k, v]) => [k.slice(pre.length), v]);
    for (const [k] of files) this.files.delete(pre + k);
    for (const [k] of dirs) this.dirs.delete(pre + k);
    this.dirs.delete(dir);
    return { files, dirs };
  }

  apply(c: Change): Finding[] {
    const paths = c.op === "rename" ? [c.from, c.to] : [c.op === "add" ? c.to : c.path];
    const bad = paths.filter((p) => !validPath(p)).map((path): Finding => ({ why: "invalid", path }));
    if (bad.length > 0) return bad;
    if (c.op === "delete") {
      if (this.files.delete(c.path)) return [];
      if (!this.dirs.has(c.path)) return [{ why: "missing", path: c.path }];
      this.takeDir(c.path);
      return [];
    }
    if (c.op === "mkdir") {
      const b = this.blocked(c.path, true);
      if (b) return [b];
      this.addDir(c.path);
      return [];
    }
    return c.op === "rename" ? this.rename(c.from, c.to) : this.add(c.to, c.what);
  }

  rename(from: string, to: string): Finding[] {
    if (!this.exists(from)) return [{ why: "missing", path: from }];
    if (this.exists(to)) return [{ why: "exists", path: to }];
    const b = this.blocked(to, false);
    if (b) return [b];
    const file = this.files.get(from);
    if (file) {
      this.files.delete(from);
      this.addFile(to, file);
      return [];
    }
    if (to.startsWith(`${from}/`)) return [{ why: "into", path: from }];
    const self = this.dirs.get(from) ?? {};
    const { files, dirs } = this.takeDir(from);
    this.addDir(to, self);
    for (const [d, st] of dirs) this.addDir(`${to}/${d}`, st);
    for (const [f, st] of files) this.addFile(`${to}/${f}`, st);
    return [];
  }

  add(to: string, what: Payload): Finding[] {
    const b = this.blocked(to, false);
    if (b) return [b];
    if (what.kind === "file") {
      if (this.dirs.has(to)) return [{ why: "folder", path: to }];
      this.put(to, what.size);
      return [];
    }
    if (this.files.has(to)) return [{ why: "file", path: to }];
    const targets = what.files.map((f) => ({ path: `${to}/${f.path}`, size: f.size }));
    const empty = what.emptyDirs.map((d) => `${to}/${d}`);
    const why: Finding[] = targets.filter((t) => this.dirs.has(t.path)).map((t) => ({ why: "folder", path: t.path }));
    for (const p of [...targets.map((t) => t.path), ...empty]) {
      const f = this.blocked(p, false);
      if (f) why.push(f);
    }
    for (const d of empty) {
      const f = this.blocked(d, true);
      if (f) why.push(f);
    }
    if (why.length > 0) {
      const seen = new Set<string>();
      return why
        .filter((f) => !seen.has(`${f.why}\0${f.path}`) && !!seen.add(`${f.why}\0${f.path}`))
        .sort((a, c) => (a.path < c.path ? -1 : a.path > c.path ? 1 : a.why < c.why ? -1 : 1));
    }
    this.addDir(to, what.truncated ? { host: true } : {});
    for (const d of empty) this.addDir(d);
    for (const t of targets) this.put(t.path, t.size);
    return [];
  }

  /** An added file at `path`: replacing a file that was there (unless that one was new too). */
  put(path: string, size: number) {
    const old = this.files.get(path);
    this.addFile(path, { size, added: old && old.added !== "new" ? "replaced" : "new" });
  }

  tree(): DirNode {
    const root = emptyDir("");
    const nodes = new Map<string, DirNode>([["", root]]);
    const originOf = (dir: string) => (dir === "" ? "" : this.dirs.get(dir)?.origin);
    // Moved: its parent was not its parent before (a new parent, or another one).
    const moved = (path: string, origin: string) => {
      const p = originOf(parentOf(path));
      return p === undefined || join(p, baseName(path)) !== origin;
    };
    const dirOf = (path: string): DirNode => {
      const known = nodes.get(path);
      if (known) return known;
      const parent = dirOf(parentOf(path));
      const node = emptyDir(baseName(path));
      const st = this.dirs.get(path) ?? {};
      if (st.origin === undefined) node.mark = "new";
      else if (moved(path, st.origin)) {
        node.mark = "renamed";
        node.from = st.origin;
      }
      if (st.host) node.host = true;
      nodes.set(path, node);
      if (!parent.children.has(node.name)) parent.children.set(node.name, node);
      return node;
    };
    for (const d of this.dirs.keys()) dirOf(d);
    for (const [path, st] of this.files) {
      const node: FileNode = { kind: "file", name: baseName(path), size: st.size };
      if (st.added) node.mark = st.added;
      else if (st.origin !== undefined && moved(path, st.origin)) {
        node.mark = "renamed";
        node.from = st.origin;
      }
      const parent = dirOf(parentOf(path));
      if (!parent.children.has(node.name)) parent.children.set(node.name, node);
    }
    return root;
  }
}

/** The game's tree with `changes` folded over it in order, as core folds them: a change that
 * does not apply changes nothing and says why. */
export function fold(files: { path: string; size: number }[], emptyDirs: string[], changes: readonly Change[]): Folded {
  const { state, findings } = replay(files, emptyDirs, changes);
  return { tree: state.tree(), findings };
}

function replay(files: { path: string; size: number }[], emptyDirs: string[], changes: readonly Change[]) {
  const s = new State();
  const base = (dir: string) => {
    for (const a of [...ancestors(dir), dir]) if (!s.dirs.has(a)) s.dirs.set(a, { origin: a });
  };
  for (const f of files) {
    for (const a of ancestors(f.path)) base(a);
    s.files.set(f.path, { size: f.size, origin: f.path });
  }
  for (const d of emptyDirs) base(d);
  const findings = changes.map((c) => s.apply(c));
  return { state: s, findings };
}

/** Names core's folder scanner leaves out (`scan.rs` `is_junk`): macOS, Windows and Spotlight
 * leftovers. */
export function isJunk(name: string): boolean {
  return name.startsWith("._") || JUNK.has(name.toLowerCase());
}
const JUNK = new Set([".ds_store", ".fseventsd", ".spotlight-v100", ".trashes", "thumbs.db", "desktop.ini", "system volume information"]);

/** A dropped folder's contents (paths relative to it) as core's scanner would read that folder:
 * junk left out, with what is inside it, and a folder left holding nothing listed as empty. */
export function withoutJunk(
  files: { path: string; size: number }[],
  emptyDirs: string[],
): { files: { path: string; size: number }[]; emptyDirs: string[] } {
  const junk = (p: string) => p.split("/").some(isJunk);
  const kept = files.filter((f) => !junk(f.path));
  const dirs = new Set<string>();
  for (const f of files) for (const a of ancestors(f.path)) if (!junk(a)) dirs.add(a);
  for (const d of emptyDirs) for (const a of [...ancestors(d), d]) if (!junk(a)) dirs.add(a);
  const busy = new Set<string>();
  for (const f of kept) for (const a of ancestors(f.path)) busy.add(a);
  for (const d of dirs) for (const a of ancestors(d)) busy.add(a);
  return { files: kept, emptyDirs: [...dirs].filter((d) => !busy.has(d)).sort() };
}

/** Something to add into a folder: its name, what it brings, and where it comes from (the
 * caller's, carried as it is). */
export interface AddItem<S> {
  name: string;
  what: Payload;
  src: S;
}

/** A change an add plan queues: core's shape plus the add's source; `only`: a folder of which
 * only the listed contents go in (some were skipped), not the whole folder. */
export type Planned<S> = Change & { src?: S; only?: boolean };

export interface AddPlan<S> {
  /** The paths the adds land on (relative to the folder), in order, once each: a file there that
   * would be replaced, or a file and a folder in each other's way. An earlier item of the same
   * batch counts like anything already there. */
  clashes: string[];
  /** The changes for an answer. Replace: a file over a file replaces it, one kind over the
   * other deletes the old first, a folder onto a folder merges. Skip: whatever clashes stays out
   * (of a folder, only those paths). */
  changes(answer: "replace" | "skip"): Planned<S>[];
}

/** Plans adding `items` (in order) into folder `dir` of the game after `changes`, folding each
 * as core will, so later items see earlier ones. */
export function planAdds<S>(
  files: { path: string; size: number }[],
  emptyDirs: string[],
  changes: readonly Change[],
  dir: string,
  items: AddItem<S>[],
): AddPlan<S> {
  const rel = (p: string) => (dir === "" ? p : p.slice(dir.length + 1));
  const run = (answer: "replace" | "skip") => {
    const s = replay(files, emptyDirs, changes).state;
    const clashes: string[] = [];
    const out: Planned<S>[] = [];
    // Queued even when it won't apply (a bad name): the pending list then says why.
    const emit = (c: Planned<S>) => {
      s.apply(c);
      out.push(c);
    };
    for (const it of items) {
      const to = join(dir, it.name);
      const kind: string[] = []; // the other kind is there: deleted first on Replace
      const over: string[] = []; // a file there: replaced
      const look = (p: string, file: boolean) => {
        const b = s.blocked(p, false);
        if (b) kind.push(b.path);
        if (file && s.dirs.has(p)) kind.push(p);
        else if (s.files.has(p)) (file ? over : kind).push(p);
      };
      if (it.what.kind === "file") look(to, true);
      else {
        look(to, false);
        for (const f of it.what.files) look(`${to}/${f.path}`, true);
        for (const d of it.what.emptyDirs) look(`${to}/${d}`, false);
      }
      const hit = [...new Set([...kind, ...over])];
      // A cut-short listing can't tell what a merge replaces: onto a folder there (or with any
      // clash) the folder itself is asked about, and Skip leaves the whole item out.
      if (it.what.kind === "dir" && it.what.truncated && (s.dirs.has(to) || hit.length > 0) && !hit.includes(to))
        hit.unshift(to);
      clashes.push(...hit.map(rel));
      if (hit.length > 0 && answer === "skip") {
        if (it.what.kind === "file" || hit.includes(to)) continue;
        const out_ = (p: string) => hit.some((h) => p === h || p.startsWith(`${h}/`));
        const keep = it.what.files.filter((f) => !out_(`${to}/${f.path}`));
        const empty = it.what.emptyDirs.filter((d) => !out_(`${to}/${d}`));
        if (keep.length === 0 && empty.length === 0) continue; // the folder is there already
        emit({ op: "add", to, what: { ...it.what, files: keep, emptyDirs: empty }, src: it.src, only: true });
        continue;
      }
      for (const k of new Set(kind)) emit({ op: "delete", path: k });
      emit({ op: "add", to, what: it.what, src: it.src });
    }
    return { clashes: [...new Set(clashes)], out };
  };
  return { clashes: run("replace").clashes, changes: (answer) => run(answer).out };
}

/** The pending changes: `gen` counts sources (a new source, or the same one read again, starts
 * a new generation with no changes), so an action that started for an older one queues
 * nothing. Each action's changes share a batch, which Undo takes back whole. */
export interface PendingState<C> {
  gen: number;
  next: number;
  changes: (C & { batch: number })[];
}
export type PendingAction<C> = { type: "reset" } | { type: "add"; gen: number; list: C[] } | { type: "undo" } | { type: "discard" };

export function pendingReducer<C>(s: PendingState<C>, a: PendingAction<C>): PendingState<C> {
  switch (a.type) {
    case "reset":
      return { gen: s.gen + 1, next: s.next, changes: [] };
    case "add":
      if (a.gen !== s.gen || a.list.length === 0) return s;
      return { ...s, next: s.next + 1, changes: [...s.changes, ...a.list.map((c) => ({ ...c, batch: s.next }))] };
    case "undo": {
      const last = s.changes[s.changes.length - 1]?.batch;
      return { ...s, changes: s.changes.filter((c) => c.batch !== last) };
    }
    case "discard":
      return { ...s, changes: [] };
  }
}

/** core's `Edit`, as an edit job takes it (api.ts's `Edit`; no import, so node can check this
 * file): an add names the path core reads. */
export type CoreEdit =
  | { op: "add"; from: string; to: string }
  | { op: "delete"; path: string }
  | { op: "rename"; from: string; to: string }
  | { op: "mkdir"; path: string };

const under = (p: string, q: string) => p === q || p.startsWith(`${q}/`);

/**
 * Core's edits for `changes`, in order. An add is one add of its source, except a folder that
 * goes in file by file (`perFile`: a browser's folder, uploaded one file at a time; or `only`:
 * some of it was skipped): one add per listed file and a new folder per empty one; such a
 * folder with nothing in it is a new folder, unless one is there already. `from(src, rel, to)`:
 * the path core reads (`rel`: the file inside a folder source, null for the source itself),
 * awaited one at a time in order (uploads run one by one).
 */
export async function toEdits<S>(
  files: { path: string; size: number }[],
  emptyDirs: string[],
  changes: readonly Planned<S>[],
  perFile: (src: S) => boolean,
  from: (src: S, rel: string | null, to: string) => Promise<string>,
): Promise<CoreEdit[]> {
  const out: CoreEdit[] = [];
  for (let i = 0; i < changes.length; i++) {
    const c = changes[i];
    if (c.op !== "add") {
      out.push(c.op === "rename" ? { op: c.op, from: c.from, to: c.to } : { op: c.op, path: c.path });
      continue;
    }
    if (c.src === undefined) throw new Error(`${c.to}: nothing to add`);
    const src = c.src;
    if (c.what.kind === "file" || !(c.only || perFile(src))) {
      out.push({ op: "add", from: await from(src, null, c.to), to: c.to });
      continue;
    }
    for (const f of c.what.files) {
      const to = `${c.to}/${f.path}`;
      out.push({ op: "add", from: await from(src, f.path, to), to });
    }
    for (const d of c.what.emptyDirs) out.push({ op: "mkdir", path: `${c.to}/${d}` });
    if (c.what.files.length === 0 && c.what.emptyDirs.length === 0) {
      if (!replay(files, emptyDirs, changes.slice(0, i)).state.dirs.has(c.to)) out.push({ op: "mkdir", path: c.to });
    }
  }
  return out;
}

/**
 * What can be checked before Apply when some adds come from files that exist only once they
 * are uploaded (`later`): the other changes, in order, when none of them depends on an upload
 * (touches a path an upload adds or replaces, its folders or anything inside it). When one does,
 * `changes` is null: nothing is checked until the uploads are in. `left`: the uploaded files.
 */
export function checkable<S>(changes: readonly Planned<S>[], later: (src: S) => boolean): { changes: Planned<S>[] | null; left: number } {
  const ups: string[] = []; // game paths uploads bring, so far
  const kept: Planned<S>[] = [];
  let left = 0;
  let dependent = false;
  const related = (p: string) => ups.some((u) => under(p, u) || under(u, p));
  for (const c of changes) {
    if (c.op === "add" && c.src !== undefined && later(c.src)) {
      ups.push(c.to);
      left += c.what.kind === "file" ? 1 : c.what.files.length;
      continue;
    }
    const paths = c.op === "rename" ? [c.from, c.to] : c.op === "add" ? [c.to] : [c.path];
    if (paths.some(related)) dependent = true;
    kept.push(c);
  }
  return { changes: dependent ? null : kept, left };
}

/** Where Apply stages each upload: `<tag>-<k>/<game path>` for the k-th, so no two staged paths
 * are the same or inside each other (a file `assets` then a folder holding `assets/x.bin`).
 * `tag` is new for each Apply, so two at once (another tab or viewer, the same source) never
 * replace each other's files. */
export function stagingNames(tag = `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 10)}`): (to: string) => string {
  let k = 0;
  return (to) => `${tag}-${++k}/${to}`;
}
