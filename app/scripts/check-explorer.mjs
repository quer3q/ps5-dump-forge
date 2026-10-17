// Checks src/explorer.ts with a plain `node scripts/check-explorer.mjs` (type stripping): the
// tree from an inspection's files and empty folders, folders-first natural order, a folder's
// lookup, the filter's walk (paths from the current folder, folders first), the keyboard focus target
// after a folder shows (none in an empty one: the grid takes it) and the totals.

import assert from "node:assert/strict";

const x = await import("../src/explorer.ts");

const files = [
  { path: "eboot.bin", size: 10 },
  { path: "sce_sys/param.json", size: 2 },
  { path: "sce_sys/icon0.png", size: 3 },
  { path: "data/file10.dat", size: 4 },
  { path: "data/file2.dat", size: 5 },
  { path: "data/Sub/deep.bin", size: 6 },
  { path: "Zeta.txt", size: 1 },
];
const root = x.buildTree(files, ["empty/inner", "data/Sub"]);

const names = (entries) => entries.map((e) => e.path);
assert.deepEqual(names(x.listDir(root, "")), ["data", "empty", "sce_sys", "eboot.bin", "Zeta.txt"]);
assert.deepEqual(names(x.listDir(x.findDir(root, "data"), "data")), ["data/Sub", "data/file2.dat", "data/file10.dat"]);
assert.deepEqual(names(x.listDir(x.findDir(root, "empty"), "empty")), ["empty/inner"]);
assert.equal(x.findDir(root, "empty/inner").children.size, 0);
assert.equal(x.findDir(root, "eboot.bin"), null);
assert.equal(x.findDir(root, "missing"), null);
assert.equal(x.findDir(root, ""), root);

// The filter: case-insensitive, matched on the path from the current folder, parents first.
assert.deepEqual(names(x.search(root, "", "SUB")), ["data/Sub", "data/Sub/deep.bin"]);
assert.deepEqual(names(x.search(x.findDir(root, "data"), "data", "data")), []);
assert.deepEqual(names(x.search(root, "", ".dat")), ["data/file2.dat", "data/file10.dat"]);
assert.deepEqual(names(x.search(root, "", "  ")), []);
assert.equal(x.search(root, "", "e").length, 10); // all but data, data/Sub

// Across folders: folders first, then files, each by path (not the walk's depth-first order);
// a folder matches through its path too (b/hit-dir/z).
{
  const t = x.buildTree([{ path: "a/hit.bin", size: 1 }, { path: "c/hit2.bin", size: 1 }], ["b/hit-dir", "b/hit-dir/z/hit-deep"]);
  assert.deepEqual(names(x.search(t, "", "hit")), ["b/hit-dir", "b/hit-dir/z", "b/hit-dir/z/hit-deep", "a/hit.bin", "c/hit2.bin"]);
}

// Keyboard focus after a folder shows: the asked-for row, else the first; none in an empty
// folder, so the grid itself takes focus (and Backspace / Alt+Up still go up).
{
  const empty = x.listDir(x.findDir(root, "empty/inner"), "empty/inner");
  assert.equal(x.focusIndex(empty, ""), null);
  assert.equal(x.focusIndex(empty, "empty/inner/gone"), null);
  const top = x.listDir(root, "");
  assert.equal(x.focusIndex(top, ""), 0);
  assert.equal(x.focusIndex(top, "sce_sys"), 2); // back up from sce_sys: its row
  assert.equal(x.focusIndex(top, "missing"), 0);
}

assert.deepEqual(x.totals(root), { files: 7, bytes: 31 });
assert.deepEqual(x.totals(x.findDir(root, "data")), { files: 3, bytes: 15 });
assert.equal(x.parentOf("data/Sub"), "data");
assert.equal(x.parentOf("data"), "");
assert.equal(x.join("", "a"), "a");
assert.equal(x.join("a", "b"), "a/b");

