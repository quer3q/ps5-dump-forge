// English: the source language. Every other file in this folder is typed as `Messages` (this
// object's shape), so tsc fails on a missing key. Grouped by the file that shows the text.
// Stays English in every language: format ids, file names and paths, product names, IEC units.

import type { ReactNode } from "react";

import type { Format, KrakenLevel, Lz4Facts } from "../api";
import type { Rating, Row } from "../Formats";
import type { ClientText } from "../http-client";
import { dotted, num, plural } from "../i18n";
import type { Failure } from "../launcher";
import type { Lz4Action, Scenario, TraceAct } from "../Lz4";
import type { Note } from "../target";

/** A flat image holds the game's files as they are: about as large as the game. */
const FLAT = "Equal to the game size";

const en = {
  // App.tsx
  app: {
    address: (url: string) => `Address: ${url}. Show how to connect, with a QR code`,
    screens: "Screens",
    convert: "Convert",
    active: (n: number) => `, ${num(n)} active`,
    edit: "Edit",
    library: "Library",
    about: "About",
    offline: "Offline, retrying…",
    stop: "Stop Dump Forge",
    stillRunning: (n: number): string => (n === 1 ? "A job is still running" : `${num(n)} jobs are still running`),
    job: (id: number) => `Job ${id}`,
    quitNote: (web: boolean, n: number) =>
      `${web ? "Stopping Dump Forge" : "Quitting"} stops ${n === 1 ? "it" : "them"} and deletes the unfinished output.`,
    keepRunning: "Keep running",
    stopAndQuit: (n: number): string => (n === 1 ? "Stop job and quit" : "Stop jobs and quit"),
    stoppingJobs: "Stopping jobs and cleaning up…",
    stoppingForge: "Stopping Dump Forge…",
    couldntStop: "Couldn't stop Dump Forge",
    close: "Close",
    tryAgain: "Try again",
    stopped: "Dump Forge has stopped",
    startAgain: (ps5: boolean): string => (ps5 ? "To continue, open the Dump Forge tile on the PS5 again." : "To continue, run the same ps5-dump-forge serve command again."),
    /** Stop's question while no job runs (web: the payload; else the app, its jobs just finished). */
    confirmStop: (web: boolean): string => (web ? "Stop Dump Forge?" : "Quit PS5 Dump Forge?"),
    stopNote: (web: boolean, ps5: boolean): string => (!web ? "No job is running." : ps5 ? "To start it again, open the Dump Forge tile on the PS5." : "To start it again, run the same ps5-dump-forge serve command again."),
    quit: "Quit",
    github: (name: string) => `${name} on GitHub`,
    queued: "queued",
    progress: (percent: number, left: string | null) => `${percent}% done${left ? `, ${left} left` : ""}`,
  },
  // Convert.tsx
  convert: {
    target: "Target",
    targetFormat: "Target format",
    alreadyFolder: "the source is already a folder",
    lz4Backport:
      "This game's assets are LZ4 packed and its LZ4 manifest lists the backport: unpack it in the LZ4 tab first, then remove the backport here.",
    unreadable: "Can't build: this source couldn't be read (see Source).",
    noContentId: "Can't build as fpkg: this game's param.json has no content ID, which a package needs.",
    blocked: (first: string, more: number) =>
      `Can't build: ${first}${more > 0 ? ` (and ${num(more)} more, see Source)` : ""}.`,
    /** What the conversion does to an AMPR game's ampr_emu. `own` is the game's version:
     * for "keep", null when it is Forge's release runtime (the bundled one); for "replace", null
     * when it names none. */
    amprRuntime: (a: "install" | "trace" | "replace" | "keep", own: string | null, version: string) => {
      if (a === "install") return `This game uses AMPR but has no ampr_emu: the conversion installs the bundled ampr_emu ${version}.`;
      if (a === "trace") return `The conversion replaces the LZ4 trace runtime with the bundled release ampr_emu ${version}.`;
      if (a === "replace")
        return own
          ? `The conversion replaces this game's ampr_emu ${own} with the bundled ampr_emu ${version}.`
          : `The conversion replaces this game's ampr_emu, which names no version, with the bundled ampr_emu ${version}.`;
      return own
        ? `This game's ampr_emu ${own} is as new as the bundled ${version} or newer: the conversion keeps it.`
        : `This game already has the bundled ampr_emu ${version}: the conversion keeps it.`;
    },
    amprIndex: (state: "stale" | "damaged" | "missing", traces: boolean) =>
      `${state === "stale" ? "ampr_emu.index doesn't list this game's files" : state === "damaged" ? "ampr_emu.index can't be read" : "This game has no ampr_emu.index"}: the conversion writes a fresh one${traces ? " and leaves out the old traces" : ""}.`,
    /** A package's index: the builder always writes it afresh from what it packages. */
    amprPkgIndex: (traces: boolean) =>
      `The package gets a fresh ampr_emu.index built from the files it holds${traces ? ", without the old traces" : ""}.`,
    removeBackport: "Remove backport",
    /** What "Remove backport" leaves out and keeps: the emulators by name, and any other homebrew libraries. */
    removeHint: (libs: number, emulators: string[], other: boolean) =>
      `Leaves out ${num(libs)} backport ${plural(libs, { one: "library", other: "libraries" })} from fakelib/` +
      (emulators.length === 0
        ? other
          ? "; other homebrew libraries stay."
          : "."
        : `; ${emulators.length === 1 ? "the emulator" : "the emulators"} (${emulators.join(", ")})${other ? " and other homebrew libraries" : ""} ${emulators.length === 1 && !other ? "stays" : "stay"}.`),
    cantRemove: (why: string) => `Can't remove the backport: ${why}`,
    compare: "Compare formats",
    build: "Build",
    likelyFail: (fmt: string): ReactNode => (
      <>
        Likely to fail as <b>{fmt}</b>: it refuses file names listed in Source.
      </>
    ),
    collides: "A job is already writing this file: Build again would fail. Pick another name.",
    already: (fmt: string): ReactNode => (
      <>
        Already building this game as <b>{fmt}</b>; Build again makes a second copy.
      </>
    ),
  },
  // Formats.tsx
  formats: {
    rating: { good: "Good", ok: "Trade-off", bad: "Drawback" } satisfies Record<Rating, string>,
    columns: ["Console speed", "Size on disk"],
    rows: {
      pkg: {
        tagline: "Best, if your firmware allows",
        cells: ["Native, full speed", "30–60% less, depending on the game"],
      },
      lz4: {
        tagline: "Experimental, for games that use AMPR",
        cells: ["30–40% faster loading, upstream's numbers", "About half the game size, upstream's numbers"],
      },
      ffpkg: { tagline: "Recommended for ShadowMountPlus", cells: ["Full image speed", FLAT] },
      exfat: { tagline: "Compatibility", cells: ["Full image speed", FLAT] },
      ffpfs: { tagline: "Experimental", cells: ["Full image speed", FLAT] },
      ffpfsc: {
        tagline: "Smallest, experimental",
        cells: ["150–250 MB/s, may stutter", "Depends on the compression level, usually 30–60% less"],
      },
      folder: { tagline: "Plain game files", cells: ["Full drive speed", FLAT] },
    } satisfies Record<Format, Row>,
    /** About's feature list: name, then a few words. */
    features: [
      ["Convert", "between a folder, ffpkg, exfat, ffpfs, ffpfsc and fpkg, both ways."],
      ["Edit", "a game's files like a folder, and see its cover, firmware, backport, DLC and file name problems before you build."],
      ["Delete", "a game from the disk, after you confirm."],
      ["LZ4", "trace an AMPR game, then pack its assets with smart healing (experimental)."],
      ["Streamed", "from any source to any target: no temporary copy, not even the image inside an ffpfsc or a game in an archive."],
      ["Archives", "a .zip, .7z or .rar holding one game is browsed, converted or packed like a folder."],
      ["Library", "the games on this computer or console, ready to convert and edit."],
      ["Safe", "your game is never changed unless you ask; junk files are skipped and bad file names reported up front."],
      ["USB and SD cards", "written onto sticks and cards formatted exFAT or FAT32."],
      ["In place or rebuilt", "edits go straight into an image when they fit, else into a new image that replaces it once checked."],
    ] as [string, string][],
    howToConnect: "How to connect",
    connectText: (url: ReactNode) => (
      <>Open {url} in a browser on any device on your local network, or scan this code.</>
    ),
    close: "Close",
    about: "About",
    aboutText: "This tool can convert a game from any format to any other, on any platform, including PS5.",
    connectAny: "How to connect on any device",
    useAddress: (url: ReactNode) => <>Use {url} or scan this code.</>,
    compare: "Compare formats",
    compareLabel: "Format comparison",
    format: "Format",
    upstream: (app: string) =>
      `Upstream's numbers: what the authors of ampr_emu and Lazy_AMPR report, not measured by ${app}; they vary by game. ampr_emu 0.4.2.1 is an upstream test build; known issue: some games crash when saving.`,
    bundled: "LZ4 packed folders run on ampr_emu 0.4.2.1 by drakmor (GPL-3.0), bundled unmodified. Source:",
    lazy: "The trace-then-pack workflow follows Lazy_AMPR by Nazky:",
    unrar:
      ".rar archives are read with the bundled UnRAR library by Alexander Roshal (extract only, under the UnRAR license).",
    language: "Language",
    system: "System",
  },
  // Edit.tsx
  edit: {
    details: "Details",
    deleting: "Deleting the source…",
    delete: "Delete the source",
    deleted: "Deleted",
    filter: "Filter by path",
    filterLabel: "Filter files by path",
    firstOf: (max: number, n: number) => `first ${num(max)} of ${num(n)}`,
    fileCount: (n: number) => `${num(n)} ${plural(n, { one: "file", other: "files" })}`,
    noMatch: (f: string) => `No files match “${f}”.`,
    noListing: "Archive file listing is not supported.",
    noParam: "No sce_sys/param.json.",
    reader: "Reader",
    leftovers: "Leftover files and folders",
    found: (n: number) => `${num(n)} found`,
    leftoversNote: (app: string) =>
      `Unfinished outputs, usually from interrupted jobs. Remove only the ones you recognize, and only when no job is running; ${app} never deletes them itself.`,
    checked: (n: number) => `Folders checked (${num(n)})`,
    /** Edit's folds under the game card. */
    facts: "Game facts",
    explorer: "Game files",
    howTitle: "How this works",
    /** The empty state's paragraphs; `**…**` is bold. */
    how: (app: string): string[] => [
      "Open a **game folder** or **image**, then change it like a folder on a USB stick: **add and replace files** (drag them in, or use Add), **rename, delete**, make folders.",
      `Nothing changes **until you press Apply**: your changes stack up here, then ${app} writes them straight into the image or folder in one go and checks the result. It is **not a mounted drive**: ${app} keeps track of your changes and applies them itself.`,
      `When the changes fit, they go **straight into the image**; when they don't (no room left inside, or a format that can't take them), ${app} builds a new image beside it with your changes and swaps it in once it checks out. Either way, if something fails, the image is left as it was.`,
      "Archives can be browsed but not edited.",
    ],
    location: "Location in the game",
    root: "Game",
    up: "Up",
    upTip: "Up one folder",
    name: "Name",
    size: "Size",
    items: (n: number) => `${num(n)} ${plural(n, { one: "item", other: "items" })}`,
    inFolder: (dirs: number, files: number) =>
      `${num(dirs)} ${plural(dirs, { one: "folder", other: "folders" })}, ${num(files)} ${plural(files, { one: "file", other: "files" })}`,
    matches: (n: number) => `${num(n)} ${plural(n, { one: "match", other: "matches" })}`,
    emptyFolder: "This folder is empty.",
    open: (name: string) => `Open ${name}`,
    archiveNote: "Archives can be browsed but not edited. Convert it to a folder or an image first to change it.",
    /** The explorer's changes: none is made until Apply. */
    addFiles: "Add files",
    addFolder: "Add folder",
    pickAddFiles: "Choose files to add",
    pickAddFolder: "Choose a folder to add",
    nameLabel: "Name",
    renameItem: (name: string) => `Rename ${name}`,
    removeItem: (name: string) => `Delete ${name}`,
    renameTip: "Rename (F2)",
    removeTip: "Delete (Delete key)",
    /** Extract: copy files or folders out of the source into a folder chosen on this machine. */
    extract: "Extract",
    extractItem: (name: string) => `Extract ${name}`,
    extractTip: "Copy it to a folder you choose",
    extractFolder: "Extract folder",
    extractFolderTip: "Copy the folder shown to a folder you choose",
    extractGameTip: "Copy the whole game to a folder you choose",
    pickExtract: "Choose where to extract to",
    extractPending: "Extracts the files as they are now, without your pending changes",
    nameEmpty: "Type a name.",
    nameBad: "A name can't hold / or \\, or be . or ..",
    nameTaken: (name: string) => `${name} already exists here.`,
    markNew: "New",
    markReplaced: "Replaced",
    markRenamed: "Renamed",
    wasAt: (path: string) => `Was ${path}`,
    sizeLater: "More than could be listed: the rest is read when the changes are applied",
    dropInto: (folder: string) => `Drop to add to ${folder}`,
    pending: (n: number) =>
      plural(n, { one: `${num(n)} change not applied yet`, other: `${num(n)} changes not applied yet` }),
    undo: "Undo",
    undoTip: "Undo the last change",
    discard: "Discard all",
    discardTip: "Forget every change not applied yet",
    discardTitle: (n: number) =>
      plural(n, { one: `Discard ${num(n)} change?`, other: `Discard all ${num(n)} changes?` }),
    discardText: "They are forgotten, and nothing in the game changes. This can't be undone.",
    discardNo: "No",
    changesList: "Changes, in order",
    willNotApply: (n: number) =>
      plural(n, {
        one: `${num(n)} change won't apply as it is. Undo it, or discard all.`,
        other: `${num(n)} changes won't apply as they are. Undo them, or discard all.`,
      }),
    opAdd: (to: ReactNode): ReactNode => <>Add {to}</>,
    opAddFolder: (to: ReactNode): ReactNode => <>Add the folder {to}</>,
    opDelete: (path: ReactNode): ReactNode => <>Delete {path}</>,
    opRename: (from: ReactNode, to: ReactNode): ReactNode => <>Rename {from} to {to}</>,
    opMkdir: (path: ReactNode): ReactNode => <>New folder {path}</>,
    opFrom: (src: ReactNode): ReactNode => <>From {src}</>,
    whyInvalid: (path: ReactNode): ReactNode => <>{path} is not a valid game path.</>,
    whyMissing: (path: ReactNode): ReactNode => <>{path} is not in the game.</>,
    whyExists: (path: ReactNode): ReactNode => <>{path} already exists.</>,
    whyFile: (path: ReactNode): ReactNode => <>{path} is a file.</>,
    whyFolder: (path: ReactNode): ReactNode => <>{path} is a folder.</>,
    whyInto: (path: ReactNode): ReactNode => <>{path} can't move into itself.</>,
    conflictTitle: (n: number) =>
      plural(n, { one: `${num(n)} item already exists here`, other: `${num(n)} items already exist here` }),
    conflictIn: (folder: ReactNode): ReactNode => <>In {folder}:</>,
    conflictText:
      "Replace puts the new ones in their place: once the changes are applied, the old contents are gone for good, and a bad file can stop the game from booting. A folder added onto a folder is merged, its files with the same names replaced.",
    replace: "Replace",
    skip: "Skip",
    cancel: "Cancel",
    /** How the changes go in (the pending bar), from core's check before Apply. */
    modeLabel: "How to apply",
    checking: "Checking how the changes can be applied",
    checkFailed: "Couldn't check the changes:",
    checkAgain: "Check again",
    inPlace: "In place",
    rebuild: "Rebuild",
    rebuildNeeded: "Rebuild needed",
    cantApply: "Can't apply",
    changedDirectly: "Changed directly",
    folderText: "Written straight into the folder.",
    inPlaceText: "Written into the image.",
    inPlaceNeeds: (need: string, free: string) => `Written into the image; needs ${need} of its ${free} free.`,
    noRoom: (need: string, free: string) => `The changes need ${need}, the image has ${free} free.`,
    alwaysRebuilt: "ffpfsc and fpkg images are always rebuilt.",
    rebuildText: "A new image is written beside it and replaces it once verified.",
    rebuildNeeds: (need: string, free: string) =>
      `A new image is written beside it (about ${need} free needed, ${free} free) and replaces it once verified.`,
    uploadLater: (n: number) =>
      plural(n, {
        one: `${num(n)} file from this browser is checked once it is uploaded, when you press Apply.`,
        other: `${num(n)} files from this browser are checked once they are uploaded, when you press Apply.`,
      }),
    decidedOnApply: (n: number) =>
      plural(n, {
        one: `Other changes build on the file from this browser, so how they go in is decided once it is uploaded, when you press Apply.`,
        other: `Other changes build on the ${num(n)} files from this browser, so how they go in is decided once they are uploaded, when you press Apply.`,
      }),
    sourceLocked: "Changes are being applied to this game. Choose another source once the job ends.",
    notOpened: (path: ReactNode): ReactNode => <>{path} wasn't opened: changes are being applied to this game. Open it again once the job ends.</>,
    apply: "Apply",
    applyTip: "Apply every change listed here",
    applyTitle: (n: number) => plural(n, { one: `Apply ${num(n)} change?`, other: `Apply ${num(n)} changes?` }),
    applyInPlace: (name: ReactNode): ReactNode => <>They are written straight into {name}. There is no undo once it is done.</>,
    applyRebuild: (name: ReactNode): ReactNode => (
      <>A new image with your changes is written beside {name}, verified, then put in its place. There is no undo once it replaces the image.</>
    ),
    closeImage: "If the game is running from this image, close it first.",
    closeFolder: "If the game is running from this folder, close it first.",
    applyRestores: "A previous edit of this image was interrupted: the image is put back as it was first.",
    uploading: (i: number, n: number, sent: string, total: string) => `Uploading file ${num(i)} of ${num(n)}: ${sent} of ${total}`,
    uploadLabel: "Upload",
    uploadCancelled: "Upload cancelled. Nothing in the game changed.",
    applyRunning: "Applying the changes. Editing waits until the job below ends.",
    applyFailed: "The changes weren't applied:",
    applyRefused: "The changes can't be applied this way:",
    applyCancelled: "The job was cancelled. Your changes are still listed here.",
    folderPartly: "Part of them may already be in the folder: the files shown are what it holds now.",
    interrupted: "A previous edit of this image was interrupted. The image is put back as it was before your next changes are checked or applied.",
  },
  // JobList.tsx
  jobs: {
    jobs: "Jobs",
    pending: (n: number) => `${num(n)} running or queued`,
    allFinished: "all finished",
    /** Core's stage ids; one bar spans them all, the label says which pass is running. */
    stages: {
      scan: "Reading the source",
      preflight: "Checking",
      check: "Checking",
      plan: "Planning",
      compress: "Compressing",
      write: "Writing",
      measure: "Measuring packs",
      pack: "Packing",
      verify: "Verifying",
      finalize: "Finishing",
    },
    /** The job row's step list: short labels, one per stage id; ids with the same label next to
     * each other are one step (preflight and check). */
    steps: {
      scan: "Reading",
      preflight: "Checking",
      check: "Checking",
      plan: "Planning",
      compress: "Compressing",
      write: "Writing",
      measure: "Measuring",
      pack: "Packing",
      verify: "Verifying",
      finalize: "Finishing",
    },
    stepsLabel: "Steps",
    /** Screen readers: a step's state (the current one is `aria-current`). */
    stepDone: (s: string) => `${s}, done`,
    stepLater: (s: string) => `${s}, not started yet`,
    packingWriting: "Packing and writing",
    rate: { read: "read", compress: "compress", pack: "pack", write: "write" },
    showPath: "Show path",
    showFinder: "Show in Finder",
    showExplorer: "Show in Explorer",
    showFolder: "Show in folder",
    lz4: {
      pack: "LZ4 pack",
      unpack: "LZ4 unpack",
      trace: "LZ4 trace",
      unpatch: "LZ4 unpatch",
      runtime: "AMPR runtime",
    },
    /** An Edit tab job's tag: changes written into the source, or a rebuilt image swapped in. */
    edit: "Edit",
    editRebuild: "Edit · rebuild",
    extract: "Extract",
    job: (id: number) => `Job ${id}`,
    done: "Done",
    cancelled: "Cancelled",
    failed: "Failed",
    cancelling: "Cancelling…",
    queued: "Queued",
    dismiss: "Dismiss",
    stopJob: "Stop job?",
    cancel: "Cancel",
    stopHint: "Click Stop job? again within 4 seconds to stop it.",
    from: "from ",
    left: (d: string) => `${d} left`,
    inStep: (time: string) => `${time} in this step`,
    result: (bytes: string, n: number) => `${bytes}, ${num(n)} ${plural(n, { one: "file", other: "files" })} →`,
    outputPath: "Output path",
    checks: "Checks",
    verifiedPlain: (n: number) => `Verified: ${num(n)} ${plural(n, { one: "check", other: "checks" })} passed`,
    verifiedFull: (n: number) => `Full verification passed: ${num(n)} ${plural(n, { one: "check", other: "checks" })}`,
    verifiedFast: (n: number) => `Fast verification passed: ${num(n)} ${plural(n, { one: "check", other: "checks" })}`,
    coverage: (checked: string, total: string, n: number) =>
      `Checked ${checked} of ${total} in ${num(n)} ${plural(n, { one: "sample", other: "samples" })}`,
    cantBuild: "Can't build this game:",
    cantEdit: "Can't apply the changes:",
    cantExtract: "Can't extract:",
    cantBuildAs: (f: string) => (
      <>
        Can't build this game as <b>{f}</b>:
      </>
    ),
    log: (n: number) => `Log (${num(n)})`,
    logLabel: "Log",
  },
  // Library.tsx
  library: {
    pickTitle: "Choose a Library folder",
    scanningIn: (name: string) => `Scanning ${name}`,
    scanning: "Scanning",
    cantScanFolder: "Can’t scan this folder",
    chooseAnother: "Choose another folder",
    pickPrompt: "Pick a folder with your library to scan",
    choose: "Choose folder",
    cantScan: "Can’t scan the Library:",
    cantDelete: "Can’t delete:",
    deleted: (path: string) => `Deleted ${path}`,
    truncated: "Scan limit reached. This list may be incomplete.",
    limits: "Discovery stops at six folder levels and after about 15 seconds of scanning.",
    noRoots: "No folders are available to scan.",
    none: "No games found. Discovery checks up to six levels deep and skips system folders. Other sources can still be chosen in Edit or Convert.",
    skipped: "Skipped locations and files",
    stays: "Results stay here until you press Refresh. New outputs appear after refreshing.",
    library: "Library",
    storageTip: "Free space on the console's internal storage (/data), as of the last scan",
    free: (free: string, total: string) => (
      <>
        Free <b>{free}</b> of {total}
      </>
    ),
    refresh: "Refresh",
    cards: "Card view",
    list: "List view",
    games: "Games",
    edit: (n: string) => `Edit ${n}`,
    convert: (n: string) => `Convert ${n}`,
    remove: (n: string) => `Delete ${n}`,
    lz4Tip: "LZ4 packed: ampr_emu asset packs",
    lz4Packed: " packed",
  },
  // Lz4.tsx
  lz4: {
    title: "LZ4 asset packs",
    help: "How LZ4 works",
    scenario: "LZ4 scenario",
    scenarios: { trace: "Trace", pack: "Pack/Unpack" },
    actions: { patch: "Patch", unpatch: "Unpatch", pack: "Pack", unpack: "Unpack" },
    scenarioInfo: (s: Scenario, packed: boolean): string => s === "trace"
      ? "Records which assets the game reads while you play, for Pack to use."
      : packed
        ? "Writes the game's LZ4 packs back as plain files."
        : "Packs the game's assets into LZ4 volumes that ampr_emu reads while the game runs.",
    traceInfo: (a: TraceAct, image: boolean): string => a === "patch"
      ? image
        ? "Rebuilds this image with the trace runtime, then replaces it."
        : "Installs the trace runtime in this folder."
      : image
        ? "Rebuilds this image with the release runtime, then replaces it."
        : "Puts the release runtime back in this folder.",
    noAmpr: "the game doesn't use AMPR",
    noEboot: "there's no eboot.bin",
    packedFirst: "the source is LZ4 packed: unpack it first",
    damaged: "its LZ4 packs are damaged",
    archive: "an archive can't be traced: convert it first",
    chooseFirst: "choose a source first",
    notRead: "the source isn't read yet",
    pickProfile: "LZ4 profile",
    profileFilter: "LZ4 profile (TOML)",
    pickTracesFolder: "LZ4 traces (a folder with ampr_commands.bin and ampr_emu.index)",
    pickTracesEither: "LZ4 traces: the zip, or a folder with both files",
    pickTracesZip: "LZ4 traces (zip)",
    unreadable: "Can't start: this source couldn't be read (see Source).",
    pending: (a: TraceAct): string => a === "patch"
      ? "Can't patch while jobs are running or queued."
      : "Can't unpatch while jobs are running or queued.",
    replacing: "A job is already rebuilding this image: wait for it to finish.",
    blocked: (a: Lz4Action, first: string, more: number) =>
      `Can't ${a}: ${first}${more > 0 ? ` (and ${num(more)} more, see Source)` : ""}.`,
    chooseGame: "Choose a game folder or image first.",
    noAmprGame: "This game doesn't use AMPR, so LZ4 can't be used here.",
    additionalContent: "This is additional content (no eboot.bin), so LZ4 can't be used here.",
    installsRuntime: (a: TraceAct | "pack", version: string) =>
      a === "pack"
        ? `This game has no ampr_emu: Pack installs the bundled ampr_emu ${version}.`
        : a === "patch"
          ? `This game has no ampr_emu: Patch installs the bundled trace runtime (ampr_emu ${version}).`
          : `This game has no ampr_emu: Unpatch installs the bundled release runtime (ampr_emu ${version}).`,
    freshIndex: (state: "stale" | "damaged" | "missing", a: TraceAct | "pack") =>
      `${state === "stale" ? "ampr_emu.index doesn't list this game's files" : state === "damaged" ? "ampr_emu.index can't be read" : "This game has no ampr_emu.index"}: ${a === "pack" ? "Pack" : a === "patch" ? "Patch" : "Unpatch"} writes a fresh one.`,
    unavailable: (items: string[]) => `Not for this source: ${items.join("; ")}.`,
    readOnly: (kind: string): ReactNode => <>
      <b>{kind}</b> is read-only on the console, so it can't be traced. Convert it to <b>ffpkg</b> or <b>exfat</b> first.
    </>,
    action: "Action",
    traceAction: "Trace action",
    changes: (a: TraceAct, image: boolean) => a === "patch"
      ? `Changes this ${image ? "image" : "folder"}: the trace runtime replaces fakelib/libSceAmpr.sprx, with no backup.`
      : `Changes this ${image ? "image" : "folder"}: the release runtime comes back and the traces are deleted, with no backup.`,
    replay: (web: boolean) => `Play, then pick this source again to use its traces.${web ? " Trace also offers them as a zip." : ""}`,
    room: "Room for the trace",
    roomHint: "Free space for the game's trace. Mount the image with image_rw=, or nothing is recorded.",
    alreadyRelease: "Already on the release runtime: only the index is rewritten.",
    spaceHint: "Needs about the image's size of free space; the image is replaced only once the copy verifies.",
    deletesTraces: (a: TraceAct, bytes: string, web: boolean) => `${a === "patch" ? "Patch" : "Unpatch"} deletes this source's traces (${bytes}): ${web ? "download them first." : "copy them first."}`,
    useProfile: "Use a rules profile instead",
    profile: "Profile",
    choose: "Choose",
    profileHint: "A TOML file of pack rules, used instead of traces.",
    target: "Target",
    packInto: "Pack into",
    direct: "Packs straight into the image; the game's files are read twice.",
    targetFormat: "Target format",
    unpackFolder: "A new game folder with the packs written back as plain files.",
    unpackHint: (n: number | null) =>
      n === null
        ? "Writes the packed files back as plain files."
        : `Writes ${num(n)} packed ${plural(n, { one: "file", other: "files" })} back as plain files.`,
    likelyFail: (fmt: string): ReactNode => <>
      Likely to fail as <b>{fmt}</b>: it refuses file names listed in Source.
    </>,
    collides: (a: Lz4Action, folder: boolean) => `A job is already writing this ${folder ? "folder" : "file"}: ${a === "pack" ? "Pack" : "Unpack"} needs another name.`,
    already: (a: Lz4Action, fmt: string): ReactNode => <>
      Already writing this game as <b>{fmt}</b>; {a === "pack" ? "Pack" : "Unpack"} again makes a second copy.
    </>,
    profileRules: "Packs what the profile's rules say; files the engine opens directly stay loose.",
    chosenRules: "Packs the files the game read in these traces; files the engine opens directly stay loose.",
    ownRules: "Packs the files the game read in its last traced session; files the engine opens directly stay loose.",
    guessRules: "No traces: packs by a less reliable built-in guess. Trace first, or choose traces or a profile.",
    recorded: "Recorded traces",
    recordedHint: (bytes: string) => `ampr_commands.bin (${bytes}): Pack uses these traces.`,
    downloadHint: "Close the game and wait a minute, then download from a computer's browser. Pick the zip there in Pack/Unpack → Traces.",
    download: (bytes: string, zip: string) => `Download traces (${bytes}) as ${zip}`,
    missingIndex: "No ampr_emu.index in this source: Pack needs it beside ampr_commands.bin.",
    truncatedJournal: "A journal cut off at the end still packs, up to its last whole record.",
    fakelibWarning: (recorded: boolean): string => recorded
      ? "Few or no traces? Turn off the fakelib updater, Patch again and replay."
      : "Turn off the fakelib updater while tracing, or nothing is recorded.",
    emptyTitle: "Choose a game that uses AMPR",
    experimental: "Experimental: keep a copy of the original game.",
    chooseSource: "Choose source",
    source: "Source",
    traces: "Traces",
    optionalTraces: "Traces (optional)",
    unlock: "Unlock to choose other traces",
    loaded: "Traces are already loaded from this source",
    lockedHint: (bytes: string) => `This source's own traces (${bytes}). Click the lock to choose others.`,
    clear: "Clear",
    zip: "Zip",
    folder: "Folder",
    tracesHint: "Pick the amprtrace zip from Download traces, or a folder with both trace files.",
    useOwn: "Use the source's traces",
    patchResult: (a: TraceAct, n: number) => `${a === "patch" ? "Patched: the trace runtime is installed" : "Unpatched: the release runtime is back"}; the index lists ${num(n)} ${plural(n, { one: "file", other: "files" })}.`,
    removed: (files: string[]) =>
      `Removed the old trace ${plural(files.length, { one: "file", other: "files" })}: ${files.join(", ")}.`,
  },
  // Lz4Help.tsx
  lz4Help: {
    title: "How LZ4 works",
    close: "Close",
    warning: "Experimental, for AMPR games only. Keep a copy of the original game.",
    body: () => (
      <>
        <h3>1. Pack/Unpack</h3>
        <ol>
          <li>
            Choose the original game, <b>Pack/Unpack</b>, and the traces (or a profile).
          </li>
          <li>Write a packed folder or a plain image; a folder converts to any format in <b>Convert</b>.</li>
          <li>
            A packed source shows <b>Unpack</b> instead: the loose files come back.
          </li>
          <li>Pack keeps the game's ampr_emu if it is the bundled version or newer and can read LZ4 packs; otherwise it installs the bundled release one, also over the LZ4 trace runtime and in a game that has none.</li>
        </ol>

        <h3>2. Trace</h3>
        <ol>
          <li>
            Choose the game's <b>ffpkg</b> or <b>exfat</b>, then <b>Trace</b> → <b>Patch</b>.
          </li>
          <li>Turn off the fakelib updater: it overwrites the trace runtime.</li>
          <li>
            Mount it with <span className="path">image_rw=</span>, play one session, close the game.
          </li>
          <li>
            <b>Download traces</b> from the PS5 page, or copy ampr_commands.bin and ampr_emu.index.
          </li>
        </ol>
      </>
    ),
  },
  // Picker.tsx
  picker: {
    drives: "Drives",
    location: "Location",
    parent: "Parent folder",
    allDrives: "All drives",
    truncated: (n: number) =>
      plural(n, { one: "Only the first entry is shown.", other: `Only the first ${num(n)} entries are shown.` }),
    loading: "Loading",
    noDrives: "No drives found.",
    noFolders: "No folders here.",
    noImages: "No folders, PS5 images or archives here.",
    noFiles: (ext: string[]) => `No folders or ${dotted(ext, " or ")} files here.`,
    chooseFiles: (n: number) => plural(n, { one: `Choose ${num(n)} file`, other: `Choose ${num(n)} files` }),
    nothing: "No folders or files here.",
    cancel: "Cancel",
    chooseFolder: "Choose this folder",
  },
  // common.tsx
  common: {
    folder: "Folder",
    pickFolder: "Game folder",
    pickImage: "Game image or archive",
    pickImageFilter: "Images and archives",
    pickFolderButton: "Folder",
    pickImageButton: "Image/Archive",
    source: "Source",
    emptyTitle: "Choose a game folder, image or archive",
    emptyText: "A PS5 game folder or an image, package or archive of one.",
    accepted: "Accepted sources",
    unknownTitle: "Unknown title",
    additionalContent: "Additional content",
    reading: "Reading the source",
    unreadable: "Unreadable source",
    backport: "Backport",
    emulators: "Emulators",
    declares: (fw: string) => `declares ${fw}`,
    fwUnknown: "firmware unknown from its executables (no readable SDK)",
    emulatorList: (names: string) => `emulators: ${names}`,
    otherEmulator: "Other",
    sdk: (v: string) => `SDK ${v} (declared)`,
    titleId: "Title ID",
    version: "Version",
    firmware: "Firmware",
    fwDeclared: "declared",
    fwBackport: "backport",
    fwExecutables: "executables",
    size: "Size",
    files: (n: number) => `${num(n)} ${plural(n, { one: "file", other: "files" })}`,
    contentId: "Content ID",
    noAmpr: "eboot.bin does not import libSceAmpr",
    runtime: {
      forge_release: "Forge release",
      forge_trace: "Forge trace",
      other: "other",
      none: "none",
    } satisfies Record<Lz4Facts["runtime"], string>,
    packed: "Packed",
    damaged: "Damaged packs",
    traced: "Traced",
    plain: "Plain",
    packedDetail: (files: number, volumes: number, percent: number | null) =>
      `${num(files)} ${plural(files, { one: "file", other: "files" })} in ${num(volumes)} ${plural(volumes, { one: "volume", other: "volumes" })}` +
      (percent === null ? "" : `, ${num(percent)}% of size`),
    journal: (size: string) => `journal ${size}`,
    noJournal: "no journal yet",
    runtimeIs: (label: string) => `runtime ${label}`,
    index: {
      stale: "ampr_emu.index out of date: a conversion refreshes it",
      damaged: "ampr_emu.index damaged: a conversion rewrites it",
      missing: "no ampr_emu.index: a conversion writes one",
    } satisfies Record<"stale" | "damaged" | "missing", string>,
    /** What a conversion does to the runtime; for "keep", the game's own version. */
    runtimeAction: (a: "install" | "replace" | "keep", version: string) =>
      a === "install"
        ? `a conversion installs the bundled ampr_emu ${version}`
        : a === "replace"
          ? `a conversion replaces the runtime with the bundled ampr_emu ${version}`
          : `ampr_emu ${version}: a conversion keeps it`,
    backportLibs: (exclusive: boolean, n: number) =>
      `${exclusive ? "fakelib2/, mounted alone" : "fakelib/"}, ${num(n)} backport ${plural(n, { one: "library", other: "libraries" })}`,
    findings: "Preflight findings",
    dlcTitle: (n: number) => `Embedded DLC (${num(n)})`,
    dlcLabel: "Embedded DLC",
    dlcAll: (note: string) => `All ${note}.`,
    dlcIn: (folder: string, size: string) => `in ${folder}/, ${size}`,
    dlcNoData: "unlocked by the DLC emulator (dlc_emu.ini): entitlement only, no data of its own",
    dlcStatus: (status: string | null) =>
      `unlocked by the DLC emulator (dlc_emu.ini), status ${status || "not set"}`,
    dlcMerged: "merged into the game's folders",
    backportFiles: "Backport files",
    emulatorsIn: "Emulators in fakelib",
    qr: (url: string) => `QR code: ${url}`,
    needsPassword: "This archive needs a password.",
    enterPassword: "Enter password",
    inArchive: (ext: string, size: string) => `in a ${ext} archive · ${size}`,
    archive: "Archive",
    parts: (n: number) => `${num(n)} ${plural(n, { one: "part", other: "parts" })}`,
    archiveRoot: (root: string) => `game in ${root}`,
    archiveAtRoot: "game at the archive's root",
    solid: "solid",
    encrypted: "password-protected",
  },
  // delete.tsx
  delete: {
    title: (folder: boolean): string => (folder ? "Delete this folder?" : "Delete this image?"),
    text: (folder: boolean) =>
      `${folder ? "The folder and everything in it" : "The image file"} will be deleted for good: no Trash, no undo. If deleting stops part way, some of it may already be gone.`,
    no: "No",
    yes: "Yes, delete",
    archiveTitle: "Delete this archive?",
    archiveText:
      "The archive file will be deleted for good: no Trash, no undo. Other parts of a multi-part archive are left as they are.",
  },
  // password.tsx
  password: {
    title: "This archive needs a password",
    text: "Enter its password to open it.",
    wrong: "The password is not correct.",
    label: "Password",
    cancel: "Cancel",
    unlock: "Unlock",
    checking: "Checking…",
  },
  // main-http.tsx
  launch: {
    failure: {
      loader:
        "Can't reach the payload loader on port 9021. Run your autoloader (jailbreak) first, then open Dump Forge again.",
      missing:
        "No saved copy of Dump Forge was found (in /data/dumpforge/ or on a USB drive). Load it once from a PC; it saves a copy for this icon.",
      timeout:
        "Dump Forge was started but hasn't answered after 30 seconds. It may still be starting (a console waking from rest mode is slow).",
      stopping: "The previous Dump Forge is still stopping (cancelling its jobs and cleaning up). Wait a moment, then try again.",
    } satisfies Record<Failure, string>,
    loadFailed: (e: string) => `Couldn't load the page: ${e}`,
    noAnswer: "Dump Forge didn't answer",
    cantStart: "Can't start Dump Forge",
    retry: "Retry",
    starting: "Starting Dump Forge…",
    /** http-client.ts's own messages (it imports nothing at run time). */
    client: {
      unreachable:
        "Can't reach Dump Forge. Check that the payload is still running and this device is on the same network.",
      noPlatform: "Can't determine whether Dump Forge is running on a PS5. Refresh to try again.",
      quitUnreachable: "Can't reach Dump Forge to stop it.",
      stillAnswering: "Dump Forge is still answering after 2 minutes; check the PS5.",
      gone: "no longer reported by Dump Forge (dropped from its job history before its result was seen)",
    } satisfies Record<ClientText, string>,
  },
  // target.tsx
  target: {
    /** One line under a target picker: which one to pick, and when. */
    format: {
      folder: { text: "Unpack an image or package back into a plain game folder." },
      exfat: {
        text: "For games that misbehave as ffpkg and only run like external-drive content. Also opens on a Mac or PC.",
      },
      ffpkg: { tone: "good", text: "Recommended. The PS5's own file system (UFS2). Writable mounts possible." },
      ffpfs: {
        tone: "warn",
        text: "Experimental. The PS5's PFS file system, uncompressed. File names must be plain ASCII.",
      },
      ffpfsc: {
        tone: "warn",
        text: "Experimental. Smallest: a compressed container around one image, always mounted read-only.",
      },
      pkg: {
        text: "Installs like a store game and runs at native speed. Needs kstuff, fpkg-enable and ppr-patch.",
      },
      lz4: {
        tone: "warn",
        text: "Experimental. A game folder with its assets packed into LZ4 volumes, which the bundled AMPR emulator (ampr_emu) unpacks while the game runs. Only for games that use AMPR.",
      },
    } satisfies Record<Format, Note>,
    /** The inner `.ffpkg`'s line: the format's own, less what a read-only `.ffpfsc` doesn't give. */
    innerFfpkg: { tone: "good", text: "Recommended. The PS5's own file system (UFS2)." } as Note,
    level0: { text: "No compression: every block stored as it is. The file is as large as the image." } as Note,
    levelDefault: {
      tone: "good",
      text: "Recommended (zlib's default). 1 builds about 3× faster with a file about 3% larger; 9 is barely smaller.",
    } as Note,
    levelLow: {
      text: "Builds faster than 6 (level 1 about 3×); the file comes out larger (level 1 about 3%).",
    } as Note,
    levelHigh: {
      text: "Barely smaller than 6 (level 9 by 0.04%), but compressing takes longer (level 9 about 25%), with every core busy throughout.",
    } as Note,
    kraken: {
      fast: { label: "Fast", note: { tone: "good", text: "Recommended. The quickest build." } },
      balanced: {
        label: "Balanced",
        note: {
          text: "About 2.6% smaller than Fast, but compressing takes about 6× as long, with every core busy the whole time.",
        },
      },
      smallest: {
        label: "Smallest",
        note: {
          text: "About 2.7% smaller than Fast, but compressing takes about 9× as long, with every core busy the whole time.",
        },
      },
      stored: {
        label: "Stored",
        note: {
          text: "No compression: the package is about the game's size. Try it if a compressed package installs but won't start (reported on firmware 8.60 and 10.40).",
        },
      },
    } satisfies Record<KrakenLevel, { label: string; note: Note }>,
    pkgFirmware: "Works only on firmware 11.60 or lower.",
    imageInside: "Image inside",
    imageInsideLabel: "Image inside the ffpfsc",
    compressionLevel: "Compression level",
    compression: "Compression",
    fullVerify: "Full verification",
    fullVerifyHint: "Re-reads every byte to check the output. Takes longer.",
    separator: "A name can't contain a folder separator; use Change for the folder.",
    badName: "A name can't be blank, start with a dot, end with a dot or a space, or hold control characters.",
    tooLong: (bytes: number, max: number, folder: boolean) =>
      `Too long: ${bytes} bytes${folder ? "" : " before the extension"}, at most ${max} (UTF-8).`,
    pkgExt: (ext: string) => `Ends in .${ext}, but fpkg is saved as .pkg: the extension picks the driver.`,
    otherExt: (ext: string, target: string) =>
      `Ends in .${ext}, but the target is ${target}: the extension picks the driver.`,
    extractInto: "Extract into this folder",
    saveInto: "Save into this folder",
    output: "Output",
    change: "Change",
    naming: "Naming…",
    chooseSourceFirst: "Choose a source first",
    fileName: "File name",
    savedAs: (name: string, dir: string) => `saved as ${name} in ${dir}`,
    inDir: (dir: string) => `in ${dir}`,
    generate: "Generate name based on content",
    generatePattern: "[game name]-[title ID]",
  },
  // UpdateToast.tsx: a newer release (checked at every start)
  update: {
    available: (v: string) => `v${v} is available`,
    update: "Update",
    later: "Later",
    updating: "Updating…",
    updatingNote: "New version is downloading, the app will restart itself.",
    downloading: "Downloading in your browser: unpack the archive and replace this copy.",
    failed: (v: string) => `Couldn't update to v${v}`,
    timeout: (v: string) => `v${v} didn't start within a minute. If it was downloaded, stop Dump Forge and open it again to finish the update.`,
    retry: "Retry",
    close: "Close",
  },
  // api.ts: prettyDuration, whole per language (CJK writes 5分30秒, no spaces)
  duration: { s: (s: number) => `${s} s`, ms: (m: number, s: number) => `${m} min ${s} s`, hm: (h: number, m: number) => `${h} h ${m} min` },
};

export type Messages = typeof en;
export default en;
