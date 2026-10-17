# PS5 Dump Forge UI design system

The rules behind `app/src/styles.css`: the CSS holds the exact values, this file says when to use what.
The look follows the SpendIQ Dribbble shot (dark cards, pill buttons, inset stat tiles, tinted tags, a
gradient notice bar) and the app icon (near-black navy with a blue edge and an orange glow).

## Principles

- **Dark only** (`color-scheme: dark`). A light theme would need a second token set.
- **One hero.** Only the game card (`.card.hero`) glows, in colours taken from its cover (`glow.ts`).
- **Pills for controls.** Buttons, inputs, tags and segmented controls are fully rounded (999px);
  cards 16px, inset tiles 12px.
- **Facts in inset tiles.** A value the user decides on (title ID, version, firmware, size) sits in a
  darker `.tile`: a muted label above a large value.
- **Colour never works alone.** A format has its hue *and* its label; a rating its tint *and* an icon
  (✓ – ✕) *and* text; a selected segment is tinted *and* ringed *and* bold.
- **Contrast.** Text is at least 4.5:1 on its ground, the hero glow and the alert bar included. The
  CSS comments give the measured ratio wherever a pair is tight.
- **Fits the window.** Default 1200×820, minimum 900×620 (`main.rs`); the web build down to 360px
  wide. No horizontal page scroll; long lists scroll inside their own box, except the job list,
  which grows with the page.
- **Offline and CSP-safe.** System fonts, inline SVG icons, the logo bundled by Vite; no remote assets,
  no injected `<style>`. The only inline styles are CSS variables set through React's `style` (CSSOM).

## Tokens (`:root`)

| Token | Value | Use |
|---|---|---|
| `--page` | `#0e0e15` | Window background, sticky header |
| `--card` | `#17171f` | Cards |
| `--inset` | `#101017` | Tiles, inputs, folds, table and list wells |
| `--raise` / `--raise-hi` | `#24242f` / `#2e2e3b` | Secondary buttons / hover, the selected tab |
| `--line` / `--line-hi` | white 7% / 12% | Borders and row dividers / input borders |
| `--fg` / `--fg-2` | `#f2f2f7` / `#d2d2dc` | Text / secondary text (paths, checks) |
| `--muted` | `#9a9aae` | Labels, meta, unselected segments (≥ 4.5:1 on every ground) |
| `--accent` / `--accent-ink` | `#3d6bff` / `#9db6ff` | Switch on, progress, empty-state icon / link text |
| `--orange` | `#ff7a2f` | Glow only (logo shadow, hero and alert-bar edge), never text |
| `--good` / `--bad` / `--warn` | `#2fd27a` / `#ff6b6b` / `#ffb547` | Status icons and text |
| `--focus` | `#9db6ff` | 2px focus ring on every control (`:focus-visible`) |
| `--mono` | `ui-monospace, SFMono-Regular, Menlo` | Paths, file names, IDs, param.json, logs |
| `--tab-lz4`, `--strap` / `--strap-hi` | `#1c1c26`, yellow 12% / 20% | The LZ4 screen tab's ground and straps |
| `--lz4-ink` / `-ink-hi` / `-soft` | `#ffd84d` / `#ffe27a` / `#e3c766` | LZ4 yellow text: the tab, its pickers, Choose source, LZ4 job tags, the Library's LZ4 badge |
| `--lz4-tint` / `-tint-hi` / `-line` | yellow 14% / 22%, ring 50% | Their tint, hover and ring (`--lz4-ink` on tint ≥ 9.3:1, `--lz4-soft` 11.4:1) |
| `--library-ink` / `-ink-hi` / `-line` | `#a8ed8b` / `#c4f6aa` / green 50% | Library green: its header icon and the selected view button only |
| `--neon-*` | lime, magenta, cyan 12%; ring lime 38%; ink `#d6e875` | A traced source's `LZ4 (AMPR)` tile |
| `--pk-*` | green 14%, cyan 12%; ring 42%; ink `#86f5b4` | A packed source's tile |
| count badge (`.count`) | `#2f5bff` + white | Active jobs on the Convert tab, the Library's count (5.2:1) |
| primary button | `#fff` + `#0e0e15` | The one main action per card |