// ---------- The pending changes' fold: the same rules and order as core's edit.rs ----------
// Its base (`base()` in edit.rs's tests): four files and an empty folder.
const base = ["data/a.bin", "data/sub/b.bin", "eboot.bin", "sce_sys/param.json"].map((path) => ({ path, size: path.length }));
const folded = (changes) => x.fold(base, ["empty"], changes);
/** Every file and empty folder of a folded tree, as core lists them (files, then empty dirs). */
function listing(tree) {
  const files = [];
  const empty = [];
  const walk = (d, at) => {
    if (at !== "" && d.children.size === 0) empty.push(at);
    for (const n of d.children.values()) {
      const p = x.join(at, n.name);
      if (n.kind === "dir") walk(n, p);
      else files.push(p);
    }
  };
  walk(tree, "");
  return { files: files.sort(), empty: empty.sort() };
}
const node = (tree, path) => x.findDir(tree, x.parentOf(path))?.children.get(path.split("/").pop());
const ok = (f) => assert.deepEqual(f.findings.map((w) => w.length), f.findings.map(() => 0), JSON.stringify(f.findings));
const del = (path) => ({ op: "delete", path });
const ren = (from, to) => ({ op: "rename", from, to });
const mk = (path) => ({ op: "mkdir", path });
const add = (to, what) => ({ op: "add", to, what });
const file = (size) => ({ kind: "file", size });

// delete_rename_and_mkdir_fold_in_order
{
  const f = folded([ren("data/a.bin", "data/c.bin"), del("eboot.bin"), mk("x/y"), ren("data", "stuff")]);
  ok(f);
  assert.deepEqual(listing(f.tree), { files: ["sce_sys/param.json", "stuff/c.bin", "stuff/sub/b.bin"], empty: ["empty", "x/y"] });
  // Marks: the moved folder is Renamed (its contents moved with it, no badge), the renamed file
  // inside too; new folders are New; untouched entries have none.
  assert.equal(node(f.tree, "stuff").mark, "renamed");
  assert.equal(node(f.tree, "stuff").from, "data");
  assert.equal(node(f.tree, "stuff/sub").mark, undefined);
  assert.equal(node(f.tree, "stuff/sub/b.bin").mark, undefined);
  assert.equal(node(f.tree, "stuff/c.bin").mark, "renamed");
  assert.equal(node(f.tree, "stuff/c.bin").from, "data/a.bin");
  assert.equal(node(f.tree, "x").mark, "new");
  assert.equal(node(f.tree, "x/y").mark, "new");
  assert.equal(node(f.tree, "sce_sys/param.json").mark, undefined);
  assert.equal(node(f.tree, "empty").mark, undefined);
}
// Renamed back to where it was: no badge.
{
  const f = folded([ren("data", "d2"), ren("d2", "data"), ren("eboot.bin", "e"), ren("e", "eboot.bin")]);
  ok(f);
  assert.equal(node(f.tree, "data").mark, undefined);
  assert.equal(node(f.tree, "eboot.bin").mark, undefined);
}
// A file moved into a new folder is Renamed (its parent wasn't its parent).
{
  const f = folded([mk("n"), ren("eboot.bin", "n/eboot.bin")]);
  ok(f);
  assert.equal(node(f.tree, "n/eboot.bin").mark, "renamed");
  assert.equal(node(f.tree, "n/eboot.bin").from, "eboot.bin");
}

// deleting_a_folder_or_its_last_file_leaves_the_parent_empty
{
  assert.deepEqual(listing(folded([del("data/sub/b.bin")]).tree).empty, ["data/sub", "empty"]);
  let f = folded([del("data/a.bin"), del("data/sub")]);
  ok(f);
  assert.deepEqual(listing(f.tree), { files: ["eboot.bin", "sce_sys/param.json"], empty: ["data", "empty"] });
  f = folded([del("data"), del("empty")]);
  ok(f);
  assert.deepEqual(listing(f.tree), { files: ["eboot.bin", "sce_sys/param.json"], empty: [] });
}

