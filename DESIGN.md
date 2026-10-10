# PS5 Dump Forge UI design system

The rules behind `app/src/styles.css`. The CSS holds the exact values; this file says when to use what.
The look follows the SpendIQ Dribbble shot (dark cards, pill buttons, inset stat tiles, tinted tags, a
gradient notice bar) and the app icon (near-black navy with a blue edge and an orange glow).

## Principles

- **Dark only.** `color-scheme: dark`. There is no light theme; adding one means a second token set.
- **One hero.** The game card (`.card.hero`) is the only card with a glow, and the glow takes its
  colours from the game's cover (`glow.ts`).
- **Pills for controls.** Buttons, inputs, tags and segmented controls are fully rounded (999px).
  Cards are 16px, inset tiles 12px.
- **Facts in inset tiles.** A value a user decides on (title ID, version, firmware, size) sits in a
  darker `.tile`: a muted label above a large value.
- **Colour never works alone.** A format has its hue *and* its label. A rating has its tint *and* an
  icon (✓ – ✕) *and* text. A selected segment is tinted *and* ringed *and* bold.
- **Contrast.** Text is at least 4.5:1 on its ground, which also holds on the hero glow and the alert
  bar. The CSS comments name the measured ratio wherever a pair is tight.
- **Fits the window.** Default size 1200×820, minimum 900×620 (`main.rs`). No horizontal page scroll.
  Long lists scroll inside their own box, and Build and Cancel stay reachable.
- **Offline and CSP-safe.** System fonts, inline SVG icons, the logo bundled by Vite. No remote
  assets and no injected `<style>`. The only inline styles are the CSS variables set through React's
  `style` (CSSOM), which the CSP allows.

## Tokens (`:root`)

| Token | Value | Use |
|---|---|---|
| `--page` | `#0e0e15` | Window background, sticky header |
| `--card` | `#17171f` | Cards |
| `--inset` | `#101017` | Tiles, inputs, folds, table and list wells |
| `--raise` / `--raise-hi` | `#24242f` / `#2e2e3b` | Secondary buttons / hover, the selected tab |
| `--line` / `--line-hi` | white 7% / 12% | Card borders and row dividers / input borders |
| `--fg` / `--fg-2` | `#f2f2f7` / `#d2d2dc` | Text / secondary text (paths, checks) |
| `--muted` | `#9a9aae` | Labels, meta, unselected segments (≥ 4.5:1 on every ground) |
| `--accent` / `--accent-ink` | `#3d6bff` / `#9db6ff` | Switch on, progress, empty-state icon / link text |
| `--orange` | `#ff7a2f` | Glow only (logo shadow, hero and alert-bar edge), never text |
| `--good` / `--bad` / `--warn` | `#2fd27a` / `#ff6b6b` / `#ffb547` | Status icons and text |
| `--focus` | `#9db6ff` | 2px focus ring on every control (`:focus-visible`) |
| `--mono` | `ui-monospace, SFMono-Regular, Menlo` | Paths, file names, IDs, param.json, logs |
| `--tab-lz4` / `--strap` / `--strap-hi` | `#1c1c26` / yellow 12% / 20% | The LZ4 screen tab's ground and straps (unselected `--lz4-ink` 9.2:1, selected `--lz4-ink-hi` 6.4:1 on a strap) |
| `--lz4-ink` / `-ink-hi` / `-soft` | `#ffd84d` / `#ffe27a` / `#e3c766` | LZ4 yellow text: the tab, its scenario and action pickers, Choose source…, How LZ4 works, LZ4 job tags |
| `--lz4-tint` / `-tint-hi` / `-line` | yellow 14% / 22%, ring 50% | Their tint, hover and ring (`--lz4-ink` on tint 9.3:1 over `--card`, 10.2:1 over `--inset`; unselected `--lz4-soft` 11.4:1; disabled `--muted` 6.9:1) |
| `--neon-*` | lime, magenta, cyan at 12%; ring lime 38%; ink `#d6e875` | A traced source's `LZ4 (AMPR)` tile (`.tile.neon`) |
| `--pk-*` | green 14%, cyan 12%; ring 42%; ink `#86f5b4` | A packed source's tile (`.tile.neon.packed`: `--pk-ink` 10.4:1, `--muted` 5.0:1) and Save as profile (`button.neon-btn`: `--fg` 9.1:1, hover 6.4:1) |
| count badge | `#2f5bff` + white | Running-jobs count on the Convert tab (5.2:1) |
| primary button | `#fff` + `#0e0e15` | The one main action per card (Build) |

