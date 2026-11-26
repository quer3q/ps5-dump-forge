// English: the source language. Every other file in this folder is typed as `Messages` (this
// object's shape), so tsc fails on a missing key. Grouped by the file that shows the text.
// Stays English in every language: format ids, file names and paths, product names, IEC units.

import type { ReactNode } from "react";

import type { Format, KrakenLevel } from "../api";
import type { Rating, Row } from "../Formats";
import type { ClientText } from "../http-client";
import { dotted, num, plural } from "../i18n";
import type { Failure } from "../launcher";
import type { Engine, PackLevel } from "../build/formats";
import type { Note } from "../target";

const FLAT = "Equal to the game size";

const en = {
  // App.tsx
  app: {
    screens: "Screens",
    active: (n: number) => `, ${num(n)} active`,
    edit: "Edit",
    storage: "Storage",
    connect: "Connect",
    disconnected: "Disconnected",
    connected: (n: number) => plural(n, { one: `${num(n)} device`, other: `${num(n)} devices` }),
    about: "About",
    offline: "Offline, retrying…",
    stop: "Stop",
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
  // build/ (Build's convert settings and checks)
  convert: {
    targetFormat: "Format",
    alreadyFolder: "the source is already a folder",
    patchBlocked: "Can't build: this is a patch package, which holds only what the update changed. Open it in Edit to extract its files.",
    openInEdit: "Open in Edit",
    noContentId: "Can't build as FPKG: this game's param.json has no content ID, which a package needs.",
    blocked: (first: string, more: number) =>
      `Can't build: ${first}${more > 0 ? ` (and ${num(more)} more, see Source)` : ""}.`,
    amprIndex: (state: "stale" | "damaged" | "missing") =>
      `${state === "stale" ? "ampr_emu.index doesn't list this game's files" : state === "damaged" ? "ampr_emu.index can't be read" : "This game has no ampr_emu.index"}: the conversion writes a fresh one.`,
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
      zstd: { tagline: "With ampr-emu-ng", cells: ["Faster loading", "Up to half the game size"] },
      lz4: {
        tagline: "With ampr-emu-ng",
        cells: ["30–40% faster loading, upstream's numbers", "About half the game size, upstream's numbers"],
      },
      ffpkg: { tagline: "Recommended for ShadowMountPlus", cells: ["Full image speed", FLAT] },
      exfat: { tagline: "Compatibility", cells: ["Full image speed", FLAT] },
      ffpfs: { tagline: "Uncompressed PFS", cells: ["Full image speed", FLAT] },
      ffpfsc: {
        tagline: "Smallest",
        cells: ["150–250 MB/s, may stutter", "Depends on the compression level, usually 30–60% less"],
      },
      folder: { tagline: "Plain game files", cells: ["Full drive speed", FLAT] },
    } satisfies Record<Format | "zstd", Row>,
    compare: "Compare formats",
    compareLabel: "Format comparison",
    format: "Format",
    credits: "Credits",
    upstream: (app: string) =>
      `Upstream's numbers: what the authors of ampr_emu and Lazy_AMPR report, not measured by ${app}; they vary by game.`,
    bundled: "ampr_emu by drakmor (GPL-3.0), which ampr-emu-ng forks. Forge no longer bundles it: it replaces a game's ampr_emu with ampr-emu-ng unless Settings keeps it. Source:",
    lazy: "The packing strategy builds on Lazy_AMPR by Nazky:",
    ng: "ZSTD and LZ4 packs run on ampr-emu-ng (GPL-3.0-or-later), the one AMPR runtime bundled, unmodified. Source:",
    logo: "The logo is by Xxcco:",
    unrar:
      ".rar archives are read with the bundled UnRAR library by Alexander Roshal (extract only, under the UnRAR license).",
    language: "Language",
    system: "System",
    notifications: "Notify when a job finishes",
    notificationsHint: "Shows on the console when a build, pack or transfer job ends.",
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
    actions: "Actions",
    items: (n: number) => `${num(n)} ${plural(n, { one: "item", other: "items" })}`,
    inFolder: (dirs: number, files: number) =>
      `${num(dirs)} ${plural(dirs, { one: "folder", other: "folders" })}, ${num(files)} ${plural(files, { one: "file", other: "files" })}`,
    matches: (n: number) => `${num(n)} ${plural(n, { one: "match", other: "matches" })}`,
    emptyFolder: "This folder is empty.",
    open: (name: string) => `Open ${name}`,
    archiveNote: "Archives can be browsed but not edited. Build a folder or an image from it first to change it.",
    rebuildTimePkg: (t: string) => `Rebuild compresses the whole game again: about ${t} at ~26 MB/s on a PS5 (faster on a computer). Edit an unpacked copy instead to skip it.`,
    rebuildTimePs5: (t: string) => `Rebuild writes the whole image again (about ${t} on this PS5).`,
    rebuildTimeHost: (t: string) => `Rebuild writes the whole image again (about ${t} on this computer).`,
    rebuildTimeAny: (t: string) => `Rebuild writes the whole image again (about ${t} on a PS5).`,
    nasNote: "Games on a NAS can be browsed and extracted but not edited. Build it or extract it to this device first to change it.",
    patchNote: "This is a patch package: it holds only what the update changed. The files it carries whole are listed; they can be browsed and extracted but not edited.",
    peerNote: "Games on another Forge can be browsed and extracted but not edited. Build it or extract it to this device first to change it.",
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
    alwaysRebuilt: "ffpfsc and FPKG images are always rebuilt.",
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
    applyRunning: "Applying the changes. Editing waits until the job ends (see Queue).",
    applyFailed: "The changes weren't applied:",
    applyRefused: "The changes can't be applied this way:",
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
    /** The job row's step list: one label per stage id; equal neighbours are one step. */
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
    stepDone: (s: string) => `${s}, done`,
    stepFailed: (s: string) => `${s}, failed`,
    stepLater: (s: string) => `${s}, not started yet`,
    packingWriting: "Packing and writing",
    rate: { read: "read", compress: "compress", pack: "pack", write: "write" },
    showPath: "Show path",
    showFinder: "Show in Finder",
    showExplorer: "Show in Explorer",
    showFolder: "Show in folder",
    packTag: {
      lz4Pack: (level: string) => `LZ4 pack · ${level}`,
      lz4Unpack: "LZ4 unpack",
      zstdPack: (level: string) => `ZSTD pack · ${level}`,
      level: (n: number) => `level ${num(n)}`,
      zstdUnpack: "ZSTD unpack",
      withRules: (tag: string) => `${tag} · custom rules`,
    },
    edit: "Edit",
    editRebuild: "Edit · rebuild",
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
  // Storage text shared by Files.tsx, Packages.tsx, Edit.tsx and common.tsx (FreeSpace)
  library: {
    cantDelete: "Can't delete:",
    deleted: (path: string) => `Deleted ${path}`,
    storageTip: "Free space on the console's internal storage (/data)",
    free: (free: string, total: string) => (
      <>
        Free <b>{free}</b> of {total}
      </>
    ),
    cards: "Card view",
    list: "List view",
    remove: (n: string) => `Delete ${n}`,
    deleting: (n: string) => `Deleting ${n}`,
    showAs: "Show as",
  },
  // Storage.tsx, Files.tsx: the Storage tab (File explorer | Package installer)
  storage: {
    views: "Storage view",
    files: "File explorer",
    title: "File explorer",
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
    otherDevice: "Move to stays on one device: the Transfer screen copies between devices.",
    loadingDevices: "Loading devices",
    noDevice: "No device connected",
    noDeviceHint: "Pair a PS5 or a computer, or add a NAS, to manage its files here.",
    openConnect: "Open Connect",
    dropMove: (folder: string) => `Drop to move into ${folder}`,
    cantCopy: "Can't copy:",
    copyNoFolder: "Open a folder on that device first.",
    copyOne: (name: string, to: string) => `Copying ${name} to ${to}. The transfer is listed in Queue.`,
    copyMany: (n: number, to: string) =>
      plural(n, {
        one: `Copying ${num(n)} item to ${to}. The transfer is listed in Queue.`,
        other: `Copying ${num(n)} items to ${to}. The transfer is listed in Queue.`,
      }),
  },
  // build/ (Build's pack settings and checks), JobList.tsx's pack tags
  pack: {
    help: "How pack works",
    patch: "A patch package can't be packed.",
    additionalContent: "Additional content (no eboot.bin) can't be packed.",
    levels: { fast: "Fast", medium: "Medium", max: "Max" } satisfies Record<PackLevel, string>,
    levelNotes: {
      zstd: {
        fast: { tone: "good", text: "Recommended. ZSTD 5: quick, good size." },
        medium: { text: "ZSTD 9: smaller, slower." },
        max: { text: "ZSTD 19: smallest; much slower, more memory." },
      },
      lz4: {
        fast: { tone: "good", text: "Recommended. LZ4 default: quickest." },
        medium: { text: "LZ4 HC 9: smaller, slower." },
        max: { text: "LZ4 HC 12: smallest, slowest." },
      },
    } satisfies Record<Engine, Record<PackLevel, Note>>,
    freshIndex: (state: "stale" | "damaged" | "missing") =>
      `${state === "stale" ? "ampr_emu.index is out of date" : state === "damaged" ? "ampr_emu.index is damaged" : "No ampr_emu.index"}: packing writes a fresh one.`,
    rules: {
      title: "Custom rules (TOML)",
      titleWith: (name: string) => `Custom rules (TOML): ${name}`,
      info: "A profile of the packing strategy in the format of ampr_emu's ampr_pack.py and Lazy_AMPR decides which files are packed and which stay loose. The last matching rule wins; system files always stay loose.",
      warn: "Wrong rules can leave a game that won't start. Keep a copy.",
      pickTitle: "Packing strategy (TOML file)",
      clear: "Clear",
    },
  },
  // PackHelp.tsx
  packHelp: {
    title: "How pack works",
    close: "Close",
    body: () => (
      <>
        <h3>ZSTD</h3>
        <ol>
          <li>Packs the game's assets into ZSTD volumes, into a folder or an image.</li>
          <li><b>Fast</b> (ZSTD 5) is quick; <b>Medium</b> (9) and <b>Max</b> (19) are smaller, slower.</li>
          <li>For a ZSTD-packed game, <b>None</b> unpacks it: the plain files come back.</li>
        </ol>

        <h3>LZ4</h3>
        <ol>
          <li><b>Some games may not start.</b> Keep a copy.</li>
          <li>Packs into LZ4 volumes, into a folder or an image: <b>Fast</b> (LZ4), <b>Medium</b> (HC 9), <b>Max</b> (HC 12).</li>
          <li>Runs on ampr-emu-ng, as ZSTD packs do.</li>
          <li>For an LZ4-packed game, <b>None</b> unpacks it.</li>
        </ol>

        <h3>ampr-emu-ng</h3>
        <ol>
          <li>The one AMPR runtime Forge bundles. It reads ZSTD and LZ4 packs; packing installs it in fakelib/.</li>
          <li>A build replaces a game's ampr_emu with it, unless Settings keeps ampr_emu. An FPKG carries no runtime: the console's loader provides it.</li>
          <li>Turn off ShadowMount+'s fakelib update, or it replaces the runtime and the game can't read its packs.</li>
        </ol>

        <h3>Packing strategy</h3>
        <ol>
          <li><b>Pack everything except system files</b> (ZSTD and LZ4): every file but eboot.bin, executables and modules, sce_sys, sce_module, fakelib and .ini files (dlc_emu.ini stays editable); ampr-emu-ng serves the rest, audio and video included.</li>
          <li>When packing, under <b>Custom rules (TOML)</b>, pick a profile in the format of ampr_pack.py and Lazy_AMPR, on this device or another one.</li>
          <li>The last matching rule wins; system files always stay loose. Wrong rules can leave a game that won't start.</li>
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
    netOnlyHere: "Archives and FPKG open from this device only.",
  },
  // bookmarks.tsx: the bookmarked folders (Transfer and the File explorer)
  bookmarks: {
    label: "Bookmarks",
    add: "Bookmark this folder",
    remove: "Remove bookmark",
    removeNamed: (name: string) => `Remove bookmark ${name}`,
    full: "9 bookmarks at most",
  },
  // DeviceTabs.tsx
  deviceTabs: {
    add: "Add a device",
    offline: "Offline",
    offlineNamed: (name: string) => `${name} · Offline`,
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
    unsigned: (n: number) =>
      plural(n, {
        one: `${num(n)} executable isn't signed, so the game won't start. Converting the game signs it.`,
        other: `${num(n)} executables aren't signed, so the game won't start. Converting the game signs them.`,
      }),
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
    traceRuntime: "Old LZ4 trace runtime",
    noRuntime: "No runtime",
    packed: (kind: "LZ4" | "ZSTD") => `Packed (${kind})`,
    zstdKept: "ZSTD asset packs: a conversion keeps them",
    damaged: "Damaged packs",
    unpacked: "Unpacked",
    packedDetail: (files: number, volumes: number, percent: number | null) =>
      `${num(files)} ${plural(files, { one: "file", other: "files" })} in ${num(volumes)} ${plural(volumes, { one: "volume", other: "volumes" })}` +
      (percent === null ? "" : `, ${num(percent)}% of size`),
    index: {
      stale: "ampr_emu.index out of date: a conversion refreshes it",
      damaged: "ampr_emu.index damaged: a conversion rewrites it",
      missing: "no ampr_emu.index: a conversion writes one",
    } satisfies Record<"stale" | "damaged" | "missing", string>,
    /** What a conversion does to the runtime; "keepIfPacks": an ampr_emu build in an LZ4-packed
     * game, with Settings' replace switch off. */
    runtimeAction: (a: "install" | "replace" | "keep" | "keepIfPacks"): string =>
      a === "install"
        ? "a conversion installs the bundled ampr-emu-ng"
        : a === "replace"
          ? "a conversion replaces the runtime with the bundled ampr-emu-ng"
          : a === "keep"
            ? "a conversion keeps it (Settings)"
            : "a conversion keeps it if it reads LZ4 packs, else replaces it with ampr-emu-ng",
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
    wrong: "The password is not correct. Check it and try again.",
    label: "Password",
    cancel: "Cancel",
    unlock: "Unlock",
    checking: "Checking",
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
    pkgExt: (ext: string) => `Ends in .${ext}, but FPKG is saved as .pkg: the extension picks the driver.`,
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
  },
  // Settings.tsx (Output names) and target.tsx's name pattern: generated output names
  naming: {
    title: "Output names",
    template: "Name template",
    parts: { title: "Title", title_id: "Title ID", version: "Version", pack: "Pack kind", region: "Region", fw: "Firmware" },
    partsLabel: "Insert a part",
    insert: (part: string) => `Insert ${part}`,
    inUse: (part: string) => `${part} is already in the template`,
    example: "Example",
    length: "Length",
    bytes: (n: number, max: number) => `${n} of ${max} bytes.`,
    room: (n: number) => plural(n, { one: `${num(n)} byte is left for the title.`, other: `${num(n)} bytes are left for the title.` }),
    noRoom: "No room is left for the title: it is left out.",
    versionOut: "The version is left out to make room.",
    cut: "The name is cut at 63 bytes.",
    idOnly: "The template's own text fills the 63 bytes, so the title ID names the output.",
    errors: {
      long: (max: number) => `A template is at most ${max} bytes.`,
      open: "A { has no closing }.",
      close: "A } has no opening {.",
      unknown: (token: string) => `{${token}} isn't a part: use Title, Title ID, Version, Pack kind, Region or Firmware.`,
      twice: (token: string) => `{${token}} is in the template twice: use each part once.`,
      char: (c: string) => `A name can't hold ${c}.`,
      control: "A name can't hold a control character.",
      needsName: "A name needs Title or Title ID.",
    },
    notSaved: "Not saved: names keep the last template that worked.",
    reset: "Reset",
  },
  // UpdateToast.tsx: a newer release (checked at every start)
  update: {
    available: (v: string) => `v${v} is available`,
    update: "Update",
    later: "Later",
    updating: "Updating…",
    updatingNote: "The new version is downloading. This page reloads into it by itself.",
    downloading: "The download is opening in your browser: replace this copy with the downloaded file (on a Mac, drag the app from the .dmg to Applications).",
    downloadingCli: "The download is opening in your browser: replace the ps5-dump-forge CLI with the downloaded file, or run ps5-dump-forge update --install on the computer running it.",
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
    waitingFor: (address: string) => `Waiting for ${address} to allow the pairing`,
    checkNumber: "Check number",
    checkHint: "It must match the number the other device shows.",
    denied: "The other device denied the pairing.",
    timedOut: "Nobody allowed the pairing on the other device in time.",
    pairBusy: "The other device has other pairing requests waiting. Try again in a minute.",
    alreadyPaired: "That device already has this one saved. Forget it there first, then pair again.",
    tooManyAttempts: "Too many refused requests. Try again in 10 minutes.",
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
    thisBrowser: "This browser",
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
    cantUpload: "Can't upload:",
    send: "Send",
    downloadHere: "Download from this device",
    wholeRoot: "A whole device, share or drive can't be sent: choose a folder inside it.",
    helpPhone: "Scan the QR code to open this page on a phone. There, Transfer uploads the phone's files to this device and downloads this device's files to the phone.",
    /** How to connect: short paragraphs, in order (no headings). */
    help: [
      "Connect links Dump Forge on your PS5 and computers on the same network.",
      "To pair, run Autodiscovery and choose Pair under Nearby, or type an address under Add a device, then allow it on the other device. Allow it only when both screens show the same check number.",
      "A NAS (SMB, FTP, NFS) is added the same way. Paired devices and NAS devices are game sources and outputs; their games aren't edited there.",
      "Transfer, in the side menu, copies files and folders between any of them. A name that already exists there is replaced only after you confirm.",
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
    started: (name: string) => `Installing ${name}. The job is listed in Queue.`,
    tag: "Install",
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
    installPkg: "Install pkg",
    installPkgOn: (device: string) => `Install a package on ${device}`,
    pickTitle: "Choose a package to install",
    sent: (device: string) => `${device} is installing the package. The job is listed in Queue.`,
  },
  // Build's and Edit's Restore metadata online switch (build/options.tsx, Edit.tsx)
  metadata: {
    tag: "Metadata",
    cant: "Can't restore the metadata:",
    label: "Restore metadata online",
    buildHint: "Before the build, downloads this version's param.json, pictures, sound and PlayGo tables from Sony's update server and puts them into the output where they differ. The game itself isn't changed. Needs the internet; without it the build goes on as is.",
    buildSummary: "Puts Sony's metadata into the output (the game isn't changed)",
    editHint: "Applies this version's param.json, pictures, sound and PlayGo tables from Sony's update server to this game, with your changes, where they differ: a folder is changed directly, an exfat or ffpkg in place, an ffpfs, ffpfsc or FPKG rebuilt. Needs the internet; if the download fails, nothing is changed.",
    editOnly: "Sony's metadata for this version, applied to this game",
    editAlso: "Sony's metadata for this version is applied too, downloaded first.",
    editConfirm: "Apply Sony's metadata?",
    editConfirmText: "The metadata is downloaded first; if that fails, nothing is changed.",
  },
  // App.tsx: the side rail
  rail: {
    build: "Build",
    settings: "Settings",
    queue: "Queue",
    transfer: "Transfer",
    collapse: "Collapse menu",
    expand: "Expand menu",
  },
  // Queue.tsx
  queue: {
    empty: "No jobs yet. Builds, edits, transfers and installs show up here.",
    counts: (running: number, queued: number, failed: number, done: number): string =>
      [
        running && `${num(running)} running`,
        queued && `${num(queued)} queued`,
        failed && `${num(failed)} failed`,
        done && `${num(done)} done`,
      ]
        .filter(Boolean)
        .join(", ") || "Nothing to show",
    retryFailed: (n: number): string => (n > 0 ? `Retry failed (${num(n)})` : "Retry failed"),
    retrying: (i: number, n: number) => `Retrying ${num(i)} of ${num(n)}`,
    clearFinished: "Clear finished",
    filters: { all: "All", running: "Running", failed: "Failed", done: "Done" },
    filterLabel: "Show",
    noneShown: "No jobs match this filter.",
  },
  // Settings.tsx
  settings: {
    appearance: "Appearance",
    theme: "Theme",
    themeHint: "Dark suits a TV; light a bright room.",
    dark: "Dark",
    light: "Light",
    scheme: "Colour scheme",
    schemeHint: "The accent colour across the app.",
    schemes: { abyss: "Abyss", ember: "Ember", moss: "Moss", cobalt: "Cobalt", dusk: "Dusk", rose: "Rose", sand: "Sand", volt: "Volt", mono: "Mono" },
    advanced: "Advanced options",
    advancedOpen: "Always show advanced options",
    advancedHint:
      "Shows every pack and format, with no More, and keeps every fold open. Turn it off to keep Build short.",
    bundled: (version: string) => `${version} (bundled)`,
    replaceAmprEmu: "Replace ampr_emu with ampr-emu-ng",
    languageHint: "The system's languages until you choose one.",
    notificationsGroup: "PS5",
  },
  // JobList.tsx: a finished job's result
  result: {
    file: "Output",
    type: "Type",
    size: "Size",
    files: "Files",
    time: "Time",
    verified: "Verified",
    verifiedFast: "Verified (fast)",
    verifiedFull: "Verified (full)",
    notVerified: "No check reported",
    checks: "Checks:",
    copy: "Copy details",
    copied: "Copied",
    copyFailed: "Can't copy here",
    ofSource: (pct: number) => `${num(pct, 1)}% of the game's files`,
    filesN: (n: number) => plural(n, { one: `${num(n)} file`, other: `${num(n)} files` }),
    to: "written to",
    took: (d: string) => `took ${d}`,
    kinds: {
      pkg: "FPKG: installable on a PS5 with FPKG support",
      exfat: "exfat image: ShadowMount+ mounts it, writable",
      ffpkg: "ffpkg image (UFS2): ShadowMount+ mounts it, writable",
      ffpfs: "ffpfs image (PFS): ShadowMount+ mounts it",
      ffpfsc: "ffpfsc image (compressed PFS): ShadowMount+ mounts it read-only",
      folder: "Game folder",
    } as Record<string, string | undefined>,
    zstdFolder: "ZSTD packed folder",
    lz4Folder: "LZ4 packed folder",
    withPacks: (image: string, kind: string) => `${image}, files in ${kind} packs`,
    edited: (what: string) => `${what}, changed in place`,
    rebuilt: (what: string) => `${what}, rebuilt`,
    extracted: "Extracted files",
    transferred: "Copied files",
    installed: "Package handed to the PS5's installer",
  },
  // JobList.tsx: the log panel
  log: {
    lines: (n: number, of?: number): string =>
      of === undefined
        ? plural(n, { one: `${num(n)} line`, other: `${num(n)} lines` })
        : plural(of, { one: `${num(n)} of ${num(of)} line`, other: `${num(n)} of ${num(of)} lines` }),
    filter: "Filter",
    follow: "Auto-scroll",
    copy: "Copy",
    copied: "Copied",
    copyFailed: "Copy failed",
    save: "Save",
    clear: "Clear",
    cleared: "Cleared",
  },
  // common.tsx: the source card's fact chips
  sourceChips: {
    sdk: (v: string) => `SDK ${v}`,
    firmware: (v: string) => `FW ${v}+`,
  },
  // prefs.tsx: Advanced's badge
  advanced: {
    changed: "Changed",
    defaults: "Defaults",
  },
  // build/*: the Build screen
  build: {
    summary: "Summary",
    addToQueue: "Add to queue",
    queued: "Added to the queue",
    openQueue: "Open Queue",
    space: (free: string, dir: string, needed: string): ReactNode => (
      <>
        {free} free in <span className="path">{dir}</span>, about {needed} needed
      </>
    ),
    needed: (needed: string) => `About ${needed} needed`,
    spaceRaw: "Room is checked as if nothing compressed.",
    estimate: "estimate",
    advanced: "Advanced options",
    // options.tsx
    options: "Options",
    fastVerify: "Fast verification: every structure checked, a sample of the data read back.",
    fullVerify: "Full verification: every byte is read back.",
    deleted: "The source is deleted after a successful build. No undo.",
    // backport.tsx
    zstdBackport:
      "This game's assets are ZSTD packed and its ZSTD index lists the backport: build it once with Packs: None, then remove the backport.",
    lz4Backport:
      "This game's assets are LZ4 packed and its LZ4 manifest lists the backport: build it once with Packs: None, then remove the backport.",
    backportLeftOut: (n: number) =>
      plural(n, {
        one: `${num(n)} backport library is left out of fakelib/.`,
        other: `${num(n)} backport libraries are left out of fakelib/.`,
      }),
    // packs.tsx and fields.tsx; `engine` is "zstd" or "LZ4"
    packsTitle: "Target",
    packs: "Packs",
    more: "More",
    moreTip: "More formats",
    cards: {
      folder: "Plain files, copy anywhere, ShadowMount+ runs it",
      pkg: "Installs on the PS5 like a game package",
      exfat: "Mounts with ShadowMount+ and opens on a PC",
      ffpkg: "UFS2 image for ShadowMount+, writable mounts possible",
      ffpfs: "PFS image, uncompressed, ASCII file names only",
      ffpfsc: "Smallest image: compressed, always mounted read-only",
      lz4: "Assets packed with LZ4, plain folder",
    } satisfies Record<Format, string>,
    zstdFolderCard: "Assets packed with ZSTD, plain folder",
    packCards: {
      nonePlain: "The game's files as they are",
      unpack: "Writes the packs back as plain files",
      keepZstd: "The ZSTD packs are kept byte for byte",
      keepLz4: "The LZ4 packs are kept byte for byte",
      zstd: "Smaller game, faster loading, with ampr-emu-ng",
      repackZstd: "Decodes the LZ4 packs, packs again with ZSTD",
      lz4: "Packs assets with LZ4, with ampr-emu-ng",
      repackLz4: "Decodes the ZSTD packs, packs again with LZ4",
    },
    naming: "Output: the name is being made",
    output: (name: string, dir: string): ReactNode => (
      <>
        Output: <b className="path">{name}</b> in <span className="path">{dir}</span>
      </>
    ),
    choices: { none: "None", zstd: "ZSTD", lz4: "LZ4" },
    levelLabel: (level: string, engine: string, n: number) => `${level}, ${engine} ${n}`,
    levelTip: (engine: string, n: number, note: string) => `${engine} ${n}. ${note}`,
    sumKeepZstd: "The ZSTD packs are kept byte for byte.",
    sumKeepLz4: "The LZ4 packs are kept byte for byte.",
    sumKeepPlain: "No packs are made: the game's files are converted as they are.",
    sumUnpack: (engine: string) => `The ${engine} packs are written back as plain files.`,
    sumPack: (engine: string, level: string) => `The game's files are packed into ${engine} volumes (${level}).`,
    sumRepack: (from: string, to: string, level: string) => `The ${from} packs are decoded and packed again with ${to} (${level}).`,
    sumRules: (name: string) => `Custom rules ${name} pick which files are packed.`,
    sumInside: (format: string, level: number) => `Inside: ${format}, zlib level ${level}.`,
    ngInstalled: "ampr-emu-ng is installed at fakelib/libSceAmpr.sprx.",
    /** `old`: the game's runtime with its version, e.g. "ampr_emu 0.4.2.1". */
    ngReplaced: (old: string) => `The game's ${old} is replaced with ampr-emu-ng.`,
    ngReplacedTrace: "The game's old trace runtime is replaced with ampr-emu-ng.",
    ngKept: (old: string) => `The game's ${old} is kept.`,
    /** An LZ4-packed game kept as it is, with the replace switch off. */
    /** `v`: the game's ampr-emu-ng version with a leading space, or empty. */
    ngUpdated: (v: string) => `The game's ampr-emu-ng${v} is replaced with the bundled one.`,
    ngSame: (v: string) => `The game's ampr-emu-ng${v} is kept.`,
    pkgNoRuntime: "The AMPR runtime is left out of the FPKG: the console's loader provides it.",
    // Build.tsx: the result line under the target
    recommended: "Recommended",
    resultTitle: "About, after the build",
    resultNow: "Game now",
    resultCustomRules: "custom rules: no estimate",
    // backport.tsx: the Backport step
    backportTitle: "Backport",
    backportNone: "No backport in this game.",
    backportPlain: "Only for a plain conversion: no packs made or unpacked.",
  },
  // TransferPane.tsx: one device's pane of the Transfer screen
  transfer: {
    pane: (device: string) => `Files on ${device}`,
    roots: "Drives",
    location: "Location",
    up: "Up",
    upTip: "Up one folder",
    refresh: "Refresh",
    filter: "Filter by name",
    filterLabel: "Filter this folder by name",
    inFolder: (dirs: number, files: number) =>
      `${num(dirs)} ${dirs === 1 ? "folder" : "folders"}, ${num(files)} ${files === 1 ? "file" : "files"}`,
    matches: (n: number) => plural(n, { one: `${num(n)} match`, other: `${num(n)} matches` }),
    firstOf: (max: number, n: number) => `first ${num(max)} of ${num(n)}`,
    truncated: "This folder holds more than 10,000 entries: only the first 10,000 are listed.",
    name: "Name",
    size: "Size",
    select: (name: string) => `Select ${name}`,
    open: (name: string) => `Open ${name}`,
    selected: (n: number) => plural(n, { one: `${num(n)} item selected`, other: `${num(n)} items selected` }),
    clear: "Clear",
    clearTip: "Clear the selection",
    dragItems: (n: number) => plural(n, { one: `${num(n)} item`, other: `${num(n)} items` }),
    dropHere: (path: string) => `Drop to send to ${path}`,
    notHere: "Open a folder to send files here.",
    emptyFolder: "This folder is empty.",
    noMatch: (f: string) => `Nothing here matches “${f}”.`,
    noRoots: "No drives are available to browse.",
    cantList: "Can't open this folder:",
    loading: "Loading",
    link: "Link",
    linkTip: "A link: it can be opened, but not selected",
    mounted: "Mounted",
    mountedTip: "Mounted by ShadowMount+: stop the game and turn ShadowMount+ off to select it",
    // Transfer.tsx: the Transfer screen
    withTitle: "Transfer with",
    loadingDevices: "Loading devices",
    noneTitle: "No device to transfer with",
    noneHint: "Pair another Dump Forge or add a NAS under Connect, or turn on a saved one.",
    openConnect: "Open Connect",
    deviceOffline: (device: string) => `${device} is offline. Its files show again once it is back.`,
    copyTo: (n: number, device: string) =>
      plural(n, { one: `Copy ${num(n)} item to ${device}`, other: `Copy ${num(n)} items to ${device}` }),
    copyNoFolder: (device: string) => `Open a folder on ${device} to copy into.`,
    installOn: (device: string) => `Install on ${device}`,
    dropFiles: "Drop files here to upload them",
    uploadInto: (folder: string) => `They go into ${folder}, the folder shown on this device.`,
    uploadNoFolder: "Open a folder on this device to upload into.",
    foldersLeftOut: (names: string) => `Folders aren't uploaded, only files: ${names}.`,
    downloadFiles: (n: number) => plural(n, { one: `Download ${num(n)} file`, other: `Download ${num(n)} files` }),
    downloadNone: "Select files on this device to download them (folders aren't downloaded).",
  },
  // api.ts: prettyDuration, whole per language (CJK writes 5分30秒, no spaces)
  duration: { s: (s: number) => `${s} s`, ms: (m: number, s: number) => `${m} min ${s} s`, hm: (h: number, m: number) => `${h} h ${m} min` },
};

export type Messages = typeof en;
export default en;