// adds_a_file_new_or_over_one: a replaced file takes the new size; parents are made.
{
  const f = folded([add("sce_sys/param.json", file(10)), add("new/deep/x.bin", file(7))]);
  ok(f);
  assert.ok(listing(f.tree).files.includes("new/deep/x.bin"));
  assert.equal(node(f.tree, "sce_sys/param.json").size, 10);
  assert.equal(node(f.tree, "sce_sys/param.json").mark, "replaced");
  assert.equal(node(f.tree, "new/deep/x.bin").mark, "new");
  assert.equal(node(f.tree, "new/deep/x.bin").size, 7);
  assert.equal(node(f.tree, "new").mark, "new");
  // A new file put over again stays New; a replaced one stays Replaced, also once renamed.
  const g = folded([add("n.bin", file(1)), add("n.bin", file(2)), add("eboot.bin", file(3)), ren("eboot.bin", "e.bin")]);
  ok(g);
  assert.equal(node(g.tree, "n.bin").mark, "new");
  assert.equal(node(g.tree, "n.bin").size, 2);
  assert.equal(node(g.tree, "e.bin").mark, "replaced");
  // Deleted, then added again: Replaced? No: nothing was there any more, so New.
  const h = folded([del("eboot.bin"), add("eboot.bin", file(4))]);
  ok(h);
  assert.equal(node(h.tree, "eboot.bin").mark, "new");
}

// a_folder_onto_a_folder_merges (contents known: a browser drop, junk already left out).
{
  const host = { kind: "dir", files: [{ path: "a.bin", size: 8 }, { path: "new.bin", size: 3 }, { path: "deep/c.bin", size: 1 }], emptyDirs: ["e"] };
  const f = folded([add("data", host), add("copy", host)]);
  ok(f);
  assert.deepEqual(listing(f.tree), {
    files: ["copy/a.bin", "copy/deep/c.bin", "copy/new.bin", "data/a.bin", "data/deep/c.bin", "data/new.bin", "data/sub/b.bin", "eboot.bin", "sce_sys/param.json"],
    empty: ["copy/e", "data/e", "empty"],
  });
  assert.equal(node(f.tree, "data/a.bin").mark, "replaced");
  assert.equal(node(f.tree, "data/a.bin").size, 8);
  assert.equal(node(f.tree, "data/new.bin").mark, "new");
  assert.equal(node(f.tree, "data/sub/b.bin").mark, undefined);
  assert.equal(node(f.tree, "data").mark, undefined);
  assert.equal(node(f.tree, "copy").mark, "new");
}
// A listing cut short: the folder is marked (the rest arrives on Apply).
{
  const f = folded([add("big", { kind: "dir", files: [{ path: "x.bin", size: 1 }], emptyDirs: [], truncated: true })]);
  ok(f);
  assert.equal(node(f.tree, "big").host, true);
  assert.equal(node(f.tree, "big/x.bin").mark, "new");
}
// mkdir of a folder already there changes nothing and is fine (core: add_dir).
{
  const f = folded([mk("data"), mk("data/sub")]);
  ok(f);
  assert.equal(node(f.tree, "data").mark, undefined);
}