**Format hues.** `.fmt-*` sets `--h` (text), `--h-bg` (tint) and `--h-line` (ring), with text on tint ≥ 4.5:1:

| Format | Hue | Note |
|---|---|---|
| Folder | white `#ececf4` | |
| `.exfat` | magenta `#ffa3e6` | |
| `.ffpkg` | blue `#a3c0ff` | the default target |
| `.ffpfs` | teal `#7ee0e0` | |
| `.ffpfsc` | amber `#ffc580` | |
| `.fpkg` | green `#8fe0ab` | class `fmt-pkg`; the id and file extension stay `pkg` |
| LZ4 packed folder | lime `#d6e875` | class `fmt-lz4`; not a Convert target segment (only the LZ4 tab's Pack target picker shows it); text on tint 9.7:1 over `--card`, 10.6:1 over `--inset` |

**Semantic tags.** `.tag.blue`, `.tag.green`, `.tag.red`, `.tag.orange` and `.tag.violet` are tinted
pills for states: Writing (blue), Done (green), Failed (red), Backport (orange), DLC (violet).
A plain `.tag` is neutral: Emulators (a `fakelib/` that holds only AMPR, DLC or PlayGo emulators).

**Type.** System font (SF Pro on macOS), 14px/1.45 body. Scale:

| Size / weight | Use |
|---|---|
| 20/600 | Game title |
| 18/600 | Tile values (tabular numbers) |
| 16/600 | Empty-state title, modal title |
| 15/500 | Card titles |
| 13.5/600 | Job names |
| 13 | Controls and body copy |
| 12.5 | Labels, meta, notes |
| 12 mono | Paths |
| 11.5 | Hints, under-tile notes |

**Spacing.**

| Value | Use |
|---|---|
| 28px | Page gutters |
| max 1200px | Content width |
| 16px | Gap between cards |
| 20–22px | Card padding |
| 14px | Card header to body |
| 12px | Between groups inside a card |
| 8px | Between tiles and tags |

Two columns start at 880px wide. Inspect splits 5:7 from 1100px.

## Components (class names in `styles.css`)

- **Header** (`.topbar`, no bottom border: the page runs straight into the screens): the 30px logo on
  the left, the screen tabs centred as a segmented control (`.seg.nav`: 14px labels, a 1% white fill over a 28px `backdrop-filter` blur). No wordmark, because the window title
  already says "PS5 Dump Forge". In the web build the server's host:port (`.address`, mono 600 15px
  `--fg-2`, ellipsized first) sits right of the logo as an `.address-btn` that opens **How to
  connect** (`.connect-modal`: the 240px QR code, `img.qr`, the server's SVG dark on white, over one
  line, one Close). The session's `url`, never the viewer's `location`; none from an older server, and
  no QR when `/api/qr` fails.
  The GitHub mark (`.gh-link`, 32px round, `--muted`, the project page) is the header's rightmost
  item: after "Stop PS5 Dump Forge" in the web build, alone in the app, which opens it in the
  default browser by the fixed-URL `open_repo` command (a link would open inside the app's window).
  The version is in the title ("PS5 Dump Forge v…": the desktop window's title, and the page title the
  PS5 browser shows in its title bar), not in the header.
- **Segmented control** (`.seg`, `.seg-opt`, `.on`): used for the screen tabs (Convert, Inspect | LZ4 |
  About: one tablist, the `.tab-sep` dividers drawn only, the arrow keys move through every tab), the
  Inspect sub-tabs, the format picker (`.seg.formats`) and the LZ4 tab's scenario and Trace action pickers.
  The LZ4 screen tab (`.seg-opt.tab-lz4`) is a grey pill (`--tab-lz4`) with four evenly spaced yellow
  `--strap` straps running from the top-right to the bottom-left (static) and `--lz4-ink` text; selected, it takes
  `--raise-hi`, brighter straps (`--strap-hi`), `--lz4-ink-hi`, bold and a `--lz4-line` ring. The focus ring is the usual one.
  - In the format picker the selected segment fills with its format tint.
  - Unselected segments are plain muted text, without dots.
  - Native radios or `role="tab"` sit underneath, so the keyboard works.
  - Segments are sized by their labels, then share the spare width; six formats fit half of a 900px window.
  - The image inside a `.ffpfsc` is a smaller picker of the same kind (`.seg.formats.inner`), shown only
    for that target. So is the `.fpkg` "Compression" picker (Fast / Balanced / Smallest, each led by a
    15px icon: bolt, scale, compress; all in the `.fpkg` tint), with one line under it on what the chosen
    level costs in time and CPU.
  - The `.ffpfsc` "Compression level" is a native range input (`.range`, `accent-color`), 0–9, default 6,
    under "Image inside": its label row shows the value (`.range-value`, tabular figures), one line under
    it says what the level costs.
- **Card** (`.card`, no border: `--card` on `--page` and the shadow set it off; `.card-head` = 18px
  icon + 18px title + actions on the right, `.card.compact`). The hero variant is `.card.hero`; its
  glow (two radial gradients from the top-right corner, 170%×140% and 90%×80% of the card) fades in once
  the colours are known (`.lit`).
- **Buttons.**

  | Kind | Class | Use |
  |---|---|---|
  | Primary | `.primary` | White. One per card (Build). |
  | Secondary | default | Dark. Choose folder…, Change…, Show in Finder (Explorer, folder). |
  | Danger | `.danger` | Red tint. "Stop job?", "Stop jobs and quit", Inspect's trash button and "Yes, delete". |
  | Small | `.small` | Job rows; `.small.icon-only` is a 30px round icon button (Inspect's trash). |
  | LZ4 | `.lz4-btn` | Yellow tint, ring and ink: the LZ4 tab's Choose source…. |
  | Help | `.help-btn` | The danger colours on a non-destructive button: How LZ4 works (experimental, read first). |
  | Neon | `.neon-btn` | Save as profile: the Packed tile's green straps (static), its label wraps; disabled drops the straps. |
  | Link | `.link` | Quiet text action ("Compare formats"). |

  Every button is a pill and may lead with an icon.
- **Inputs**: pill, `--inset` fill, mono font for paths. A generated output name shows whole in
  `.out-box`, with its folder on the `.out-dir` line below it. Never clip a file name. A typed
  name that can't work (another image extension, or longer than core's name limit: 63 bytes
  before the extension, 58 for `.ffpfsc`, what ShadowMountPlus mounts) is refused in `.field-error` under the box,
  and Build stays off.
- **Switch** (`.switch`): a native checkbox drawn as a switch, blue when on. A disabled switch dims
  its track only; the reason sits under it as a `.note-line.warn` ("Remove backport", shown under the
  format picker only for a source with backport libraries, is disabled when core refuses removal).
  "Full verification" (off by default) sits below it, for every target.
  Both sit in their own inset box (`.field.backport`, `.field.verify`: `--inset`, `--line` border,
  12px radius), apart from the one-liners around them; "Generate name" in the build row does not.
  Convert's one LZ4 control is the **AMPR runtime** switch (`.field.ampr-runtime`, an inset box right
  after the target settings): an AMPR title, not packed, whose `fakelib/libSceAmpr.sprx` is missing
  ("Install the AMPR runtime (ampr_emu <version>)") or not one Forge ships ("Replace …"), the version from
  the inspection (hidden when an older backend leaves it out). On for each new source; its hint says
  the journal and logs are left out, a fresh index is written and only the output changes; on, a
  `.note-line.warn` carries the known saving crash. Otherwise a packed source is copied as it is, and a combination core would
  refuse (Remove backport on a packed dump, or a packed dump carrying the trace runtime) disables
  Build with the reason in a `.note-line.block` beside it, pointing at the LZ4 tab.
