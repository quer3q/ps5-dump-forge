// English: the source language. Every other file in this folder is typed as `Messages` (this
// object's shape), so tsc fails on a missing key. Grouped by the file that shows the text.
// Stays English in every language: format ids, file names and paths, product names, IEC units.

import type { ReactNode } from "react";

import type { Format, KrakenLevel, Lz4Facts, Lz4Runtime } from "../api";
import type { Features, Rating, Row } from "../Formats";
import type { ClientText } from "../http-client";
import { dotted, num, plural } from "../i18n";
import type { Failure } from "../launcher";
import type { Engine, PackAction, PackLevel } from "../Pack";
import type { Note } from "../target";

/** A flat image holds the game's files as they are: about as large as the game. */
const FLAT = "Equal to the game size";

const en = {
  // App.tsx
  app: {
    screens: "Screens",
    convert: "Convert",
    active: (n: number) => `, ${num(n)} active`,
    edit: "Edit",
    pack: "Pack",
    storage: "Storage",
    connect: "Connect",
    disconnected: "Disconnected",
    connected: (n: number) => plural(n, { one: `Connected (${num(n)} device)`, other: `Connected (${num(n)} devices)` }),
    about: "About",
    prevTab: "Previous tab (L2)",
    nextTab: "Next tab (R2)",
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
    quitting: "Quitting PS5 Dump Forge…",
    couldntStop: "Couldn't stop Dump Forge",
    close: "Close",
    tryAgain: "Try again",
    stopped: "Dump Forge has stopped",
    startAgain: (ps5: boolean): string => (ps5 ? "To continue, open the Dump Forge tile on the PS5 again." : "To continue, run the same ps5-dump-forge serve command again."),
    /** Stop's question while no job runs (web: the payload; else the app, its jobs just finished). */
    confirmStop: (web: boolean): string => (web ? "Stop Dump Forge?" : "Quit PS5 Dump Forge?"),
    stopNote: (web: boolean, ps5: boolean): string => (!web ? "No job is running." : ps5 ? "To start it again, open the Dump Forge tile on the PS5." : "To start it again, run the same ps5-dump-forge serve command again."),
    peerBusy: "A paired device is reading or writing files on this computer. Quitting stops its transfer.",
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
      "This game's assets are LZ4 packed and its LZ4 manifest lists the backport: unpack it in the Pack tab first, then remove the backport here.",
    zstdBackport:
      "This game's assets are zstd packed and its zstd index lists the backport: unpack it in the Pack tab first, then remove the backport here.",
    unreadable: "Can't build: this source couldn't be read (see Source).",
    patchBlocked: "Can't build: this is a patch package, which holds only what the update changed. Open it in Edit to extract its files.",
    openInEdit: "Open in Edit",
    noContentId: "Can't build as fpkg: this game's param.json has no content ID, which a package needs.",
    blocked: (first: string, more: number) =>
      `Can't build: ${first}${more > 0 ? ` (and ${num(more)} more, see Source)` : ""}.`,
    /** What the conversion does to an AMPR game's ampr_emu. `own` is the game's version:
     * for "keep", null when it is Forge's release runtime (the bundled one); for "replace", null
     * when it names none. "trace": Forge's old LZ4 trace runtime, said as a plain replace. */
    amprZstdRuntime: "This conversion installs ampr-emu-ng at fakelib/libSceAmpr.sprx, which these packs need.",
    /** A game whose runtime is ampr-emu-ng that the conversion replaces with ampr_emu. */
    amprNgReplaced: (version: string) => `Replaces this game's ampr-emu-ng with ampr_emu ${version}.`,
    amprRuntime: (a: "install" | "trace" | "replace" | "keep", own: string | null, version: string) => {
      if (a === "install") return `This game uses AMPR but has no ampr_emu: the conversion installs the bundled ampr_emu ${version}.`;
      if (a === "trace") return `The conversion replaces this game's ampr_emu with the bundled ampr_emu ${version}.`;
      if (a === "replace")
        return own
          ? `The conversion replaces this game's ampr_emu ${own} with the bundled ampr_emu ${version}.`
          : `The conversion replaces this game's ampr_emu, which names no version, with the bundled ampr_emu ${version}.`;
      return own
        ? `This game's ampr_emu ${own} is as new as the bundled ${version} or newer: the conversion keeps it.`
        : `This game already has the bundled ampr_emu ${version}: the conversion keeps it.`;
    },
    amprIndex: (state: "stale" | "damaged" | "missing") =>
      `${state === "stale" ? "ampr_emu.index doesn't list this game's files" : state === "damaged" ? "ampr_emu.index can't be read" : "This game has no ampr_emu.index"}: the conversion writes a fresh one.`,
    /** A package's index: the builder always writes it afresh from what it packages. */
    amprPkgIndex: "The package gets a fresh ampr_emu.index built from the files it holds.",
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
      zstd: { tagline: "For games that use AMPR, with ampr-emu-ng", cells: ["Up to 80% faster loading", "Up to half the game size"] },
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
    } satisfies Record<Format | "zstd", Row>,
    /** About's feature list: name, then a few words. */
    features: [
      ["Convert", "between a folder, ffpkg, exfat, ffpfs, ffpfsc and fpkg, both ways."],
      ["Transfer", "send files and convert games between your PS5, computers and a NAS over SMB, FTP or NFS."],
      ["Edit", "a game's files like a folder, and see its cover, firmware, backport, DLC and file name problems before you build."],
      ["Streamed", "from any source to any target: no temporary copy, not even the image inside an ffpfsc or a game in an archive."],
      ["Archives", "a .zip, .7z or .rar holding one game is browsed, converted or packed like a folder."],
      ["USB and SD cards", "written onto sticks and cards formatted exFAT or FAT32."],
      ["Pack", "zstd or LZ4 (experimental) asset packs with smart healing."],
      ["Storage", "a Library of your games and a File explorer to rename, move and delete."],
      ["Safe", "your game is never changed unless you ask; junk files are skipped and bad file names reported up front."],
    ] as Features,
    about: "About",
    compare: "Compare formats",
    compareLabel: "Format comparison",
    format: "Format",
    credits: "Credits",
    upstream: (app: string) =>
      `Upstream's numbers: what the authors of ampr_emu and Lazy_AMPR report, not measured by ${app}; they vary by game. ampr_emu 0.4.2.1 is an upstream test build; known issue: some games crash when saving.`,
    bundled: "LZ4 packed folders run on ampr_emu 0.4.2.1 by drakmor (GPL-3.0), bundled unmodified. Source:",
    lazy: "LZ4 packing builds on Lazy_AMPR by Nazky:",
    ng: "zstd and LZ4 packs run on ampr-emu-ng. Source:",
    logo: "The logo is by Xxcco:",
    unrar:
      ".rar archives are read with the bundled UnRAR library by Alexander Roshal (extract only, under the UnRAR license).",
    language: "Language",
    system: "System",
    notifications: "Notify when a job finishes",
    notificationsHint: "Shows on the console for Convert, Pack and Transfer jobs.",
    notificationsFailed: "Couldn't change notifications.",
  },
  // Edit.tsx
  edit: {
    details: "Details",
    deleting: "Deleting the source",
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
      "Open a **game folder** or **image**, then change it like a folder on a USB stick: **add and replace files** (drag them in, or use Add), **rename and delete**.",
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
    /** The row actions' column header (screen readers only). */
    actions: "Actions",
    items: (n: number) => `${num(n)} ${plural(n, { one: "item", other: "items" })}`,
    inFolder: (dirs: number, files: number) =>
      `${num(dirs)} ${plural(dirs, { one: "folder", other: "folders" })}, ${num(files)} ${plural(files, { one: "file", other: "files" })}`,
    matches: (n: number) => `${num(n)} ${plural(n, { one: "match", other: "matches" })}`,
    emptyFolder: "This folder is empty.",
    open: (name: string) => `Open ${name}`,
    archiveNote: "Archives can be browsed but not edited. Convert it to a folder or an image first to change it.",
    nasNote: "Games on a NAS can be browsed and extracted but not edited. Convert it or extract it to this device first to change it.",
    patchNote: "This is a patch package: it holds only what the update changed. The files it carries whole are listed; they can be browsed and extracted but not edited.",
    peerNote: "Games on another Forge can be browsed and extracted but not edited. Convert it or extract it to this device first to change it.",
    /** The explorer's changes: none is made until Apply. */
    addFiles: "Add files",
    addFolder: "Add folder",
    pickAddFiles: "Choose files to add",
    pickAddFolder: "Choose a folder to add",
    peerRoot: "A whole device can't be added: choose a folder inside it.",
    renameItem: (name: string) => `Rename ${name}`,
    removeItem: (name: string) => `Delete ${name}`,
    renameTip: "Rename (F2)",
    removeTip: "Delete (Delete key)",
    /** On a Mac the key marked delete is Backspace (Up a folder here). */
    removeTipMac: "Delete (fn+Delete)",
    /** Extract: copy files or folders out of the source into a folder chosen on this machine. */
    extractItem: (name: string) => `Extract ${name}`,
    extractTip: "Copy it to a folder you choose",
    extractFolder: "Extract folder",
    extractFolderTip: "Copy the folder shown to a folder you choose",
    extractGameTip: "Copy the whole game to a folder you choose",
    /** Extract folder's title while it is off: the folder shown is not on disk yet. */
    extractNotOnDisk: "This folder exists only in your pending changes: apply them to extract it.",
    pickExtract: "Choose where to extract to",
    extractPending: "Extracts the files as they are now, without your pending changes.",
    nameEmpty: "Type a name.",
    nameBad: "A name can't hold / or \\, or be . or ..",
    nameTaken: (name: string) => `${name} already exists here.`,
    markNew: "New",
    markReplaced: "Replaced",
    markRenamed: "Renamed",
    wasAt: (path: string) => `Was ${path}`,
    sizeLater: "More than could be listed: the rest is read when the changes are applied",
    dropInto: (folder: string) => `Drop to add to ${folder}`,
    droppedGone: (path: string) => `${path}: the dropped file is no longer there`,
    pending: (n: number) =>
      plural(n, { one: `${num(n)} change not applied yet`, other: `${num(n)} changes not applied yet` }),
    undo: "Undo",
    undoTip: "Undo the last action, with all its changes",
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
    nothingToChange: "Nothing to change: the game stays as it is.",
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
    uploadingFile: (i: number, n: number) => `Uploading file ${num(i)} of ${num(n)}`,
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
      install: "Installing",
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
      install: "Installing",
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
    /** A Pack tab job's tag; a zstd pack with its level. */
    packTag: {
      lz4Pack: (level: string) => `LZ4 pack · ${level}`,
      lz4Unpack: "LZ4 unpack",
      zstdPack: (level: string) => `zstd pack · ${level}`,
      level: (n: number) => `level ${num(n)}`,
      zstdUnpack: "zstd unpack",
    },
    /** An Edit tab job's tag: changes written into the source, or a rebuilt image swapped in. */
    edit: "Edit",
    editRebuild: "Edit · rebuild",
    /** A transfer job's tag (Connect's copy job). */
    transfer: "Transfer",
    extract: "Extract",
    job: (id: number) => `Job ${id}`,
    done: "Done",
    cancelled: "Cancelled",
    failed: "Failed",
    cancelling: "Cancelling…",
    queued: "Queued",
    dismiss: "Dismiss",
    retry: "Retry",
    retrying: "Starting",
    stopJob: "Stop job?",
    cancel: "Cancel",
    stopHint: "Press Stop job? again within 4 seconds to stop it.",
    remoteLabel: (name: string, device: string) => `${name}, a job on ${device}`,
    remoteTitle: (device: string) => `Runs on ${device} and uses this device's files: you can only cancel it here`,
    from: "from ",
    left: (d: string) => `${d} left`,
    inStep: (time: string) => `${time} in this step`,
    result: (bytes: string, n: number) => `${bytes}, ${num(n)} ${plural(n, { one: "file", other: "files" })} →`,
    outputPath: "Output path",
    sourceDeleted: (v: string) => `Source deleted: ${v}`,
    sourceKept: (v: string) => `Source kept: ${v}`,
    sourcePartlyDeleted: (v: string) => `Source partly deleted: ${v}`,
    checks: "Checks",
    verifiedPlain: (n: number) => `Verified: ${num(n)} ${plural(n, { one: "check", other: "checks" })} passed`,
    verifiedFull: (n: number) => `Full verification passed: ${num(n)} ${plural(n, { one: "check", other: "checks" })}`,
    verifiedFast: (n: number) => `Fast verification passed: ${num(n)} ${plural(n, { one: "check", other: "checks" })}`,
    coverage: (checked: string, total: string, n: number) =>
      `Checked ${checked} of ${total} in ${num(n)} ${plural(n, { one: "sample", other: "samples" })}`,
    cantBuild: "Can't build this game:",
    cantEdit: "Can't apply the changes:",
    cantExtract: "Can't extract:",
    cantTransfer: "Can't transfer:",
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
    cantScanFolder: "Can't scan this folder",
    chooseAnother: "Choose another folder",
    pickPrompt: "Choose a folder with your library to scan",
    choose: "Choose folder",
    cantScan: "Can't scan the Library:",
    cantDelete: "Can't delete:",
    deleted: (path: string) => `Deleted ${path}`,
    truncated: "Scan limit reached. This list may be incomplete.",
    limits: "The scan stops at six folder levels and after about 15 seconds of scanning.",
    noRoots: "No folders are available to scan.",
    none: "No games found. The scan checks up to six levels deep and skips system folders. Other sources can still be chosen in Edit or Convert.",
    skipped: "Skipped locations and files",
    library: "Library",
    storageTip: "Free space on the console's internal storage (/data)",
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
    deleting: (n: string) => `Deleting ${n}`,
    lz4Tip: "LZ4 asset packs",
    zstdTip: "zstd asset packs",
    showAs: "Show as",
    kinds: "Show kind",
    all: "All",
    lz4Chip: "LZ4",
    zstdChip: "zstd",
    countIn: (n: number, path: ReactNode) => (
      <>
        <b>{num(n)}</b> {plural(n, { one: "game in", other: "games in" })} {path}
      </>
    ),
  },
  // Storage.tsx, Files.tsx: the Storage tab (Library | File explorer)
  storage: {
    views: "Storage view",
    library: "Library",
    files: "File explorer",
    title: "File explorer",
    pickTitle: "Choose a folder to browse",
    pickPrompt: "Choose a folder to browse its files",
    choose: "Choose folder",
    roots: "Drives",
    location: "Location",
    folders: "Folders",
    up: "Up",
    upTip: "Up one folder",
    refresh: "Refresh",
    filter: "Filter by name",
    filterLabel: "Filter this folder by name",
    inFolder: (dirs: number, files: number) =>
      `${num(dirs)} ${dirs === 1 ? "folder" : "folders"}, ${num(files)} ${files === 1 ? "file" : "files"}`,
    matches: (n: number) => `${num(n)} ${n === 1 ? "match" : "matches"}`,
    firstOf: (max: number, n: number) => `first ${num(max)} of ${num(n)}`,
    truncated: "This folder holds more than 10,000 entries: only the first 10,000 are listed.",
    name: "Name",
    size: "Size",
    emptyFolder: "This folder is empty.",
    noMatch: (f: string) => `Nothing here matches “${f}”.`,
    noRoots: "No drives are available to browse.",
    cantList: "Can't open this folder:",
    link: "Link",
    linkTip: "A link: it can be opened, but not renamed, moved or deleted here",
    mounted: "Mounted",
    mountedTip: "Mounted by ShadowMount+: stop the game and turn ShadowMount+ off to delete, rename or move it",
    mountedNote: "Images tagged Mounted are in use by ShadowMount+: stop the game and turn ShadowMount+ off to delete, rename or move them.",
    open: (n: string) => `Open ${n}`,
    more: (n: string) => `More actions for ${n}`,
    rename: (n: string) => `Rename ${n}`,
    renameTip: "Rename (F2)",
    move: (n: string) => `Move ${n}`,
    moveTip: "Move to another folder",
    moveTitle: (n: string) => `Move ${n} to`,
    remove: (n: string) => `Delete ${n}`,
    removeTip: "Delete (Delete key)",
    /** On a Mac the key marked delete is Backspace (Up a folder here). */
    removeTipMac: "Delete (fn+Delete)",
    cantDelete: "Can't delete:",
    cantRename: "Can't rename:",
    cantMove: "Can't move:",
    sameFolder: "It is already in that folder.",
    deleted: (path: string) => `Deleted ${path}`,
    renamed: (path: string) => `Renamed to ${path}`,
    moved: (path: string) => `Moved to ${path}`,
    devices: "Devices",
    thisDevice: "This device",
    otherDevice: "Move to stays on one device: drag it onto another device's tab to copy it there.",
    dropCopy: (folder: string) => `Drop to copy into ${folder}`,
    dropMove: (folder: string) => `Drop to move into ${folder}`,
    cantCopy: "Can't copy:",
    copyNoFolder: "Open a folder on that device first.",
    copyOne: (name: string, to: string) => `Copying ${name} to ${to}. The transfer is listed under Connect.`,
    copyMany: (n: number, to: string) =>
      plural(n, {
        one: `Copying ${num(n)} item to ${to}. The transfer is listed under Connect.`,
        other: `Copying ${num(n)} items to ${to}. The transfer is listed under Connect.`,
      }),
    openConnect: "Open Connect",
  },
  // Pack.tsx
  pack: {
    title: "Pack",
    help: "How pack works",
    engine: "Kind of packs",
    engines: { zstd: "zstd", lz4: "LZ4" } satisfies Record<Engine, string>,
    actions: { pack: "Pack", unpack: "Unpack" } satisfies Record<PackAction, string>,
    info: (e: Engine, packed: boolean): string =>
      e === "zstd"
        ? packed
          ? "Writes the zstd packs back as plain files."
          : "Packs the game's assets into zstd volumes."
        : packed
          ? "Writes the LZ4 packs back as plain files."
          : "Some games may not start. Packs the game's assets into LZ4 volumes.",
    noAmprGame: (e: Engine) => `This game doesn't use AMPR, so ${e === "zstd" ? "zstd" : "LZ4"} packs can't be used.`,
    patch: "A patch package can't be packed.",
    additionalContent: "Additional content (no eboot.bin) can't be packed.",
    unreadable: "Can't start: the source couldn't be read (see Source).",
    blocked: (a: PackAction, first: string, more: number) =>
      `Can't ${a}: ${first}${more > 0 ? ` (and ${num(more)} more, see Source)` : ""}.`,
    target: "Target",
    packFolder: (e: Engine): string => `Recommended. Fastest, lowest latency: a new game folder, its assets in ${e === "zstd" ? "zstd" : "LZ4"} packs.`,
    levels: { fast: "Fast", medium: "Medium", max: "Max" } satisfies Record<PackLevel, string>,
    levelNotes: {
      zstd: {
        fast: { tone: "good", text: "Recommended. zstd 5: quick, good size." },
        medium: { text: "zstd 9: smaller, slower." },
        max: { text: "zstd 19: smallest; much slower, more memory." },
      },
      lz4: {
        fast: { tone: "good", text: "Recommended. LZ4 default: quickest." },
        medium: { text: "LZ4 HC 9: smaller, slower." },
        max: { text: "LZ4 HC 12: smallest, slowest." },
      },
    } satisfies Record<Engine, Record<PackLevel, Note>>,
    runtime: "Runtime",
    runtimeNotes: {
      ng: { tone: "good", text: "Recommended. Reads zstd and LZ4 packs." },
      ampr_emu: { text: "Upstream ampr_emu." },
    } satisfies Record<Lz4Runtime, Note>,
    ngNote: "Needs ampr-emu-ng: Pack installs it in fakelib/. Turn off ShadowMount+'s fakelib update, or it replaces the runtime.",
    installsRuntime: (version: string) => `No ampr_emu here: Pack installs ampr_emu ${version}.`,
    freshIndex: (state: "stale" | "damaged" | "missing") =>
      `${state === "stale" ? "ampr_emu.index is out of date" : state === "damaged" ? "ampr_emu.index is damaged" : "No ampr_emu.index"}: Pack writes a fresh one.`,
    direct: "Packs straight into the image; reads the game twice.",
    unpackFolder: "A new game folder with plain files.",
    unpackHint: (n: number | null) =>
      n === null
        ? "Writes the packed files back as plain files."
        : `Writes ${num(n)} packed ${plural(n, { one: "file", other: "files" })} back as plain files.`,
    likelyFail: (fmt: string): ReactNode => <>
      Likely to fail as <b>{fmt}</b>: it refuses file names listed in Source.
    </>,
    collides: (a: PackAction, folder: boolean) => `A job is already writing this ${folder ? "folder" : "file"}: ${a === "pack" ? "Pack" : "Unpack"} needs another name.`,
    already: (a: PackAction, fmt: string): ReactNode => <>
      Already writing this game as <b>{fmt}</b>; {a === "pack" ? "Pack" : "Unpack"} again makes a second copy.
    </>,
    source: "Source",
  },
  // PackHelp.tsx
  packHelp: {
    title: "How pack works",
    close: "Close",
    body: () => (
      <>
        <h3>zstd</h3>
        <ol>
          <li>Packs the game's assets into zstd volumes, into a folder or an image. Only for games that use AMPR.</li>
          <li><b>Fast</b> (zstd 5) is quick; <b>Medium</b> (9) and <b>Max</b> (19) are smaller, slower.</li>
          <li>A packed game shows <b>Unpack</b>: the plain files come back.</li>
        </ol>

        <h3>LZ4</h3>
        <ol>
          <li><b>Some games may not start.</b> Keep a copy.</li>
          <li>Packs into LZ4 volumes, into a folder or an image: <b>Fast</b> (LZ4), <b>Medium</b> (HC 9), <b>Max</b> (HC 12).</li>
          <li>Runtime: <b>ampr-emu-ng</b> (default) or upstream <b>ampr_emu</b>.</li>
          <li>A packed game shows <b>Unpack</b>.</li>
        </ol>

        <h3>ampr-emu-ng</h3>
        <ol>
          <li>Reads zstd and LZ4 packs; Pack installs it in fakelib/.</li>
          <li>Turn off ShadowMount+'s fakelib update, or it replaces the runtime and the game can't read its packs.</li>
        </ol>
      </>
    ),
  },
  // Picker.tsx
  picker: {
    devices: "Devices",
    thisDevice: "This device",
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
    chooseFiles: (n: number) => (n === 0 ? "Choose files" : plural(n, { one: `Choose ${num(n)} file`, other: `Choose ${num(n)} files` })),
    nothing: "No folders or files here.",
    cantList: "Can't open this folder:",
    cancel: "Cancel",
    chooseFolder: "Choose this folder",
  },
  // common.tsx
  common: {
    folder: "Folder",
    pickButton: "Pick",
    pickAny: "Game folder, image or archive",
    source: "Source",
    emptyTitle: "Choose a game folder, image or archive",
    accepted: "Accepted sources",
    unknownTitle: "Unknown title",
    patch: "Patch",
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
      forge_trace: "old LZ4 trace runtime",
      zstd: "ampr-emu-ng",
      other: "other",
      none: "none",
    } satisfies Record<Lz4Facts["runtime"], string>,
    packed: "Packed",
    zstdKept: "zstd asset packs: a conversion keeps them",
    damaged: "Damaged packs",
    plain: "Plain",
    packedDetail: (files: number, volumes: number, percent: number | null) =>
      `${num(files)} ${plural(files, { one: "file", other: "files" })} in ${num(volumes)} ${plural(volumes, { one: "volume", other: "volumes" })}` +
      (percent === null ? "" : `, ${num(percent)}% of size`),
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
    text: (folder: boolean): string =>
      folder
        ? "The folder and everything in it will be deleted for good: no Trash, no undo. If deleting stops part way, some of it may already be gone."
        : "The image file will be deleted for good: no Trash, no undo.",
    no: "No",
    yes: "Yes, delete",
    archiveTitle: "Delete this archive?",
    archiveText:
      "The archive file will be deleted for good: no Trash, no undo. Other parts of a multi-part archive are left as they are.",
    fileTitle: "Delete this file?",
    fileText: "The file will be deleted for good: no Trash, no undo.",
  },
  // replace.tsx
  replace: {
    title: (n: number) =>
      plural(n, { one: `${num(n)} item already exists there`, other: `${num(n)} items already exist there` }),
    inFolder: (folder: ReactNode): ReactNode => <>In {folder}:</>,
    more: (n: number) => `and ${num(n)} more`,
    text: "Replace puts the new ones in their place: the old files are gone for good, no Trash, no undo. A folder onto a folder of the same name is merged: its files with the same names are replaced, everything else there stays.",
    cancel: "Cancel",
    replace: "Replace",
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
      uploadIncomplete: "The upload didn't complete.",
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
      ffpkg: { text: "The PS5's own file system (UFS2). Writable mounts possible." },
      ffpfs: {
        text: "The PS5's PFS file system, uncompressed. File names must be plain ASCII.",
      },
      ffpfsc: {
        text: "Smallest: a compressed container around one image, always mounted read-only.",
      },
      pkg: { tone: "good", text: "Recommended. Runs like a native game, but only if your firmware allows it." },
      lz4: {
        tone: "warn",
        text: "Experimental. A game folder with its assets packed into LZ4 volumes, which the bundled AMPR emulator (ampr_emu) unpacks while the game runs. Only for games that use AMPR.",
      },
    } satisfies Record<Format, Note>,
    /** The inner `.ffpkg`'s line: the format's own, less what a read-only `.ffpfsc` doesn't give. */
    innerFfpkg: { text: "The PS5's own file system (UFS2)." } as Note,
    innerExfat: { tone: "good", text: "Recommended. The most stable image inside a compressed container." } as Note,
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
    imageInside: "Image inside",
    imageInsideLabel: "Image inside the ffpfsc",
    compressionLevel: "Compression level",
    compression: "Compression",
    fullVerify: "Full verification",
    fullVerifyHint: "Re-reads every byte to check the output. Takes longer.",
    deleteSource: "Delete the source",
    deleteSourceHint: "No undo. The source is deleted only after a successful conversion.",
    deleteSourceNas: "A game on a NAS can't be deleted from here.",
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
    updatingNote: "The new version is downloading. This page reloads into it by itself.",
    downloading: "Downloading in your browser: unpack the archive and replace this copy.",
    failed: (v: string) => `Couldn't update to v${v}`,
    timeout: (v: string) => `v${v} didn't start within a minute. If it was downloaded, stop Dump Forge and open it again to finish the update.`,
    retry: "Retry",
    close: "Close",
  },
  // Connect.tsx, ConnectHelp.tsx: pairing, Autodiscovery, saved devices
  connect: {
    thisDevice: "This device",
    howItWorks: "How to connect",
    ipAddress: "IP address",
    seen: (n: number, unit: "now" | "m" | "h" | "d") =>
      unit === "m"
        ? plural(n, { one: `seen: ${num(n)} minute ago`, other: `seen: ${num(n)} minutes ago` })
        : unit === "h"
          ? plural(n, { one: `seen: ${num(n)} hour ago`, other: `seen: ${num(n)} hours ago` })
          : unit === "d"
            ? plural(n, { one: `seen: ${num(n)} day ago`, other: `seen: ${num(n)} days ago` })
            : "seen: just now",
    listenError: (e: string) => `Other devices can't reach this one: ${e}`,
    unavailable: (e: string) => `Connect isn't available: ${e}`,
    loading: "Loading",
    discover: "Autodiscovery",
    stop: "Stop",
    fileTransfer: "File Transfer",
    devices: "Devices",
    show: "Show",
    all: "All",
    onlineOf: (n: number, total: number) => `${num(n)} of ${num(total)} online`,
    sendFilesTo: (name: string) => `Send files to ${name}`,
    details: (name: string) => `Details of ${name}`,
    userIs: (user: string) => `user ${user}`,
    nearby: "Nearby",
    nearbyNone: "No new devices found",
    addDevice: "Add a device",
    addressExample: "192.168.1.20 or nas.local",
    addByProtocol: "Or add by protocol:",
    savedEmpty: "No saved devices yet",
    savedEmptyHint:
      "Autodiscovery finds Forge and network storage on this network. Pair a device and allow it there, or save a NAS.",
    online: "Online",
    offline: "Offline",
    computer: "Computer",
    address: "Address",
    keyId: "Key ID",
    lastSeen: "Last seen",
    status: "Status",
    never: "Never",
    unknown: "Unknown",
    forget: "Forget",
    forgetNote: "Forget removes the pairing on this device only. To use it again, pair it again.",
    close: "Close",
    cancel: "Cancel",
    pairTitle: "Pair a device",
    pair: "Pair",
    connectTo: "Connect",
    probing: (address: string) => `Checking ${address}`,
    pairing: "Pairing",
    /** The Pair dialog while the other device's user decides. */
    waitingFor: (address: string) => `Waiting for ${address} to allow the pairing`,
    checkNumber: "Check number",
    checkHint: "It must match the number the other device shows.",
    denied: "The other device denied the pairing.",
    timedOut: "Nobody allowed the pairing on the other device in time.",
    pairBusy: "The other device has other pairing requests waiting. Try again in a minute.",
    alreadyPaired: "That device already has this one saved. Forget it there first, then pair again.",
    tooManyAttempts: "Too many refused requests. Try again in 10 minutes.",
    /** The prompt on the device asked to pair. */
    pairPromptTitle: (name: string) => `Pair with ${name}?`,
    pairPromptBody: (name: string, platform: string, ip: string) =>
      `${name} (${platform}, ${ip}) wants to pair with this device. A paired device can read this device's files and add new ones.`,
    pairPromptCheck: (name: string) => `Allow it only if ${name} shows the same check number.`,
    allow: "Allow",
    deny: "Deny",
    notPaired: "The other device doesn't know this one any more. Pair it again.",
    searching: "Searching this network",
    forge: "Forge",
    save: "Save",
    kindNas: "NAS",
    nasAddTitle: (protocol: string) => `Add an ${protocol} NAS`,
    nasEditTitle: (protocol: string) => `Edit ${protocol} NAS`,
    nasName: "Name",
    nasAddressHint: "IP address or host name; add :port when it isn't 445",
    nasUser: "User name",
    nasUserHint: "Leave empty to sign in as a guest",
    ftpAddressHint: "IP address or host name; add :port when it isn't 21",
    nfsAddressHint: "IP address or host name; add :port when it isn't 2049",
    ftpUserHint: "Leave empty to sign in anonymously",
    ftpNote: "FTP is not encrypted, the login included.",
    nfsNote: "NFS has no login: Forge connects as root (NFSv4.1, else NFSv3), so the server's export must allow this device. NFS is not encrypted.",
    nasPassword: "Password",
    nasPasswordKeep: "Leave empty to keep the saved password",
    guest: "Guest",
    passwordSaved: "Saved",
    passwordNone: "None",
    edit: "Edit",
    nasForgetNote: "Forget removes this NAS and its saved password from this device.",
    pickSendFiles: "Files to send",
    pickSendFolder: "Folder to send",
    pickSaveTo: "Where to save it",
    thisBrowser: "This browser",
    browserHint: "Files from this phone or computer go into a folder on this device, and a file from this device is saved by this browser.",
    uploadTo: "Folder to upload into",
    uploadPick: "Choose files to upload",
    uploading: (name: string, i: number, n: number) => `Uploading ${name} (${num(i)} of ${num(n)})`,
    uploaded: "Upload finished",
    /** Files not uploaded: a folder or link of the same name is in the folder chosen. */
    uploadSkipped: (names: string) => `Not uploaded, a folder or link of that name is there: ${names}.`,
    uploadedBefore: (done: number, n: number) =>
      plural(done, {
        one: `${num(done)} of ${num(n)} files was uploaded and stays on this device.`,
        other: `${num(done)} of ${num(n)} files were uploaded and stay on this device.`,
      }),
    cantStart: "Can't start the transfer:",
    cantUpload: "Can't upload:",
    pickDownload: "File to download",
    transferIntro: "Send files or a folder from any device into a folder here, on a paired device or on a NAS. The transfer runs as a job in the job list.",
    send: "Send",
    sendWhat: "What to send",
    sendTo: "Send to",
    /** Under Send to while no other device (paired Forge or NAS) is online: the button is off. */
    sendNeedsDevice: "Connect another device first: pair a Forge or add a NAS.",
    nothingChosen: "Nothing chosen yet",
    chooseFiles: "Choose files",
    chooseFolder: "Choose a folder",
    startTransfer: "Start",
    uploadHere: "Upload to this device",
    downloadHere: "Download from this device",
    consoleNoFiles: "The console's browser can't pick files: open this page on a phone or computer.",
    wholeRoot: "A whole device, share or drive can't be sent: choose a folder inside it.",
    helpPhone: "Scan the QR code to open this page on a phone. There, Upload to this device and Download from this device in File Transfer move the phone's files.",
    /** How to connect: short paragraphs, in order (no headings). */
    help: [
      "Connect links Dump Forge on your PS5 and computers on the same network.",
      "To pair, run Autodiscovery and choose Pair under Nearby, or type an address under Add a device, then allow it on the other device. Allow it only when both screens show the same check number.",
      "A NAS (SMB, FTP, NFS) is added the same way. Paired devices and NAS devices are game sources and outputs; their games aren't edited there.",
      "File Transfer copies files and folders between any of them. A name that already exists there is replaced only after you confirm.",
    ] as string[],
    helpSecurity: "Nothing sent between devices is encrypted: anyone on your network can read it. A saved NAS password is kept on this device unencrypted. Use Connect on your home network only.",
    helpSpeed: "Speed test, in a paired device's row or its details, sends 128 MiB to it and fetches 128 MiB back, nothing written to disk, and shows the network's speed each way.",
    speedTest: "Speed test",
    speedTestWith: (name: string) => `Speed test with ${name}`,
    speedTesting: "Testing",
    speedResult: (up: string, down: string) => `Upload ${up} MB/s · Download ${down} MB/s`,
    speedFailed: "Speed test failed:",
    speedNote: "Sends 128 MiB and fetches 128 MiB back; nothing is written to disk.",
    installTitle: "Dump Forge on PS5",
    ps5Address: "PS5 address",
    install: "Install on PS5",
    installHint: "Needs the console's ELF loader listening on the network (port 9021).",
    findingRelease: "Finding the release on GitHub",
    installing: (v: string, host: string) => `Installing Dump Forge ${v} on ${host}`,
    installed: (v: string, host: string) => `Dump Forge ${v} is starting on ${host}. Pair it from Add a device once it runs.`,
    noRelease: "Can't find Dump Forge's PS5 release on GitHub. Check this computer's internet connection, then try again.",
    otherPayload: "Any other ELF payload:",
    sendPayload: "Send a payload",
    pickPayload: "Choose an ELF payload",
    sendingPayload: (name: string, host: string) => `Sending ${name} to ${host}`,
    sentPayload: (name: string, size: string, host: string) => `Sent ${name} (${size}) to ${host}.`,
    helpInstallTitle: "Install Dump Forge on a PS5 in one click",
    helpInstallText: "Use the Install Dump Forge card to send the payload to the console. It downloads the matching version from GitHub (or the latest) and starts it.",
  },
  // Packages.tsx (Storage's Package installer), InstallDialog.tsx, Files.tsx and Connect.tsx's Install pkg
  install: {
    view: "Package installer",
    packages: "Packages",
    refresh: "Refresh",
    scanning: "Looking for packages",
    none: "No packages found on this console.",
    noneHint: "Copy a .pkg file to the console's storage or a USB drive, then press Refresh.",
    truncated: "List may be incomplete",
    limits: "The search stopped at one of its limits: some packages may be missing.",
    cantScan: "Can't search:",
    cantInstall: "Can't install:",
    content: { game: "Game", patch: "Patch", additional_content: "Additional content", unknown: "Unknown" },
    install: "Install",
    installName: (name: string) => `Install ${name}`,
    started: (name: string) => `Installing ${name}. The job is listed below.`,
    /** The job row's tag. */
    tag: "Install",
    /** The confirm dialog. */
    title: "Install this package?",
    titleOn: (device: string) => `Install this package on ${device}?`,
    file: "File",
    game: "Game",
    titleId: "Title ID",
    version: "Version",
    size: "Size",
    text: "The PS5 fetches the package from Dump Forge and installs it. Keep Dump Forge running until the job ends.",
    reinstall: "If this game is already installed, the PS5 deletes the installed copy before it installs this one.",
    cancel: "Cancel",
    confirm: "Install",
    /** Connect: an online paired PS5's row. */
    installPkg: "Install pkg",
    installPkgOn: (device: string) => `Install a package on ${device}`,
    pickTitle: "Choose a package to install",
    sent: (device: string) => `${device} is installing the package. The job is listed below.`,
  },
  // api.ts: prettyDuration, whole per language (CJK writes 5分30秒, no spaces)
  duration: { s: (s: number) => `${s} s`, ms: (m: number, s: number) => `${m} min ${s} s`, hm: (h: number, m: number) => `${h} h ${m} min` },
};

export type Messages = typeof en;
export default en;