// every_offender_is_listed (the paths core finds the same way; host-side refusals are core's).
{
  const one = { kind: "dir", files: [{ path: "a.bin", size: 1 }], emptyDirs: [] };
  const changes = [
    del(""),
    del("/data"),
    mk("a//b"),
    mk("./a"),
    ren("data/..", "x"),
    ren("data", "a\\b"),
    del("missing"),
    ren("missing", "x"),
    ren("data", "eboot.bin"),
    ren("data", "data/sub/inner"),
    add("data", file(1)),
    add("eboot.bin", one),
    mk("eboot.bin/x"),
    add("eboot.bin/x", file(1)),
  ];
  const f = folded(changes);
  assert.deepEqual(f.findings, [
    [{ why: "invalid", path: "" }],
    [{ why: "invalid", path: "/data" }],
    [{ why: "invalid", path: "a//b" }],
    [{ why: "invalid", path: "./a" }],
    [{ why: "invalid", path: "data/.." }],
    [{ why: "invalid", path: "a\\b" }],
    [{ why: "missing", path: "missing" }],
    [{ why: "missing", path: "missing" }],
    [{ why: "exists", path: "eboot.bin" }],
    [{ why: "into", path: "data" }],
    [{ why: "folder", path: "data" }],
    [{ why: "file", path: "eboot.bin" }],
    [{ why: "file", path: "eboot.bin" }],
    [{ why: "file", path: "eboot.bin" }],
  ]);
  // A refused change changed nothing.
  assert.deepEqual(listing(f.tree), listing(folded([]).tree));
  // Every offender inside a folder add, sorted and once each; then nothing of it is added.
  const g = folded([
    add("data", { kind: "dir", files: [{ path: "sub", size: 1 }, { path: "a.bin/x", size: 1 }, { path: "ok.bin", size: 1 }], emptyDirs: ["a.bin/e"] }),
  ]);
  assert.deepEqual(g.findings, [[{ why: "file", path: "data/a.bin" }, { why: "folder", path: "data/sub" }]]);
  assert.equal(node(g.tree, "data/ok.bin"), undefined);
}
// Order matters: a rename frees the name a later add takes; Undo is folding one change fewer.
{
  const changes = [ren("eboot.bin", "old.bin"), add("eboot.bin", file(5)), del("data"), mk("data")];
  const f = folded(changes);
  ok(f);
  assert.equal(node(f.tree, "eboot.bin").mark, "new");
  assert.equal(node(f.tree, "old.bin").mark, "renamed");
  assert.equal(node(f.tree, "data").mark, "new");
  assert.equal(node(f.tree, "data").children.size, 0);
  const u = folded(changes.slice(0, 3));
  assert.equal(node(u.tree, "data"), undefined);
}

// Names and paths as core takes them.
assert.ok(x.validPath("a/b.bin"));
for (const p of ["", "/a", "a/", "a//b", ".", "a/..", "a\\b", "a\0b"]) assert.ok(!x.validPath(p), p);
assert.ok(x.validName("New folder"));
assert.ok(!x.validName("a/b"));

// A dropped folder's junk (core's scanner leaves it out): its folder, then empty, is listed.
assert.deepEqual(
  x.withoutJunk(
    [{ path: "a.bin", size: 1 }, { path: ".DS_Store", size: 1 }, { path: "j/._x", size: 1 }, { path: ".fseventsd/f", size: 1 }, { path: "k/Thumbs.db", size: 1 }, { path: "k/m/n.bin", size: 1 }],
    ["e", ".Trashes/z"],
  ),
  { files: [{ path: "a.bin", size: 1 }, { path: "k/m/n.bin", size: 1 }], emptyDirs: ["e", "j"] },
);