- **LZ4 tab** (`Lz4.tsx`): its own Source card, then an "LZ4 asset packs" card. Empty, the Source is
  `.empty.lz4` (yellow icon and glow): "Choose a game that uses AMPR", an "Experimental: … no guarantee"
  lead and one `.lz4-btn` **Choose source…** whose menu asks Game folder… / Image… (the native dialogs pick
  one kind), then the usual picker; Convert's and Inspect's empty cards are unchanged. The card head holds
  a red `.small.help-btn` **How LZ4 works** (any time, no source needed). First the scenario picker, a full-width
  `.seg.formats` in LZ4 yellow (`.lz4-y`: unselected `--lz4-soft`, chosen tint and ring, disabled plain
  `--muted`) without icons: **Trace** and **Pack/Unpack**. A scenario that doesn't apply is disabled with the reason
  in its tooltip and read out, and one muted line under the picker lists it ("Not for this source:
  Trace (the source is LZ4 packed: unpack it first (Pack))"); a title that neither uses AMPR nor is
  packed gets one `.note-line.warn` instead. A new source starts on Pack when it is packed, is a
  read-only image, or (desktop app only) carries a journal; otherwise on Trace. Under the picker
  the scenario's one-liner, then:
  - **Trace** (an AMPR title, not packed): a read-only image (`.ffpfs`, `.ffpfsc`, `.fpkg`) gets one
    `.note-line.warn` (read-only on the console; convert it to `.ffpkg` or `.exfat` first) and
    nothing else. Otherwise, in the web build and only when the source has a journal, a
    `.field.lz4.recorded` box "Recorded traces": what to do first (close the game, wait a minute,
    use a computer's browser, pick the zip in Pack/Unpack → Traces), one download link drawn as a
    secondary button (`a.button` with the download icon, `.downloads` row; its label wraps, as it
    carries the zip's whole name): "Download traces (<size>) as [GAME_NAME]-[TITLE_ID]-amprtrace.zip" (a
    `.note-line.warn` instead when the source has no index), and a muted line that a journal cut
    off at the end still packs (the desktop app: the box holds one muted line, Pack/Unpack loads them by
    itself); both builds end it with the fakelib warning (`.note-line.warn`: few or no traces? turn off
    any fakelib updater, it overwrites `fakelib/libSceAmpr.sprx`). Then an "Action" label and a smaller picker of the same kind
    (`.seg.formats.inner`, LZ4 yellow): **Patch** / **Unpatch** (default Unpatch when the runtime is
    Forge's trace build, else Patch), its one-liner, and one `.field.lz4` box: what it changes
    (Patch: play, then pick the source again; the trace runtime replaces fakelib/libSceAmpr.sprx.
    Unpatch: Forge's release runtime (0.4.2.1) does, the journal and logs are deleted); for an image
    Patch the "Room for the trace" range (64 MiB to 1 GiB, step 64 MiB, default 256 MiB) and the `image_rw=` mount note; for an image
    a muted `.note-line` with the info icon ("Needs free space about the image's size next to it;
    the image is replaced only after the copy verifies."); for Patch the fakelib warning; with a journal a `.note-line.warn` that
    both actions delete it. A folder is changed in place by one request (no verification, output or
    "Generate name"; the button is off while a job runs or waits, with the reason beside it); its
    result: a `.note-line.good` (what was installed, files indexed), a muted line for the old trace
    files removed, and the runtime's known issue as a `.note-line.warn`. An `.exfat` /
    `.ffpkg` is a job that rebuilds it in its own format and replaces it once verified
    (`lz4_in_place`): Full verification shows, no output or "Generate name"; a second one on the
    same image is refused beside the button until the first finishes, and the source is read again
    when it is done. The button reads **Patch** or **Unpatch**.
  - **Pack** (any AMPR source, not packed): in one `.field.lz4` box: **Traces**, one control for a zip or a folder. The web build's
    Choose… opens the browser's zip-or-folder mode; the desktop app's Choose… (with a chevron) opens
    a two-item menu (`.menu`, Zip… / Folder…: the native dialog can't offer both at once). The
    chosen path shows whole with Clear. When the source's own traces load by themselves (journal
    and index, runtime Forge's trace build) the control is locked: a greyed `.lock-box` (dashed
    `--line-hi` border) with a round lock button and "Traces are already loaded from this source",
    and a muted line on what they are. The lock unlocks it (focus moves to Choose…); unlocked, a
    `button.link` with the lock icon, "Use the source's traces", locks it again and drops the
    choice (traces and profile). Under it a collapsed fold (`details.fold.rules`) "Use a rules profile instead" with the
    Profile choice (Choose… / Clear); a profile and traces exclude each other. A muted line says
    where the rules come from (profile, chosen traces, this source's traces, else a built-in guess).
    Under the box a "Target" label and the format picker (`FormatPicker`, `.seg.formats`) with
    **LZ4 packed folder** (default) | **.ffpkg** | **.exfat** | **.ffpfs** (no `.ffpfsc` or `.fpkg`: a UI
    limit, the CLI keeps `.ffpfsc`; the folder converts to any format in Convert),
    the chosen format's one-liner (`FORMAT_INFO`), and for an image a muted line: "Packs into the image directly: no temporary folder; the packed
    files are read twice." The request is `lz4: "pack"` with that `format` (the folder as `folder`).
  - **Unpack** (Pack/Unpack on a packed source): the target picker with Folder (default) | `.ffpkg` |
    `.exfat` | `.ffpfs`, and a line on how many packed files come back.
  Pack and Unpack then show Full verification, Output and "Generate name" as Convert does (shared
  in `target.tsx`); the button reads Pack or Unpack. Left of Pack (not Unpack; before it in the tab
  order, both in `.build-actions`, wrapping under the switch when narrow) a `.neon-btn`
  **Save as profile** (download icon; "Saving…" while it runs; disabled while any job runs or Pack is
  blocked) saves the plan Pack would use with the current settings as a TOML (the app: the save dialog,
  `.toml` filter, `[GAME_NAME]-[TITLE_ID]-lz4profile.toml` offered next to the source; the web UI: a
  download), then a muted note: "Saved <name>: N files packed, M loose. Edit it and load it with Use a
  rules profile." Jobs join the one job list
  (`JobList.tsx`), shown under the cards on both Convert and LZ4.
- **Lead word** (`Lead` in `target.tsx`, `.lead-good` / `.lead-warn`): a Convert or LZ4 note that starts with
  "Recommended" shows that word in `--good`, one that starts with "Experimental" in `--warn`. Text
  colour only (≥ 9:1 on `--card` and `--inset`), no tint; the word itself carries the meaning.
- **Tags** (`.tag`, `.tag.fmt`, semantic colours): 22px pills, 12px/500.
- **Format names**: written without the dot everywhere in the UI (ffpkg, exfat, ffpfs, ffpfsc, fpkg;
  Folder and LZ4 as they are): pills and pickers by their own style, running text in bold (`Prose`
  in `common.tsx`, or `<b>`). Paths and file names keep their extension (`game.ffpkg`).
- **Stat tiles** (`.tiles`, `.tile`, `.value`, `.note`): two per row (four per row at medium widths);
  `.tile.wide` spans the row (the `LZ4 (AMPR)` facts tile: Packed, Damaged packs, Traced or Plain,
  with the volume count, journal size or runtime in its note). A traced source (Forge's trace
  runtime, or a journal) makes that tile `.tile.neon` everywhere it shows (Convert, LZ4, Inspect):
  muted neon straps (`--neon-lime`, `--neon-magenta`, `--neon-cyan`, 12% over `--inset`) across
  the whole tile, a `--neon-line` ring with a faint `--neon-glow`, the state in `--neon-ink`.
  Static. On the brightest strap `--fg` is 12.8:1, `--muted` 5.2:1, `--neon-ink` 10.7:1. A valid
  packed source takes green neon instead (`.tile.neon.packed`, `--pk-*`), over the traced straps;
  damaged packs (`.tile.damaged`) get no straps, the state in `--warn`.
- **Alert bar** (`.alert-bar`, `.alert-bar.warn`): one line of notice.
  - The blue-to-orange gradient is kept dark under the text.
  - Orange is only a glow at the right edge.
  - The warn variant is amber and is used for the `.fpkg` firmware limit (11.60 or lower), under
    the format's one-liner.
- **Findings** (`ul.findings`): `li` warns (amber), `li.block` fails every target (red), `li.info` is
  good news (blue).
- **Folds** (`details.fold`): the DLC list, backport files, emulators in fakelib, job logs, verified
  checks and folders checked. They are collapsed by default; a list inside one that can grow is a scrollable, focusable box.
- **Job row** (`.job`): format tag, name, `from` path, progress bar (`progress`, blue gradient), status
  tag, meta line (`%` · speed · time left). The bar never goes back and stays under 100% until the job
  succeeds. An LZ4 tab job (`.job.lz4`) has a yellow left edge and a `.tag.lz4` (LZ4 pack, LZ4 unpack,
  LZ4 trace, LZ4 unpatch) beside its target's tag (a pack into a folder shows the LZ4 packed folder's);
  Convert's runtime job gets a plain `.tag` "AMPR runtime". Stage "Measuring packs" (its rate counts
  compressed input), and an image pack's write reads "Packing and writing". Once finished, the row shows Show in Finder (Show in Explorer
  on Windows, Show in folder on Linux), the error box or "Fast (Full) verification passed: N checks"; a
  fast one lists its coverage first among the checks.
- **Rating chip** (`.chip.good` / `.ok` / `.bad`): used in the About tab's formats table (Console speed and
  Size on disk per format, the LZ4 row second, after fpkg). The tab holds, in order: an introduction card (one line on what the tool does, a bullet list of the features, each a bold name and a few plain words) and, in the web build only, a second card on its right (`.about-row`: a 300px column, the same height; stacked under 880px): **How to connect on any device** (`.about-connect`), a 200px QR code and the address under it, centred. The app, with no address to open, shows the About card alone, then that table and, under it, a credits block
  (`.credits`: plain selectable text, no links, for ampr_emu and Lazy_AMPR and the note that LZ4 numbers
  are upstream's). Tint, icon and text together, every text ≥ 8.9:1.
- **Empty state** (`.empty`): an inset well with a round blue icon, a title, one line of help, the
  accepted kinds as tags, and two buttons.
- **Modal** (`.modal-backdrop`, `.modal`): 440px card on a blurred dark backdrop; the screens behind it
  are `inert`. `Modal` + `openOverlay` (`common.tsx`) render a dialog in its own root, the whole app
  behind it (header included) inert: Escape and a click outside cancel, Tab stays inside, focus goes
  back to the opener, and a quit request closes them first, so dialogs never stack.
  - **How LZ4 works** (`.modal.help`, 560px, `Lz4Help.tsx`): the info head icon in yellow, a warn line
    (experimental, AMPR only, the saving crash), then Trace and Pack/Unpack as short numbered steps,
    text only; the body scrolls (focusable), title and Close stay.
  - **How to connect** (`.connect-modal`, `Formats.tsx` `openConnect`): the 240px QR code over one
    centred 16px line (any device on the local network: the address, or scan the code), one Close.
  - **Delete** (`role="alertdialog"`, Inspect's trash button on the source card): the warn head icon
    with the trash icon, "Delete this folder?" / "Delete this image?", the whole path in an `.out-box`,
    a muted line (for good, no Trash or undo, may stop part way), and just two buttons: **No** (focused
    first) and **Yes, delete** (`.danger`). No typed confirmation. After it, a muted "Deleted <path>"
    under the empty Source card; Convert and LZ4 drop that source too.

### Web build only (`npm run build:http`, the PS5 payload's UI)

- **Launcher** (`main-http.tsx`, shown before the app while the tile's page starts the payload): a
  modal card on the backdrop, the spinner and "Starting PS5 Dump Forge…" (`role="status"`). On failure
  (`role="alert"`): the warn head icon, "Can't start PS5 Dump Forge" (or "PS5 Dump Forge didn't
  answer"), the reason as muted text, and a primary Retry.
- **Header controls** (`.top-actions`, the header's right column): "Stop PS5 Dump Forge" (`.small`, 14px here
  with the power icon) stands in for closing the window; with jobs running it opens the quit modal
  ("Keep running" / "Stop PS5 Dump Forge"), then "has stopped" with how to start again. While the
  server can't be reached, an `.tag.orange` "Offline, retrying…" sits before it (`role="status"`).
  A failed stop (or a server still answering after 2 minutes) turns the quit modal into "Couldn't
  stop PS5 Dump Forge" with the reason, Close and Try again (`.danger`).
- **Folder and file browser** (`.modal.picker`): the modal frame at 560px.
  - Breadcrumbs (`.crumbs`, mono 12px): "Drives", then each folder as a `.link`, the current one
    plain (`aria-current`). Long paths wrap; they are never clipped.
  - The list (`ul.rows.pick-list`) is an inset well that scrolls inside itself (max 420px): "Parent
    folder" (or "All drives") first, then folders, then files with their size (tabular, muted).
    The file mode lists only the accepted extensions. The zip-or-folder mode (`folderToo`,
    LZ4 → Pack → Traces) lists folders and `.zip` files: a zip click picks it, "Choose this
    folder" the folder shown.
  - Footer: Cancel and, in folder and zip-or-folder mode, "Choose this folder" (`.primary`). Esc cancels, focus
    stays inside, and the next browse opens where the last one ended.
- **Show path** (replaces Show in Finder on a finished job): the output path in an `.out-box`
  (`.path-shown`) under the job's status, focused and selected, to copy by hand (plain HTTP has
  no clipboard API).

Icons come from `icons.tsx`: 24-unit grid, stroke 1.8, `currentColor`, shown at 18px (15px in small
spots). Add a path there rather than importing an icon set.

## Cover-derived glow (`glow.ts`)

- The cover (a `data:` URL) is drawn to a 24×24 canvas.
- Pixels are bucketed by hue into 12 bins of 30°. Near-greys, near-blacks and near-whites are skipped.
- The one or two most saturated bins become `--glow-1` and `--glow-2`.
- Both are dimmed until the card's brightest point keeps muted text ≥ 4.9:1 (max relative luminance
  0.028).
- The result is cached per cover for the session.
- With no cover, or on any failure, the card uses the brand glow (blue `rgba(61,107,255,.13)` + orange
  `rgba(255,122,47,.08)`).
- Old WebKit (the PS5's browser): the cover loads through `onload` (no `decode()`, 5 s timeout), and
  the `::before` layer uses four offsets (not `inset`) inside a `z-index: 0` stacking context.

## Motion

Only short fades and transforms: the hero glow (0.5s), the switch and fold chevrons (0.15s), and the
quit spinner. `prefers-reduced-motion: reduce` turns all of them off.

## Copy

- Plain user language: say what a format is *for*, not its spec. Formats are explained in the About
  tab (`Formats.tsx`).
- The debug package is labelled `.fpkg` everywhere it is shown. Files are still saved as `.pkg`, and
  the internal id and the CLI flag stay `pkg`.
- Errors lead with a sentence a user understands ("Can't build this game as .exfat:") and then quote
  the core's lines unchanged.