**Format hues.** `.fmt-*` sets `--h` (text), `--h-bg` (tint) and `--h-line` (ring); text on tint ≥ 4.5:1.

| Format | Class | Hue |
|---|---|---|
| Folder | `fmt-folder` | white `#ececf4` |
| exfat | `fmt-exfat` | magenta `#ffa3e6` |
| ffpkg | `fmt-ffpkg` | blue `#a3c0ff` |
| ffpfs | `fmt-ffpfs` | teal `#7ee0e0` |
| ffpfsc | `fmt-ffpfsc` | amber `#ffc580` |
| fpkg | `fmt-pkg` | green `#8fe0ab` (Convert's default target) |
| LZ4 packed folder | `fmt-lz4` | lime `#d6e875` (only the LZ4 tab's Pack target picker offers it) |

**Semantic tags.** Tinted pills for states: `.tag.blue` Writing, `.tag.green` Done, `.tag.red` Failed,
`.tag.orange` Backport, `.tag.violet` DLC and Additional content (a source without `eboot.bin`, beside
the other labels on every tab, Library cards and rows too). A plain `.tag` is neutral (Emulators, AMPR runtime).

**Type.** System font (SF Pro on macOS), 14px/1.45 body.

| Size / weight | Use |
|---|---|
| 20/600 | Game title |
| 18/600 | Tile values (tabular numbers) |
| 18/500 | Card titles |
| 16/600 | Empty-state, modal and Library titles |
| 13.5/600 | Job names |
| 13 | Controls and body copy |
| 12.5 | Labels, meta, notes |
| 12 mono | Paths |
| 11.5 | Hints, under-tile notes |

**Spacing.** Page gutters 28px, content max 1200px, 16px between
cards, card padding 20–22px, 14px from card head to body, 12px between groups in a card, 8px between
tiles and tags. Two columns from 880px; Edit splits 1:2 from 1100px (its game card is the narrow one).

## Components (class names in `styles.css`)

- **Header** (`.topbar`, sticky, no bottom border): the 30px logo left, the screen tabs centred
  (`.seg.nav`: 14px labels, a 1% white fill over a 28px backdrop blur), the GitHub mark (`.gh-link`,
  32px round, `--muted`) rightmost; the app opens it in the default browser through the fixed-URL
  `open_repo` command. No wordmark and no version: the window or page title carries both ("PS5 Dump
  Forge v…" in the app, "Dump Forge v…" in the web build; `APP_NAME` in `api.ts`).
  Under 900px: two rows, logo and actions over the full-width tabs (`--top` 98px); the tab bar scrolls
  inside itself. At ≤ 420px the tab padding tightens so all five tabs fit at 360px.
- **Segmented control** (`.seg`, `.seg-opt`, `.on`): the screen tabs (Convert, Edit | LZ4 | Library |
  About: one tablist, `.tab-sep` dividers drawn only, arrow keys move through every tab), the format
  picker (`.seg.formats`), Edit's mode choice and the LZ4 tab's scenario and action pickers. Native
  radios or `role="tab"` sit underneath for the keyboard.
  - The LZ4 screen tab (`.seg-opt.tab-lz4`): a `--tab-lz4` pill with four static yellow straps from top
    right to bottom left and `--lz4-ink` text; selected, `--raise-hi`, `--strap-hi`, `--lz4-ink-hi`, bold
    and a `--lz4-line` ring. The Library tab is plain, with the books icon (16px) before its text.
  - Format picker: the selected segment fills with its format tint and ring, bold; unselected segments
    are plain muted text. Segments are sized by their labels, then share the spare width.
  - Smaller pickers of the same kind (`.seg.formats.inner`): the image inside an ffpfsc (shown only for
    that target) and the fpkg Compression picker (Fast / Balanced / Smallest / Stored, 15px bolt, scale,
    compress and file icons, the fpkg tint), each with one line under it on what the choice costs.
  - The ffpfsc Compression level is a native range (`.range`, `accent-color`, 0–9, default 6) with its
    value in the label row (`.range-value`, tabular).
- **Card** (`.card`, no border: `--card` on `--page` plus a shadow; `.card-head` = 18px icon, 18px
  title, actions right; `.card.compact`). `.card.hero` adds the glow: two radial gradients from the top
  right corner, faded in once its colours are known (`.lit`).
- **Buttons.** Every button is a pill and may lead with an icon.

  | Kind | Class | Use |
  |---|---|---|
  | Primary | `.primary` | White. One per card (Build, Pack, Edit's Apply, Choose folder on an empty Library). |
  | Secondary | default | Dark. Choose, Change, Show in Finder (Explorer, folder), download links (`a.button`). |
  | Danger | `.danger` | Red tint. Stop job, Stop jobs and quit, the trash buttons, Yes, delete. |
  | Small | `.small` | Job rows; `.small.icon-only` is a 30px round icon button. |
  | LZ4 | `.lz4-btn` | Yellow tint, ring and ink: the LZ4 tab's Choose source. |
  | Help | `.help-btn` | The danger colours on a harmless button: How LZ4 works (experimental, read first). |
  | Link | `.link` | Quiet text action (Compare formats). |
- **Update toast** (`.update-toast`, bottom corner, mirrored in RTL): the Done green (`.tag.green`), a 14% tint
  over `--card`, a 45% ring, `#6fe3a3` icon and title, its primary button green with dark ink.
- **Inputs**: pill, `--inset` fill, mono for paths. A generated output name shows whole in `.out-box`,
  its folder on the `.out-dir` line below; a file name is never clipped. A typed name that can't work
  (another image extension, longer than core's name limit, blank, a leading dot, a trailing dot or
  space, a control character) is refused in `.field-error` under the box, and Build stays off; so
  does a name an unfinished job is already writing. **Change** opens the picker at the output's
  folder.
- **Right-to-left** (Arabic, `<html dir="rtl">`): the layout mirrors through flexbox and logical
  properties (`margin-inline-*`, `padding-inline-*`, `text-align: start/end`); the few physical
  positions (switch knob, menus, the LZ4 badge, the LZ4 job edge, the select's chevron) have
  `[dir="rtl"]` overrides at the end of styles.css. Paths, file names, the output name, job names,
  the backend's English (`.backend`: errors and core's checks; job logs) and number + unit values
  (`.value`, sizes) stay left-to-right, isolated from the text around them (in Edit: the names, file
  sizes, rename input and param.json); the steps' "›", the progress bar's bright head and the
  explorer's open chevron mirror.
- **Language select** (`select.lang-select`, right in the About card's header after a muted
  "Language" label, `.head-lang`): the one dropdown. A native `<select>` drawn as a small pill
  (`button.small`'s 30px and 12.5px, `--inset`, `--line-hi` border, `--focus` ring), `appearance: none`
  with the `chevron` icon in `--muted` as its background; options are System, then each language in its
  own words (`lang` set), so it holds any number of languages.
- **Switch** (`.switch`): a native checkbox drawn as a switch, blue when on. Disabled, only the track
  dims; the reason sits under it as a `.note-line.warn`. Remove backport (only for a source with
  backport libraries) and Full verification (off by default) each sit in their own inset box
  (`.field.backport`, `.field.verify`: `--inset`, `--line` border, 12px radius). A combination core
  would refuse disables Build with the reason in a `.note-line.block` beside it. For an AMPR game,
  Convert says under Build what the conversion does by itself, each a muted `.note-line` with the
  `info` icon, from core's `runtime_action`: the bundled ampr_emu (its version) is installed (none
  there) or replaces the game's runtime (older or unknown), or the game's own one is kept (the bundled
  version or newer, named); and a stale, damaged or missing `ampr_emu.index` is written afresh (the old
  traces left out; for fpkg, the package always gets a fresh one). A game that doesn't use AMPR gets none.
- **LZ4 tab** (`Lz4.tsx`): a Source card, then an "LZ4 asset packs" card whose head holds a red
  `.small.help-btn` **How LZ4 works**. The empty Source is `.empty.lz4` (yellow icon and glow): "Choose a
  game that uses AMPR", an experimental lead and one `.lz4-btn` **Choose source** with a Folder /
  Image/Archive menu.
  - The scenario picker is a full-width `.seg.formats` in LZ4 yellow (`.lz4-y`: unselected `--lz4-soft`,
    chosen tint and ring, disabled plain `--muted`, no icons): **Pack/Unpack** (first, the default
    whenever it applies) and **Trace**. A disabled scenario carries its reason in its tooltip and in one
    muted "Not for this source: …" line under the picker; a title that neither uses AMPR nor is packed
    gets one `.note-line.warn` instead.
  - Settings sit in `.field.lz4` inset boxes. **Trace**: in the web build, when the source has a
    journal, a `.field.lz4.recorded` box "Recorded traces" with one wrapping download link (`a.button`,
    `.downloads`); then a `.seg.formats.inner` **Patch** / **Unpatch** picker and a box with what it
    changes, for an image Patch the "Room for the trace" range, and the fakelib warning. A read-only
    image gets one `.note-line.warn` and nothing else. **Pack**: Traces (a zip or a folder; the app's
    Choose opens a Zip / Folder `.menu`), locked to the source's own traces when they load by
    themselves (`.lock-box`: dashed `--line-hi` border, a round lock button; unlocked, a `button.link`
    "Use the source's traces" locks it again), a collapsed `details.fold.rules` "Use a rules profile
    instead", then the Target picker: Folder (the LZ4 packed folder, shown as Folder) | ffpkg (default) | exfat | ffpfs. **Unpack**:
    Folder | ffpkg (default) | exfat | ffpfs.
  - Pack and Unpack end like Convert (Full verification, Output, Generate name; shared in
    `target.tsx`).
- **Edit tab** (`Edit.tsx`, `EditChanges.tsx`, `EditApply.tsx`; `.cols.edit`, 1fr:2fr from 1100px). Before a
  source: the empty Source card left, "How this works" (`.how`: short paragraphs, the last muted) right.
  - **Compact game card** (`SourceCard compact`, `.hero.compact-src`; smaller than the other tabs' by user
    decision, so the explorer gets the room): padding 14px 16px, the hero glow, a 56px cover (radius 10px),
    a 16px title, the title ID and version tiles (6px gap) (two per row at every width) and the findings; under it, folds (`.game-folds`)
    Game facts, param.json and Details, long contents in a scrolling `.fold-box`; then leftovers. An
    interrupted edit (`<image>.forge-undo` beside it) adds a `.note-line.warn` (`.interrupted-note`).
  - **Explorer** (`.explorer` card), top to bottom: "Game files" with the file count as a `.tag` (an
    archive adds a note-line that it can be browsed, not edited); the pending bar (below); Up and the
    crumbs (`.explorer-nav`, "Game" / …, the current folder `aria-current`); the toolbar
    (`.explorer-tools`, `.small`: Add files, Add folder, then **Extract folder**, the folder shown or at
    the root the whole game; no New folder button, by user decision; an archive gets Extract folder only;
    with changes pending a muted `.extract-note` says Extract copies the files as they are now); the filter and a live count ("N folders, M files", "N matches", "first 2000 of N"). The well (`.explorer-well`: `--inset`,
    scrolls inside; from 880px the card is sticky and window-high, narrower the well stops at 60vh) is a
    `role="grid"` of rows (`.xrow`: icon, mono name that wraps anywhere, tabular muted size, actions): folders
    first with the `--accent-ink` icon, then files; a sticky header row. Rows take a roving `tabIndex`; the
    focused row tints, `:focus-visible` adds the ring. Row actions (`button.xbtn`: Rename (pencil), Extract
    (download; rows on disk only), Delete in danger hover, and a folder's open chevron) are quiet icon
    buttons with no fill until hover, in a fixed column (`.xgrid.editable` 126px; `.xgrid.extractable`, an
    archive: Extract and open, 62px) so sizes line up. Rename edits inline (`.xinput`, mono pill; the reason under it
    as a `.field-error`). Where dragged files would land rings in `--focus`: the folder row (`.xrow.drop`,
    blue tint) or the whole well (`.explorer-well.drop`), and the count line says "Drop to add to …".
  - **Badges** (`.tag.xmark`, 20px so a row keeps its height): **New** green, **Replaced** orange,
    **Renamed** blue with "Was <path>" as its tooltip. A size read only on Apply shows "—" with a tooltip.
  - **Pending bar** (`.pending-bar`, under the explorer's head while changes are pending): a blue-tinted inset (state, not an error) with
    "N changes not applied yet", **Undo**, **Discard all** (`.danger`) and **Apply** (`.small.primary`) in
    its head; a warn line when some changes won't apply; a "Changes, in order" fold (`ol.changes`, each
    change, its source path, its findings).
  - **Mode box** (`.edit-mode`, in the bar): one tag and a line: `.tag.blue` **Changed directly** (a
    folder), `.tag.green` **In place** ("needs N of its M free"), `.tag.red` **Rebuild needed** with the
    reason, or `.tag.red` **Can't apply** with core's findings (`.edit-why`, a scrolling `.backend` list).
    When both ways work, a `.seg.formats.inner` radiogroup (`.mode-hue`, by user decision): **In place** in
    the Done green (`.mode-green`), **Rebuild** in the Failed red (`.mode-red`); an unchosen option's text
    keeps its hue, the chosen one takes the tint and ring; one muted line on what the choice does. A muted "Checking" while an answer is due; a failed check a
    `.note-line.block` and **Check again**. While locked it says the changes are being applied.
  - **Uploads** (web build, `.upload-line`): a full-width `<progress>`, "Uploading file i of n: sent of
    total" and **Cancel**. Apply's error sits in the bar as an `.error-box`.
  - **Dialogs** (`role="alertdialog"`, warn icon, Cancel or No focused): the conflict question ("N items
    already exist here", the folder, the names in a scrolling `.conflict-names` box, the replace warning;
    Cancel, Skip, Replace `.danger`), Discard all (above one change; No, Discard all), and Apply's
    confirm (In place: written straight into the path, no undo; Rebuild: a new image beside, put in its
    place; close the game first; Cancel and a primary Apply).
- **Lead word** (`Lead` in `target.tsx`): a note starting with "Recommended" shows that word in
  `--good` (`.lead-good`), one starting with "Experimental" in `--warn` (`.lead-warn`). Text colour only
  (≥ 9:1 on `--card` and `--inset`); the word itself carries the meaning.
- **Tags** (`.tag`, `.tag.fmt`, semantic colours): 22px pills, 12px/500.
- **Format names** go without the dot in the UI (ffpkg, exfat, ffpfs, ffpfsc, fpkg; Folder and LZ4 as
  they are): pills and pickers in their own style, running text in bold (`Prose` in `common.tsx`, or
  `<b>`). Paths and file names keep the extension (`game.ffpkg`).
  - **Archive pills** (`.fmt-archive`): zip, 7z and rar share one sand hue (text 8.7:1 on its tint over
    `--card`, 9.5:1 over `--inset`); the label tells them apart. Sources only: the empty state's
    accepted kinds and an archive source's head, never a target picker.
- **Stat tiles** (`.tiles`, `.tile`, `.value`, `.note`): two per row, four from 680 to 879px;
  an archive source adds an "in a .7z archive · <size>" `.tag` to the Size tile everywhere and, in
  Edit's Game facts, a `.tile.wide` **Archive** tile: the part count as the value, then "game in <folder>" (or
  "at the archive's root") · solid · password-protected as its note, and the part names as a `.path`
  note when there are several.
  `.tile.wide` spans the row (the `LZ4 (AMPR)` tile: Packed, Damaged packs, Traced or Plain; its note
  adds what a conversion does to the runtime (installs or replaces with the bundled ampr_emu, or keeps
  another build's named version) and a stale, damaged or missing `ampr_emu.index`; an index to refresh
  or a runtime to install also makes the tile show). A traced
  source's tile is `.tile.neon` wherever it shows: static muted lime, magenta and cyan straps over
  `--inset`, a `--neon-line` ring with a faint `--neon-glow`, the state in `--neon-ink` (on the
  brightest strap `--fg` 12.8:1, `--muted` 5.2:1, `--neon-ink` 10.7:1). A packed source takes green
  straps instead (`.tile.neon.packed`, `--pk-*`: `--pk-ink` 10.4:1, `--muted` 5.0:1); damaged packs
  (`.tile.damaged`) get no straps and the state in `--warn`.
- **Alert bar** (`.alert-bar`): one line of notice on a blue-to-orange gradient kept dark under the
  text, orange only as a glow at the right edge. `.alert-bar.warn` is amber: the fpkg firmware limit.
- **Findings** (`ul.findings`): `li` warns (amber), `li.block` fails every target (red), `li.info` is
  good news (blue). Under Build, `.note-line` (`.warn`, `.block`, `.good`) says why it is off or what to
  expect.
- **Folds** (`details.fold`): DLC, backport files, emulators in fakelib, job logs, verified checks,
  folders checked. Collapsed by default; a list inside that can grow is a scrollable, focusable box.
- **Job list** (`JobList.tsx`, `.card.jobs`): one card under the other cards on Convert, Edit and LZ4, newest
  first; it fills the rest of the window when short and grows with its rows (the page scrolls, never
  a box inside it). A row (`.job`): format tag, name,
  `from` path, progress bar (`progress` in a `.bar` wrapper, blue gradient; it never goes back and stays
  under 100% until the job succeeds; a sheen while the job runs, see Motion), status tag (no dots),
  meta line (% · speed · time left, the last two only in a pass that streams the game's bytes: compress, write, measure, pack, verify; then "0:42 in this step" once a stage has run 10 s; m:ss, h:mm:ss
  past an hour; the timer alone for a stage without a total), and the step row (`ol.steps`, under the
  bar; under the status line while the job waits without one): every stage core said the job will run,
  in order, short labels (`jobs.steps`), neighbours with one label merged (preflight and check are one
  Checking), "›" between them (mirrored in RTL), wrapping, 12.5px. Done steps: a small green check and
  `--fg-2`; the current one: `--accent-ink`, 600, `aria-current="step"`; later ones `--muted`. Screen
  readers hear "done" / "not started yet" (visually hidden). No row until the stages are known, none once
  the job has ended; no motion of its own. An LZ4 tab job (`.job.lz4`) has a
  yellow left edge and a `.tag.lz4` (LZ4 pack, unpack, trace, unpatch); an unpatch conversion sent to the
  server's API (not the LZ4 tab's in-place one) a plain `.tag` "AMPR runtime"; an edit job a plain `.tag` **Edit** or **Edit · rebuild**, its error box led
  by "Can't apply the changes:"; an extract job a plain `.tag` **Extract** ("Can't extract:"). Finished: Show in Finder (Explorer on Windows, folder on Linux), the error box,
  or "Fast (Full) verification passed: N checks".
- **Rating chip** (`.chip.good` / `.ok` / `.bad`): the About tab's formats table (Console speed and
  Size on disk). Tint, icon and text together, every text ≥ 8.9:1. About holds an introduction card
  (one line, then the features as a bold name plus a few words: ten rows, Convert … In place or rebuilt, as
  many as fill the QR card's height at 1200px with the content flush), in the web build beside a 300px
  **How to connect on any device** card (`.about-row`, `.about-connect`: a 200px QR code over the
  address; stacked under 880px), then the table and a `.credits` block (plain selectable text, no
  links).
- **Empty state** (`.empty`): an inset well with a round blue icon (`.empty-icon`; `.bad` red for an
  error), a 16px title, one line of help, the accepted kinds as tags, and its buttons.
  - **Source pickers** (`PickButtons` in `common.tsx`): **Folder** (primary, folder icon) and
    **Image/Archive** (secondary, archive icon; one file dialog for images, packages and archives,
    `.001`/`.z01` first parts included). Once a source is chosen the same two sit in the Source card's
    head as `.small` buttons.
- **Modal** (`.modal-backdrop`, `.modal`): a 440px card on a blurred dark backdrop. `Modal` +
  `openOverlay` (`common.tsx`) render it in its own root with the whole app behind it `inert`: Escape
  and a click outside cancel, Tab stays inside, focus returns to the opener, a quit request closes it
  first, so dialogs never stack.
  - **How LZ4 works** (`.modal.help`, 560px, `Lz4Help.tsx`): the info icon in yellow, a warn line, then
    Trace and Pack/Unpack as short numbered steps; the body scrolls (focusable), title and Close stay.
  - **How to connect** (`.connect-modal`, `openConnect` in `Formats.tsx`): the 240px QR code over one
    centred 16px line, one Close.
  - **Password** (`.modal.password`, `password.tsx`): opens when inspecting an archive answers "needs
    a password". The lock icon and "This archive needs a password", the path in an `.out-box`, one line
    (muted; `.bad` "The password is not correct." after a failed try, `aria-live`), a full-width
    `type="password"` input (`.pw-input`, focused, selected again after a wrong try), **Cancel** and a
    primary **Unlock** ("Checking…" while it runs). Cancel leaves the source locked: the Source card
    shows the red reason and a `.small` **Enter password** (lock icon) in `.row.locked`. The password
    stays in memory with that source (inspect, naming and its jobs), never stored or shown.
  - **Delete** (`confirmDelete` in `delete.tsx`, `role="alertdialog"`, shared by Edit and the
    Library): the warn icon, "Delete this folder?" / "Delete this image?", the whole path in an
    `.out-box`, a muted line (for good, no Trash or undo), and two buttons: **No** (focused first) and
    **Yes, delete** (`.danger`). Afterwards a muted "Deleted <path>".

- **Library tab** (`Library.tsx`, `.library`): one card. Head: the green library icon, "Library" (16/600)
  and its count; right (`.library-ctl`, wraps): on a computer the chosen folder (muted `PathLine`),
  **Refresh**, **Choose folder**, then the Card / List buttons (`.library-views`, icon-only,
  `aria-pressed`, selected `--library-ink-hi` on `--raise-hi` with a `--library-line` ring). On the PS5:
  the internal storage's free space first (`.library-storage`: drive icon, "Free **312 GB** of 825 GB"
  in decimal GB, `--muted` 6.5:1 with the free amount in `--fg-2`), then Refresh and the view buttons.
  Under the head the scrolling well (`.library-well`: `--inset`, 14px padding, 160px minimum, height to
  the viewport, focusable, `aria-label` "Games"); the list scrolls inside it, never the page. Under
  900px the well grows with its list and the page scrolls instead (no box to scroll on a phone).
  - **Cards** (`.library-cards`, `minmax(240px, 1fr)` columns, 16px gaps): each card (`--card`, 14px
    radius, no glow, no outline) has a square cover (10px radius, a `padding-top: 100%` box for older
    WebKit), the name (600, wraps anywhere), the title ID and version muted, the path line (copies
    of one game differ only there), and a foot with the format pill and the actions. **List** (`.library-rows`): 48px cover, name, title
    ID and version, format pill, path line and actions on one wrapping row. Covers load lazily, one at a time; a
    placeholder with the disc icon stands in at the same size.
  - **LZ4 badge** (`.lz4-badge`, an entry with ampr_emu packs): "LZ4" in bold `--lz4-ink` on a
    liquid-glass pill: a faint white gradient, a light rim brighter on top, a soft dark text shadow,
    no colour of its own. On a card it sits on the cover's top-right corner (24px) over a light dark
    tint with a backdrop `blur(14px) saturate(160%) brightness(0.55)` (`-webkit-` prefixed for the PS5
    browser), so it stays readable on any cover; in a row it follows the title ID (20px, ≥ 7:1 on
    `--card`). It reads "LZ4 packed" to a screen reader; the format pill stays the entry's own.
  - **Actions**: icon-only buttons with `aria-label` and `title` ("Edit <name>" with the pencil icon,
    "Convert <name>", "Delete <name>", the last `.danger`). Edit and Convert switch to that tab with the game loaded;
    Delete opens the shared Delete dialog, then focus moves to the next entry (or Refresh); a failure
    shows an orange "Can’t delete:" tag + the reason.
  - **States**: scanning, a centred spinner and "Scanning <folder name>…" in the well (`role="status"`,
    `aria-busy`). A computer with no folder yet: a centred `.empty` panel with the folder icon, **Pick a
    folder with your library to scan** and a primary **Choose folder**. A chosen folder refused or
    failed: the same panel with the red icon, **Can’t scan this folder**, core's message and **Choose
    another folder**. A failed Refresh keeps the list under an orange `.tag` "Can’t scan the
    Library:" + reason. A hit scan limit: an orange tag "Scan limit reached. This list may be incomplete." and a
    "Skipped locations and files" fold. No roots: "No folders are available to scan."

### Web build only (`npm run build:http`, the PS5 payload's UI)

The web build names the product "Dump Forge" (the PS5 tile's name); the desktop app "PS5 Dump Forge".

- **Launcher** (`main-http.tsx`, before the app while the tile's page starts the payload): a modal
  card with the spinner and "Starting Dump Forge…" (`role="status"`); on failure (`role="alert"`) the
  warn icon, "Can't start Dump Forge" (or "Dump Forge didn't answer"), the reason muted, a primary
  Retry.
- **Address** (`.address-btn`, right of the logo): the server's host:port from the session's `url`
  (never the viewer's `location`) in `.address` (mono 600 15px `--fg-2`, ellipsized first). It opens
  **How to connect**; no QR code when `/api/qr` fails.
- **Header controls** (`.top-actions`): "Stop Dump Forge" (`.small`, 14px, power icon) before the
  GitHub mark; under 600px it shrinks to its icon (the label stays as visually hidden text), as it
  does at 900–1049px while the orange `.tag` "Offline, retrying…" (`role="status"`) sits before it.
  With no job running it stops at once; with jobs it opens the quit modal listing them (Keep
  running / Stop Dump Forge); if they all finish while it is open, it asks "Stop Dump Forge?" and
  says how it starts again (the PS5: its tile; a computer's `serve`: run it again). Then "has
  stopped" with the same. Once Stop is confirmed the dialog takes no keys or clicks:
  Escape stays the browser's, so the PS5's Circle leaves the page. A failed stop turns it into
  "Couldn't stop Dump Forge" with the reason, Close and Try again (`.danger`).
- **Folder and file browser** (`.modal.picker`, 560px): breadcrumbs (`.crumbs`, mono 12px: "Drives",
  then each folder as a `.link`, the current one plain with `aria-current`; long paths wrap, never
  clipped, and scroll inside the crumbs once they would push the buttons off screen), then an
  inset list (`ul.rows.pick-list`, scrolls inside, max 420px): Parent folder (or All
  drives), folders, files with their size (tabular, muted). File mode lists only the accepted
  extensions (Edit's Add file pickers: every file; Add files toggles a selection within the folder
  shown: the row tinted and ringed, its icon a check, `aria-pressed`, then a primary "Choose N files");
  zip-or-folder mode (LZ4 Pack Traces) lists folders and `.zip` files. Footer: Cancel and,
  in folder modes, "Choose this folder" (`.primary`). The next browse opens where the last one ended.
- **Show path** (instead of Show in Finder on a finished job): the output path in an `.out-box`
  (`.path-shown`), focused and selected, to copy by hand (plain HTTP has no clipboard API).

Icons come from `icons.tsx`: 24-unit grid, stroke 1.8, `currentColor`, shown at 18px (15px in small
spots). Add a path there rather than importing an icon set.

## Cover-derived glow (`glow.ts`)

- The cover (a `data:` URL) is drawn to a 24×24 canvas; pixels are bucketed by hue into 12 bins of
  30°, skipping near-greys, near-blacks and near-whites.
- The two most weighted saturated hues (the second at least 60° from the first) become `--glow-1` and
  `--glow-2`, dimmed until the card's brightest point keeps muted text ≥ 4.9:1 (relative luminance
  ≤ 0.028). Cached per cover for the session.
- No cover, or any failure: the brand glow (blue `rgba(61,107,255,.13)` + orange `rgba(255,122,47,.08)`).
- Old WebKit (the PS5's browser): the cover loads through `onload` (no `decode()`, 5 s timeout), and
  the `::before` layer uses four offsets (not `inset`) inside a `z-index: 0` stacking context.

## Motion

Only short fades and transforms: the hero glow (0.5s), the switch and fold chevrons (0.15s), the
spinners, the dots after a waiting text (`Dots`, `.dots`: ".", "..", "..." in turn over 1.2s; Edit's
"Reading the source", the Library's "Scanning", the picker's "Loading"), and a job bar's sheen (`.bar
.sheen`: the whole time a job runs, since many stages move the bar little or in steps and a
stall-only sheen flickered, a soft
blue-white band sweeps the filled part every 1.6s, ease-in-out, by `transform` only on an overlay as wide
as the fill (`--p`), since old WebKit animates nothing inside `<progress>`; under 2% fill it crosses the
whole track at 60%). `prefers-reduced-motion: reduce` turns all of them off. Straps are static.

## Copy

- Plain user language: say what a format is *for*, not its spec. The About tab (`Formats.tsx`)
  explains the formats.
- The debug package is labelled fpkg wherever it is shown; files are saved as `.pkg`, and the internal
  id and the CLI flag stay `pkg`.
- Errors lead with a sentence a user understands ("Can't build this game as **exfat**:"), then quote
  the core's lines unchanged.