// ---------- Adding a batch: conflicts against what is there and the batch so far ----------
const plan = (changes, dir, items) => x.planAdds(base, ["empty"], changes, dir, items);
const item = (name, what) => ({ name, what, src: name });
const dirOf = (files, emptyDirs = []) => ({ kind: "dir", files: files.map((p) => ({ path: p, size: 9 })), emptyDirs });
const ops = (cs) => cs.map((c) => (c.op === "add" ? `add ${c.to}${c.only ? " only " + c.what.files.map((f) => f.path).join(",") : ""}` : `${c.op} ${c.path}`));
// A host folder (its listing from host_tree) folds like core's folder add: its files are
// browsable, renamable and deletable, replaced ones badged with the new size.
{
  const host = dirOf(["a.bin", "new.bin"]);
  const p = plan([], "", [item("data", host)]);
  assert.deepEqual(p.clashes, ["data/a.bin"]); // only the file that is there, not the folder
  let cs = p.changes("replace");
  assert.deepEqual(ops(cs), ["add data"]);
  let f = folded([...cs, ren("data/new.bin", "data/n2.bin"), del("data/a.bin")]);
  ok(f);
  assert.equal(node(f.tree, "data/n2.bin").mark, "new");
  assert.equal(node(f.tree, "data/a.bin"), undefined);
  f = folded(cs);
  assert.equal(node(f.tree, "data/a.bin").mark, "replaced");
  assert.equal(node(f.tree, "data/a.bin").size, 9);
  // Skip leaves out only the colliding file; the rest of the folder still goes in.
  cs = p.changes("skip");
  assert.deepEqual(ops(cs), ["add data only new.bin"]);
  f = folded(cs);
  ok(f);
  assert.equal(node(f.tree, "data/a.bin").mark, undefined);
  assert.equal(node(f.tree, "data/new.bin").mark, "new");
}
// A truncated host folder (host_tree cut short): a merge onto a folder there is always asked
// about, Replace merges the whole folder, Skip leaves the whole item out (never an `only` add).
{
  const host = { ...dirOf(["new.bin"]), truncated: true }; // only an unlisted file may clash
  let p = plan([], "", [item("data", host)]);
  assert.deepEqual(p.clashes, ["data"]);
  assert.deepEqual(ops(p.changes("replace")), ["add data"]);
  assert.deepEqual(ops(p.changes("skip")), []);
  p = plan([], "", [item("data", { ...dirOf(["a.bin", "new.bin"]), truncated: true })]);
  assert.deepEqual(p.clashes, ["data", "data/a.bin"]);
  assert.deepEqual(ops(p.changes("skip")), []);
  // A truncated folder onto nothing: no question, added whole.
  p = plan([], "", [item("fresh", host)]);
  assert.deepEqual(p.clashes, []);
  assert.deepEqual(ops(p.changes("skip")), ["add fresh"]);
}
// Two items with the same name in one batch: the second clashes with the first.
{
  const p = plan([], "data", [item("x.bin", file(1)), item("x.bin", file(2))]);
  assert.deepEqual(p.clashes, ["x.bin"]);
  assert.deepEqual(ops(p.changes("replace")), ["add data/x.bin", "add data/x.bin"]);
  assert.deepEqual(ops(p.changes("skip")), ["add data/x.bin"]);
  assert.equal(node(folded(p.changes("skip")).tree, "data/x.bin").size, 1);
  assert.equal(node(folded(p.changes("replace")).tree, "data/x.bin").size, 2);
}
// No clash: nothing to ask, every item added.
{
  const p = plan([], "", [item("n1", file(1)), item("n2", dirOf(["q"]))]);
  assert.deepEqual(p.clashes, []);
  assert.deepEqual(ops(p.changes("replace")), ["add n1", "add n2"]);
}
// One kind over the other: Replace deletes the old first; Skip leaves the item out.
{
  const p = plan([], "", [item("data", file(3)), item("eboot.bin", dirOf(["x"]))]);
  assert.deepEqual(p.clashes, ["data", "eboot.bin"]);
  const cs = p.changes("replace");
  assert.deepEqual(ops(cs), ["delete data", "add data", "delete eboot.bin", "add eboot.bin"]);
  const f = folded(cs);
  ok(f);
  assert.equal(node(f.tree, "data").kind, "file");
  assert.equal(node(f.tree, "eboot.bin/x").mark, "new");
  assert.deepEqual(ops(p.changes("skip")), []);
}
// Inside a folder: a file where the game has a folder, a folder path through a game file.
{
  const p = plan([], "", [item("data", dirOf(["sub", "a.bin/deep", "ok.bin"]))]);
  assert.deepEqual(p.clashes, ["data/sub", "data/a.bin"]);
  const cs = p.changes("replace");
  assert.deepEqual(ops(cs), ["delete data/sub", "delete data/a.bin", "add data"]);
  ok(folded(cs));
  assert.deepEqual(ops(p.changes("skip")), ["add data only ok.bin"]);
  ok(folded(p.changes("skip")));
}
// Against changes already pending: a renamed-away name is free, a new one is taken.
{
  const pend = [ren("eboot.bin", "old.bin"), add("n.bin", file(1))];
  const p = plan(pend, "", [item("eboot.bin", file(5)), item("n.bin", file(6))]);
  assert.deepEqual(p.clashes, ["n.bin"]);
}
// A name with a backslash (a macOS or Linux file name, as paths.basename keeps it) is queued and
// refused, never turned into another name.
{
  const paths = await import("../src/paths.ts");
  const name = paths.basename("/tmp/assets\\part.bin");
  assert.equal(name, "assets\\part.bin");
  const cs = plan([], "", [item(name, file(1))]).changes("replace");
  assert.deepEqual(folded(cs).findings, [[{ why: "invalid", path: "assets\\part.bin" }]]);
}

// ---------- The pending list: batches and generations ----------
{
  let s = { gen: 0, next: 0, changes: [] };
  const r = (a) => (s = x.pendingReducer(s, a));
  r({ type: "add", gen: 0, list: [mk("a")] });
  r({ type: "add", gen: 0, list: [add("b", file(1)), add("c", file(1))] });
  assert.deepEqual(s.changes.map((c) => c.batch), [0, 1, 1]);
  r({ type: "undo" });
  assert.deepEqual(s.changes.map((c) => c.op), ["mkdir"]);
  // A drop that began for game A, resolved after game B was chosen, queues nothing.
  const started = s.gen;
  r({ type: "reset" });
  assert.deepEqual(s.changes, []);
  const before = s;
  r({ type: "add", gen: started, list: [add("late.bin", file(1))] });
  assert.equal(s, before);
  r({ type: "add", gen: s.gen, list: [add("now.bin", file(1))] });
  assert.equal(s.changes.length, 1);
  r({ type: "add", gen: s.gen, list: [] });
  assert.equal(s.changes.length, 1);
  r({ type: "discard" });
  assert.deepEqual(s.changes, []);
}

// Core's edits: a whole host folder is one add; a folder that goes in file by file (a browser's,
// or one of which some was skipped) is one add per file and a new folder per empty one; an empty
// one is a new folder unless one is there already. Sources are read one at a time, in order.
{
  const seen = [];
  const from = async (src, rel, to) => {
    seen.push(to);
    return rel === null ? `${src.p}` : `${src.p}/${rel}`;
  };
  const perFile = (src) => src.browser === true;
  const changes = [
    del("eboot.bin"),
    { ...add("eboot.bin", file(3)), src: { p: "/h/eboot.bin" } },
    { ...add("data/Host", dirOf(["x.bin"], ["e"])), src: { p: "/h/Host" } },
    { ...add("data/Skip", dirOf(["k.bin"])), src: { p: "/h/Skip" }, only: true },
    { ...add("Up", dirOf(["a/b.bin", "c.bin"], ["z"])), src: { p: "up", browser: true } },
    { ...add("Hollow", dirOf([])), src: { p: "up2", browser: true } },
    { ...add("empty", dirOf([])), src: { p: "up3", browser: true } },
    ren("data/a.bin", "data/b.bin"),
    mk("n/m"),
  ];
  const edits = await x.toEdits(base, ["empty"], changes, perFile, from);
  assert.deepEqual(edits, [
    { op: "delete", path: "eboot.bin" },
    { op: "add", from: "/h/eboot.bin", to: "eboot.bin" },
    { op: "add", from: "/h/Host", to: "data/Host" },
    { op: "add", from: "/h/Skip/k.bin", to: "data/Skip/k.bin" },
    { op: "add", from: "up/a/b.bin", to: "Up/a/b.bin" },
    { op: "add", from: "up/c.bin", to: "Up/c.bin" },
    { op: "mkdir", path: "Up/z" },
    { op: "mkdir", path: "Hollow" },
    { op: "rename", from: "data/a.bin", to: "data/b.bin" },
    { op: "mkdir", path: "n/m" },
  ]);
  assert.deepEqual(seen, ["eboot.bin", "data/Host", "data/Skip/k.bin", "Up/a/b.bin", "Up/c.bin"]);
}

// Before Apply: uploads are left out when nothing else depends on them; a change on an
// uploaded path, a folder above it or anything inside it means nothing is checked before Apply.
{
  const later = (src) => src === "up";
  const up = (to, what = file(1)) => ({ ...add(to, what), src: "up" });
  const host = (to) => ({ ...add(to, file(1)), src: "host" });
  const free = x.checkable([up("data/new.bin"), up("Up", dirOf(["a.bin", "b.bin"])), host("host.bin"), ren("empty", "Empty"), del("eboot.bin"), mk("x/y")], later);
  assert.equal(free.left, 3);
  assert.deepEqual(free.changes.map((c) => c.op + " " + (c.to ?? c.path)), ["add host.bin", "rename Empty", "delete eboot.bin", "mkdir x/y"]);
  // A change before the upload doesn't depend on it.
  assert.notEqual(x.checkable([del("data/a.bin"), up("data/a.bin")], later).changes, null);
  // The review's sequence: replace old.bin by an upload, rename it, a folder where it was.
  const files = [{ path: "old.bin", size: 1 }, { path: "eboot.bin", size: 1 }];
  const seq = [up("old.bin"), ren("old.bin", "new.bin"), mk("old.bin")];
  assert.deepEqual(x.fold(files, [], seq).findings.map((f) => f.length), [0, 0, 0]);
  assert.deepEqual(x.checkable(seq, later), { changes: null, left: 1 });
  // Above it, inside it, onto it.
  assert.equal(x.checkable([up("data/new.bin"), ren("data", "Data")], later).changes, null);
  assert.equal(x.checkable([up("Up", dirOf(["a.bin"])), del("Up/a.bin")], later).changes, null);
  assert.equal(x.checkable([up("n.bin"), host("n.bin")], later).changes, null);
  assert.equal(x.checkable([up("n.bin"), ren("eboot.bin", "n.bin/z")], later).changes, null);
}

// Staging: every upload in its own namespace. The review's sequence: a file `assets`, deleted,
// then a folder holding `assets/x.bin`: no staged path is another's or inside another.
{
  const at = x.stagingNames("t");
  const staged = [];
  const changes = [
    { ...add("assets", file(1)), src: { browser: true } },
    del("assets"),
    { ...add("assets", dirOf(["x.bin", "y/z.bin"])), src: { browser: true } },
    { ...add("data/a.bin", file(1)), src: { browser: true } },
  ];
  const edits = await x.toEdits(base, ["empty"], changes, (src) => src.browser, async (_src, _rel, to) => {
    const p = at(to);
    staged.push(p);
    return `/stage/${p}`;
  });
  assert.deepEqual(staged, ["t-1/assets", "t-2/assets/x.bin", "t-3/assets/y/z.bin", "t-4/data/a.bin"]);
  for (const p of staged) for (const q of staged) if (p !== q) assert.ok(!q.startsWith(`${p}/`), `${q} inside ${p}`);
  assert.deepEqual(edits.map((e) => e.op + " " + (e.from ?? e.path) + (e.to ? " " + e.to : "")), [
    "add /stage/t-1/assets assets",
    "delete assets",
    "add /stage/t-2/assets/x.bin assets/x.bin",
    "add /stage/t-3/assets/y/z.bin assets/y/z.bin",
    "add /stage/t-4/data/a.bin data/a.bin",
  ]);
  // Two Applies at once (two tabs, one source): never the same staged path.
  const one = x.stagingNames()("data/a.bin");
  const two = x.stagingNames()("data/a.bin");
  assert.notEqual(one, two);
  assert.match(one, /^[a-z0-9]+-1\/data\/a\.bin$/);
}

console.log("check-explorer: ok");
