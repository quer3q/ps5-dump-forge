# PS5 Dump Forge UI design system

The rules behind `app/src/styles.css`: the CSS holds the exact values, this file says when to use what.
The look follows the SpendIQ Dribbble shot (dark cards, pill buttons, inset stat tiles, tinted tags, a
gradient notice bar) and the app icon (a blue portal-and-anvil emblem with an orange flame).

## Principles

- **Dark only** (`color-scheme: dark`). A light theme would need a second token set.
- **One hero.** Only the game card (`.card.hero`) glows, in colours taken from its cover (`glow.ts`).
- **Pills for controls.** Buttons, inputs, tags and segmented controls are fully rounded (999px);
  cards 16px, inset tiles 12px (a segmented control that wraps rounds to 16px).
- **Facts in inset tiles.** A value the user decides on (title ID, version, firmware, size) sits in a
  darker `.tile`: a muted label above a large value.
- **Colour never works alone.** A format has its hue *and* its label; a rating its tint *and* an icon
  (✓ – ✕) *and* text; a selected segment is tinted *and* ringed *and* bold.
- **Contrast.** Text is at least 4.5:1 on its ground, the hero glow and the alert bar included. The
  CSS comments give the measured ratio wherever a pair is tight.
- **Fits the window.** Default 1200×820, minimum 900×620 (`main.rs`); the web build down to 320px
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
| `--line-lo` | white 4% | The faint edge of tiles, fold summaries and inset wells |
| `--fg` / `--fg-2` | `#f2f2f7` / `#d2d2dc` | Text / secondary text (paths, checks) |
| `--muted` | `#9a9aae` | Labels, meta, unselected segments (≥ 4.5:1 on every ground) |
| `--accent` / `--accent-ink` | `#3d6bff` / `#9db6ff` | Switch on, progress fill, range inputs / link text, empty-state and folder icons, the info finding icon, a job's current step |
| `--orange` | `#ff7a2f` | The glow's orange (logo shadow, hero and alert-bar edge, written there as `rgba(255, 122, 47, …)`), never text |
| `--good` / `--bad` / `--warn` | `#2fd27a` / `#ff6b6b` / `#ffb547` | Status icons and text |
| `--focus` | `#9db6ff` | 2px focus ring on every control (`:focus-visible`) |
| `--sans` | `-apple-system, BlinkMacSystemFont, "SF Pro Text", "Segoe UI", system-ui, sans-serif` | Body text; placeholders and the Naming text inside mono boxes |
| `--mono` | `ui-monospace, SFMono-Regular, Menlo, monospace` | Paths, file names, IDs, param.json, logs |
| `--top` | `57px`; `98px` under 900px | The sticky header's height: what sits under it and focus scrolling clear of it |
| `--lz4-ink` / `-soft` | `#ffd84d` / `#e3c766` | Pack yellow text: the Pack tab's zstd \| LZ4 picker, its Choose source and How pack works icon, Pack job tags, the Library's LZ4 and zstd chips and badges |
| `--lz4-tint` / `-tint-hi` / `-line` | yellow 14% / 22%, ring 50% | Their tint, hover and ring (`--lz4-ink` on tint ≥ 9.3:1, `--lz4-soft` 11.4:1) |
| `--edit-ink` / `-tint` | `#a3c0ff` / blue 18% | Edit blue (the `.tag.blue` hue): an edit or extract job's `.tag.edit` |
| `--rate-good-*` / `--rate-ok-*` / `--rate-bad-*` | ink `#86e8b0` / `#ffd28f` / `#ffaaaa` on green 12% / amber 12% / red 13% | About's rating chips (`.chip.good` / `.ok` / `.bad`), every text ≥ 8.9:1 |
| `--done-ink` / `-tint` | `#6fe3a3` / green 14% | Done green: `.tag.green`, Edit's In place, the update toast |
| `--danger-ink` / `-tint` / `-tint-hi` / `-line` | `#ff9b9b` / red 15% / 24% / 35% | Danger red: `.tag.red`, `.danger` buttons, Edit's Rebuild, a blocking note |
| `--danger-wash` / `-ring` / `-text` / `-title` | red 8% / 25% / `#ffb4b4` / `#ffd6d6` | A job's `.error-box` (text 10.3:1, title 13.1:1 on it over `--inset`) and a finding that fails every target (`li.block`) |
| `--transfer-ink` / `-tint` | `var(--done-ink)` / `var(--done-tint)` | Transfer green (the Done green): a transfer job's `.tag.transfer` |
| `--bar-track` / `--bar-lo` / `--bar-hi` | `#262633` / `#232c8f` / `#a8bfff` | The progress bar's track; its fill runs `--bar-lo`, `--accent` at 70%, `--bar-hi` (the bright head) |
| `--library-ink-hi` / `--library-line` | `#c4f6aa` / green 50% | Library green: the chosen **All** chip, the selected Cards/List button and the view switch's Library option only |
| `--view-lib-tint`, `--view-files-ink` / `-tint` / `-line` | green 14%; `#c3d3ff` / blue 18% / ring 50% | The Storage view switch's selected option: Library (`--library-ink-hi` on its tint 11.9:1), File explorer (10.6:1; also the folder tree's current folder, a picker's chosen file and File Transfer's chosen device) |
| `--pk-*` | green 14%, cyan 12%; ring 42%; glow 16%; ink `#86f5b4` | A packed source's tile (`.tile.neon.packed`) |
| count badge (`.count`) | white + `--page` | Unfinished jobs in the Convert, Edit, Pack and Connect tabs: one look on every tab (19:1) |
| primary button | `#fff` + `--page` | The one main action per card (the update toast's is `--done-ink`) |

**Format hues.** `.fmt-*` sets `--h` (text), `--h-bg` (tint) and `--h-line` (ring); text on tint ≥ 4.5:1.

| Format | Class | Hue |
|---|---|---|
| Folder | `fmt-folder` | white `#ececf4` |
| exfat | `fmt-exfat` | magenta `#ffa3e6` |
| ffpkg | `fmt-ffpkg` | blue `#a3c0ff` |
| ffpfs | `fmt-ffpfs` | teal `#7ee0e0` |
| ffpfsc | `fmt-ffpfsc` | amber `#ffc580` |
| fpkg | `fmt-pkg` | green `#8fe0ab` (Convert's default target) |
| zstd packed folder | `fmt-zstd` | teal `#7fe3d8` (About's Compare formats row) |
| LZ4 packed folder | `fmt-lz4` | lime `#d6e875` (a job row's pill for an LZ4 Pack into a folder; the Pack target picker shows that target as Folder) |
| Archive (zip, 7z, rar) | `fmt-archive` | sand `#e8c9a6` (sources only; one hue for the three, the label tells them apart) |

**Semantic tags.** Tinted pills for states: `.tag.blue` a running job's stage (Writing), `.tag.green` Done
(also Connected and another device's name), `.tag.red` Failed, `.tag.orange` Backport and warnings (Offline,
Can't scan, Can't delete, Leftovers' "N found"), `.tag.violet` DLC and Additional content (a source without `eboot.bin`, beside
the other labels on every tab, Library cards and rows too). A plain `.tag` is neutral (Emulators,
Queued, Disconnected, a device's kind).

**Type.** System font (SF Pro on macOS), 14px/1.45 body.

| Size / weight | Use |
|---|---|
| 20/600 | Game title |
| 18/600 | Tile values (tabular numbers) |
| 18/500 | Card titles |
| 16/600 | Empty-state and modal titles |
| 14/600 | Library game names (cards and rows) |
| 13.5/600 | Job names |
| 13 | Controls and body copy |
| 12.5 | Labels, meta, notes |
| 12 mono | Paths |
| 34/600 mono | Pairing's check number (`.connect-code`) |
| 15 mono | Connect's IP tile (`.connect-ip`) |
| 11.5 | Hints, under-tile notes |

**Spacing.** Page gutters 28px (12px under 600px), content max 1200px, 16px between
cards, card padding 20–22px, 14px from card head to body, 12px between groups in a card, 8px between
tiles and tags. Two columns from 880px; Edit splits 1:2 from 1100px (its game card is the narrow one).

## Components (class names in `styles.css`)

- **Header** (`.topbar`, sticky, no bottom border): the 30px logo left (a transparent emblem by Xxcco: a blue ring with the four button symbols around an orange gear, no tile; its orange glow a `drop-shadow` that follows its shape), the screen tabs centred
  (`.seg.nav`: 14px labels, a 1% white fill over a 28px backdrop blur), the GitHub mark (`.gh-link`,
  32px round, `--muted`) rightmost; the app opens it in the default browser through the fixed-URL
  `open_repo` command. Right of the logo the Connect indicator (`.conn-btn`, every platform, a bare button
  around one `.tag`): grey **Disconnected**, or `.tag.green` **Connected (N devices)** (online peers + online
  saved NAS); it ellipsizes before it pushes the tabs, and a click opens the Connect tab. No wordmark and no version: the window or page title carries both ("PS5 Dump
  Forge v…" in the app, "Dump Forge v…" in the web build; `APP_NAME` in `api.ts`).
  Under 900px: two rows, logo and actions over the full-width tabs (`--top` 98px); the tab bar scrolls
  inside itself, keeping the chosen tab in view. At ≤ 420px the tab padding tightens (with six tabs the bar
  may scroll inside itself at 360px). A phone held landscape (under 900 × 500px) gets a static header
  that scrolls away with the page.
  PS5 only: **L2** / **R2** trigger badges (`button.shoulder`: 28 × 20px, 24px at ≤ 420px, top
  corners rounder like the triggers, `--raise` fill, `--line-hi` ring, 10.5px mono `--muted`, `--fg`
  on hover) on either side of the bar, outside its pill (`.seg-wrap`), out of the Tab order; a click
  steps to the previous / next tab, as the triggers do. With them (`.nav.shoulders`) the
  ≤ 420px tab padding is 5px, so the bar still fits at 360px.
- **Segmented control** (`.seg`, `.seg-opt`, `.on`): the screen tabs (Convert, Edit | Pack | Storage |
  Connect | About: one tablist, `.tab-sep` dividers drawn only, arrow keys move through every tab), the format
  picker (`.seg.formats`), Edit's mode choice, the Pack tab's zstd | LZ4, level and runtime pickers, the
  pickers' device tabs and the small pill switches (`ViewSwitch`: Storage's views, Connect's filter,
  File Transfer's devices). Native radios, `role="tab"` or `role="radio"` buttons sit underneath for
  the keyboard. Options are 29px, 13px/500 `--muted`; chosen `--raise-hi`, `--fg`.
  - Every screen tab is plain text, the Pack tab too.
  - The Storage view switch (`.seg.view-switch`, `Storage.tsx`): first in the view's card head
    (`.storage-head`; left, right in RTL), the view's actions after it; smaller than the screen tabs so it never reads
    as a second navbar (26px pills, 12.5px labels, 15px icons). **[disc] Library** | **[folder] File
    explorer** (selected at start), `role="radiogroup"` of two `role="radio"` (one tab stop; arrows, Home, End move and
    choose); selected tinted, ringed and bold in its own colour (Library green, explorer blue). Under
    400px the options show their icons only (the labels visually hidden, still their names), so a long label
    (fr, es, pt) never runs past a phone's card.
  - Format picker: the selected segment fills with its format tint and ring, bold; unselected segments
    are plain muted text. Segments are sized by their labels, then share the spare width. A refused
    format is dimmed (38%), its reason read out with its label and in its tooltip.
  - Smaller pickers of the same kind (`.seg.formats.inner`): the image inside an ffpfsc (shown only for
    that target) and the fpkg Compression picker (Fast / Balanced / Smallest / Stored, 15px bolt, scale,
    compress and file icons, the fpkg tint), each with one line under it on what the choice costs.
  - The ffpfsc Compression level is a native range (`.range`, `accent-color`, 0–9, default 6) with its
    value in the label row (`.range-value`, tabular) and a line under it on what the level costs.
- **Connect** (`Connect.tsx`, a device hub): two columns from 980px (`.connect-cols`: 340px, then the
  rest; one column below).
  - Left (`.connect-side`): **This device** (`.connect-me`: lock icon, a plain `.small` **How to
    connect** with the info icon at its head's end on every platform (the head never wraps), as Convert's
    Compare formats; name and platform `.tag`; in the web build the page's QR code, 220px and centred, no
    caption (`.connect-qr`, hidden under 600px; left out when the code doesn't load); the IP address
    (`.connect-ip`) as a one-column tile; no notes, by user request). In the desktop app only, then **Dump Forge on PS5** (`.connect-install`: power icon, a plain
    card, no pitch): the PS5 address input (prefilled with a saved PS5's IP)
    and the primary **Install on PS5**, its hint (the console's ELF loader on port 9021), the status line
    (spinner and `Dots`: finding the release, installing; then a green `.note-line.good` or the error),
    and "Any other ELF payload:" with a small **Send a payload** (a native file pick, any name: the ELF header is checked). How to connect
    opens with a blue box (`.install-lead.help-install`: bolt icon, a title and a short paragraph: type the address, press Install on PS5, the console downloads the matching release from GitHub, else the latest, and starts it), desktop only. Then
    **Add a device**: one address input (mono, `dir="ltr"`, an example placeholder) and
    **Connect** (primary once something is typed; under 420px the input takes its own row), no hint
    (by user request), then "Checking <address>" (spinner, `Dots`) while it probes, or the error line; then "Or
    add by protocol:" and three `.small` buttons SMB, FTP, NFS (their Add dialogs).
  - Right: **Devices** (grid icon). Head: Autodiscovery (`.small`, off while it searches) and File Transfer
    (`.small.primary`). Over the list (`.connect-tools`): the All / Forge / NAS filter with counts (the
    Storage view switch, `ViewSwitch` in `common.tsx`, chosen in the explorer's blue; only while both kinds
    are saved) and "N of M online" muted. The list (`.dev-list`, at most 9): online first, then Forge
    peers before NAS devices; a row (`.dev-row`, an inset tile, no border) is the `.online-dot`, the
    name (600) with its kind `.tag` (Forge; SMB, FTP, NFS), a muted facts line (platform, the address in
    mono `--fg-2`, a NAS's user or Guest, Online or "Offline · seen: 5 hours ago"), then **Send** (online
    only: File Transfer with the picker opening on that device) and a ⋯ icon button for the details dialog.
    Under 480px the actions wrap under the text. No saved device: the `.empty` panel ("No saved devices
    yet", its hint, a primary **Autodiscovery**) and the head keeps only File Transfer (secondary).
  - **Nearby** (`.nearby`, under the list or the empty panel): Autodiscovery's finds not saved here yet
    (Forge searches once at every start, the app and the PS5 alike, so they may be there before the tab is opened),
    as the same rows: a Forge with **Pair**, a NAS (its protocol tag) with **Save** (its Add dialog
    prefilled). While a search runs its title row shows a spinner, "Searching this network" with `Dots`
    and **Stop**; after a search this tab asked for with nothing new, "No new devices found". Hidden
    before any search that found nothing.
  Pairing (`.pair-modal`): Add a device's Connect probes the address: a NAS opens Add <protocol>
  (`.nas-modal`, `.nas-form`: each hint under its input describes it) prefilled, a Forge (and Nearby's
  Pair) sends the request, then the waiting dialog: a spinner and "Waiting for <address> to allow the
  pairing" with `Dots`, the check number in a `.tile.pair-check` (`.connect-code` digits, "123 456"), a
  muted line that it must match the other screen, and Cancel (closing cancels too); an outcome (denied,
  timed out, busy, already paired) replaces it with the error line and Close. One request at a time:
  another Pair or Connect sends nothing until that dialog closes; the typed address stays until the
  device pairs (a NAS hand-off clears it at once). The device asked
  shows the `PairPrompt` over any screen (`role="alertdialog"`): "Pair with <name>?", who asks (platform, IP)
  and what pairing grants, the same check-number tile, "Allow it only if <name> shows the same check number",
  **Deny** (focused first; Escape and a click outside deny) and **Allow** (primary); focus goes back where
  it was once no request is left.
  A control busy with its own action keeps focus: a button is `aria-disabled` (its action ignored), an
  input read-only. A failed poll keeps the last list (the error line under This device); before the
  first answer both cards show a spinner and "Loading" with `Dots`.
- **Card** (`.card`, no border: `--card` on `--page` plus a shadow; `.card-head` = 18px icon, 18px
  title, actions right; `.card.compact`). `.card.hero` adds the glow: two radial gradients from the top
  right corner, faded in once its colours are known (`.lit`); Edit's is smaller (`.compact-src`: 14px
  × 16px padding, 56px cover, 16px title). A card of plain prose (Edit's How this works, `.card.how`)
  fills its column and its text wraps across it: no line cap, no empty column.
- **Buttons.** Every button is a pill and may lead with an icon. Off (`disabled` or `aria-disabled`)
  is 42% opacity.

  | Kind | Class | Use |
  |---|---|---|
  | Primary | `.primary` | White. One per card (Build, Pack, Edit's Apply, Choose folder on an empty Library). |
  | Secondary | default | Dark. Choose, Change, Show in Finder (Explorer, folder), download links (`a.button`). |
  | Danger | `.danger` | Red tint. Stop job (a running job's Cancel, second click), Stop jobs and quit, the trash buttons, Yes, delete, Forget, Discard all, Replace. |
  | Small | `.small` | Job rows; `.small.icon-only` is a 30px round icon button. |
  | Pack | `.lz4-btn` | Yellow tint, ring and ink: the Pack tab's Choose source. |
  | Link | `.link` | Quiet text action (the crumbs). |

  Touch (`pointer: coarse`, a phone or tablet): every button, menu item, input, fold summary,
  switch row and explorer row is at least 40px tall, and fields take 16px text (iOS zooms the
  page into a smaller one on focus).
- **Update toast** (`.update-toast`, bottom corner, mirrored in RTL): the Done green (`.tag.green`), a 14% tint
  over `--card`, a 45% ring, `#6fe3a3` icon and title, its primary button green with dark ink. 360px,
  under dialogs; on a phone full width within the 12px gutters. A failed update shows the amber alert
  icon and the backend's reason.
- **Inputs**: pill, `--inset` fill, mono for paths. A generated output name shows whole in `.out-box`,
  its folder on the `.out-dir` line below; a file name is never clipped. Beside the Output label
  (`.label-group`) the device it goes to: a `.tag.green` with a paired device's or saved NAS's name, or a plain
  **This device** tag (only while there are saved devices and a folder is chosen). A source card's title
  row has the same green tag, with the device's or NAS's name, when the game isn't on this device. A typed name that can't work
  (a path separator, another image extension, longer than core's name limit, blank, a leading dot, a
  trailing dot or space, a control character) is refused in `.field-error` under the box, and Build stays off; so
  does a name an unfinished job is already writing. **Change** opens the picker at the output's
  folder.
- **Right-to-left** (Arabic, `<html dir="rtl">`): the layout mirrors through flexbox and logical
  properties (`margin-inline-*`, `padding-inline-*`, `text-align: start/end`); the few physical
  positions (switch knob, menus, the LZ4 badge, the select's chevron, the update toast, the progress
  bar's sheen) have `[dir="rtl"]` overrides, most in styles.css's Right-to-left section. Paths, file names, the output name, job names,
  the backend's English (`.backend`: errors and core's checks; job logs) and number + unit values
  (`.value`, sizes) stay left-to-right, isolated from the text around them (in Edit: the names, file
  sizes, rename input and param.json); the steps' "›", the progress bar's bright head, a closed
  fold's chevron, the explorer's open chevron and File Transfer's arrow mirror.
- **Language select** (`select.lang-select`, right in the About card's header after a muted
  "Language" label, `.head-lang`): the one dropdown. A native `<select>` drawn as a small pill
  (`button.small`'s 30px and 12.5px, `--inset`, `--line-hi` border, `--focus` ring), `appearance: none`
  with the `chevron` icon in `--muted` as its background, `--raise` on hover; options are System, then each language in its
  own words (`lang` set), so it holds any number of languages.
- **Switch** (`.switch`): a native checkbox drawn as a switch, blue when on. Disabled, only the track
  dims; the reason sits under it as a `.note-line.warn`. Remove backport (only for a source with
  backport libraries) and Full verification (off by default) each sit in their own inset box
  (`.field.backport`, `.field.verify`: `--inset`, `--line` border, 12px radius). Generate name based
  on content sits in the build row beside Build (off, it shows the name pattern in muted mono). A
  combination core would refuse disables Build with the reason in a `.note-line.block` under it
  (`.build-notes`). For an AMPR game,
  Convert says under Build what the conversion does by itself, each a muted `.note-line` with the
  `info` icon, from core's `runtime_action`: the bundled ampr_emu (its version) is installed (none
  there) or replaces the game's runtime (older or unknown), or the game's own one is kept (the bundled
  version or newer, named); a game's ampr-emu-ng is said only when it is replaced ("Replaces this game's
  ampr-emu-ng with ampr_emu <version>"), and a zstd packed game's only when it goes in; and a stale,
  damaged or missing `ampr_emu.index` is written afresh (for fpkg, the package always gets a fresh one). A
  game that doesn't use AMPR gets none.
- **Pack tab** (`Pack.tsx`): a Source card, then a "Pack" card whose head holds a plain `.small` **How pack
  works** (info icon, as Convert's Compare formats; `PackHelp.tsx`). The empty Source is `.empty.lz4`
  (yellow icon and glow): "Choose a game that uses AMPR", an experimental lead and one `.lz4-btn` **Choose
  source** with a Folder / Image/Archive menu.
  - The kind picker is a full-width `.seg.formats` in Pack yellow (`.lz4-y`: unselected `--lz4-soft`, chosen
    tint and ring, no icons): **zstd** (the default) | **LZ4**, with one line under it on what the job does
    (LZ4's an amber experimental lead). Before a source one muted line asks for one; a game the kind can't
    take (a patch package, additional content, a title that doesn't use AMPR) gets one `.note-line.warn`
    (`p.lz4-why`) instead of the settings. A game already packed by the kind chosen gets **Unpack**.
  - Then the Target picker (`.target-label`): **Pack** Folder (the default, the LZ4 packed folder shown as
    Folder) | exfat | ffpkg | ffpfs, the folder's line a green "Recommended" lead, an image's its
    format's line and a muted "Packs straight into the image" hint; zstd **Unpack** adds fpkg, LZ4 Unpack doesn't. Unpack
    shows one `.field.lz4` inset box saying what it writes back.
  - Pack's level: a `.seg.formats.inner` **Fast** / **Medium** / **Max** picker in the fpkg tint (`fmt-pkg`,
    bolt, scale and compress icons, as Convert's fpkg Compression), one line under it on what it costs
    (Fast green "Recommended"). LZ4 Pack adds the **Runtime** picker of the same look: **ampr-emu-ng**
    (default, green "Recommended") | **ampr_emu** with the bundled version. A Pack that installs ampr-emu-ng
    (zstd, or LZ4 with it) gets an amber line: turn off ShadowMount+'s fakelib update, or it replaces the
    runtime.
  - Pack and Unpack end like Convert (Full verification, Output, Generate name; shared in `target.tsx`),
    then the action button. Under it (`.build-notes`), muted `info` lines: for LZ4 with ampr_emu, what the job
    does to the runtime (core's `pack_runtime_action`; a game's ampr-emu-ng that it replaces is said so,
    one it keeps isn't), and a stale, damaged or missing `ampr_emu.index` LZ4 Pack writes afresh.
  - **How pack works** (`.modal.help`, `PackHelp.tsx`): the info icon in yellow (`.head-icon.lz4`), then
    zstd, LZ4 and ampr-emu-ng as short numbered lists; the body scrolls (focusable), title and Close stay.
- **Edit tab** (`Edit.tsx`, `EditChanges.tsx`, `EditApply.tsx`; `.cols.edit`, 1fr:2fr from 1100px). Before a
  source: the empty Source card left, "How this works" (`.how`: short paragraphs across the card's width, the last muted) right.
  - **Compact game card** (`SourceCard compact`, `.hero.compact-src`; smaller than the other tabs' by user
    decision, so the explorer gets the room): padding 14px 16px, the hero glow, a 56px cover (radius 10px),
    a 16px title, the title ID and version tiles (6px gap) (two per row at every width) and the findings; under it, folds (`.game-folds`)
    Game facts, param.json and Details, long contents in a scrolling `.fold-box`; then leftovers. Its head
    holds Delete (a trash `.small.danger.icon-only`; none for a NAS's or a peer's game). An
    interrupted edit (`<image>.forge-undo` beside it) adds a `.note-line.warn` (`.interrupted-note`).
  - **Explorer** (`.explorer` card), top to bottom: "Game files" with the file count as a `.tag` (an
    archive, a NAS's or a peer's game adds a note-line: browsed, not edited); the pending bar (below); Up and the
    crumbs (`.explorer-nav`, "Game" / …, the current folder `aria-current`); the toolbar
    (`.explorer-tools`, `.small`: Add files, Add folder, then **Extract folder**, the folder shown or at
    the root the whole game; no New folder button, by user decision; an archive gets Extract folder only;
    with changes pending a muted `.extract-note` says Extract copies the files as they are now; a failed
    Add a `.note-line.block`); the filter and a live count ("N folders, M files", "N matches", "first 2000 of N"). The well (`.explorer-well`: `--inset`,
    scrolls inside; from 880px the card is sticky and window-high, narrower the well stops at 60vh) is a
    `role="grid"` of rows (`.xrow`: icon, mono name that wraps anywhere, tabular muted size, actions): folders
    first with the `--accent-ink` icon, then files; a sticky header row. Rows take a roving `tabIndex`; the
    focused row tints, `:focus-visible` adds the ring. Row actions (`button.xbtn`: Rename (pencil), Extract
    (download; rows on disk only), Delete in danger hover, and a folder's open chevron) are quiet icon
    buttons with no fill until hover, in a fixed column (`.xgrid.editable` 126px; `.xgrid.extractable`, an
    archive: Extract and open, 62px; 82px with finger sized (40px) buttons) so sizes line up; under 600px or with
    finger sized buttons Edit's take their own line under the name, as the File explorer's do. Rename edits inline (`.xinput`, mono pill; the reason under it
    as a `.field-error`). Where dragged files would land rings in `--focus`: the folder row (`.xrow.drop`,
    blue tint) or the whole well (`.explorer-well.drop`), and the count line says "Drop to add to …".
  - **Badges** (`.tag.xmark`, 20px so a row keeps its height): **New** green, **Replaced** orange,
    **Renamed** blue with "Was <path>" as its tooltip. A folder's size is its entry count; one added from a
    listing cut short shows "—" with a tooltip (read on Apply).
  - **Pending bar** (`.pending-bar`, under the explorer's head while changes are pending; from 880px at most 45% of
    the card, scrolling, so the well keeps 160px): a blue-tinted inset (state, not an error) with
    "N changes not applied yet", **Undo**, **Discard all** (`.danger`) and **Apply** (`.small.primary`) in
    its head; a warn line when some changes won't apply; a "Changes, in order" fold (`ol.changes`, each
    change, its source path, its findings).
  - **Mode box** (`.edit-mode`, in the bar): one tag and a line: `.tag.blue` **Changed directly** (a
    folder), `.tag.green` **In place** ("needs N of its M free"), `.tag.red` **Rebuild needed** with the
    reason, or `.tag.red` **Can't apply** with core's findings (`.edit-why`, a scrolling `.backend` list).
    When both ways work, a `.seg.formats.inner` radiogroup (`.mode-hue`, by user decision): **In place** in
    the Done green (`.mode-green`), **Rebuild** in the Failed red (`.mode-red`); an unchosen option's text
    keeps its hue, the chosen one takes the tint and ring; one muted line on what the choice does. A muted "Checking" while an answer is due; a failed check a
    `.note-line.block` and **Check again**; changes that cancel out a muted "Nothing to change"; uploads
    still to send an info `.note-line`. While the job runs (or the source is read again) one info line
    replaces the box.
  - **Uploads** (web build, `.upload-line`): a full-width `<progress>`, "Uploading file i of n: sent of
    total" and **Cancel**. Apply's error sits in the bar as an `.error-box`.
  - **Dialogs** (`role="alertdialog"`, warn icon, Cancel or No focused): the conflict question ("N items
    already exist here", the folder, the names in a scrolling `.conflict-names` box, the replace warning;
    Cancel, Skip, Replace `.danger`), Discard all (above one change; No, Discard all), and Apply's
    confirm (In place: written straight into the path, no undo; Rebuild: a new image beside, put in its
    place; an interrupted edit is put back first; close the game first; Cancel and a primary Apply).
- **Lead word** (`Lead` in `target.tsx`): a note with a tone shows its first word in colour: `good`
  ("Recommended") in `--good` (`.lead-good`), `warn` ("Experimental") in `--warn` (`.lead-warn`). Text colour only
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
  `.tile.wide` spans the row (Edit's Game facts also add Content ID and the Forge version that made
  the image, both mono; the `LZ4 (AMPR)` tile: Packed, Damaged packs or Plain (with the runtime); its note
  says when the game doesn't use AMPR and adds what a conversion does to the runtime (installs or replaces with the bundled ampr_emu, or keeps
  another build's named version) and a stale, damaged or missing `ampr_emu.index`; an index to refresh
  or a runtime to install also makes the tile show; a zstd packed source shows a `zstd` tile in its
  place, Packed with the runtime). A packed source's tile (LZ4 or zstd) is `.tile.neon.packed` wherever it
  shows: static green and cyan straps over `--inset`, a `--pk-line` ring with a faint `--pk-glow`, the
  state in `--pk-ink` (`--pk-ink` 10.4:1, `--fg` 12.5:1, `--muted` 5.0:1 on the brightest strap); damaged
  packs (`.tile.damaged`) get no straps and the state in `--warn`.
- **Alert bar** (`.alert-bar`): one line of notice on a blue-to-orange gradient kept dark under the
  text, orange only as a glow at the right edge. `.alert-bar.warn` is amber: the fpkg firmware limit
  (`PkgWarning`), today the bar's only use.
- **Findings** (`ul.findings`): `li` warns (amber), `li.block` fails every target (red), `li.info` is
  good news (blue). Under Build, `.note-line` (`.warn`, `.block`, `.good`, `.muted` with an info icon)
  says why it is off or what to expect.
- **Folds** (`details.fold`): DLC, backport files, emulators in fakelib, job logs, verified checks,
  folders checked, Edit's Game facts / param.json / Details and "Changes, in order", the Library's
  skipped folders. Collapsed by default; a list inside that can grow is a scrollable, focusable box.
- **Job list** (`JobList.tsx`, `.card.jobs`): under Convert, Edit, Pack and Connect (`.screen.jobs-screen`;
  none on Storage and About) each tab's own jobs, the type its count counts (Connect: transfers), one list
  per tab kept mounted by `App`, only while that type has jobs, newest first; the head says "N running
  or queued" or "all finished"; min-height 240px, it grows
  with its rows (the page scrolls, never a box inside it). A row (`.job <type>`) is a tile (12px radius, 10px
  apart) in the app's black `--inset`, no edge ring (every type alike, by user decision; `--muted` 6.9:1, `--fg-2`
  12.6:1 on it; its fold summaries, checks list and path box on `--card`, the log's lines straight on the tile, no box): format tag, name,
  `from` path, progress bar (`progress` in a `.bar` wrapper, blue gradient; it never goes back and stays
  under 100% until the job succeeds; a sheen while the job runs, see Motion), status tag (no dots),
  meta line (% · speed · time left, the last two only in a pass that streams the game's bytes: compress, write, measure, pack, verify; then "0:42 in this step" once a stage has run 10 s; m:ss, h:mm:ss
  past an hour; the timer alone for a stage without a total), and the step row (`ol.steps`, under the
  bar; under the status line while the job waits without one): every stage core said the job will run,
  in order, short labels (`jobs.steps`), neighbours with one label merged (preflight and check are one
  Checking), "›" between them (mirrored in RTL), wrapping, 12.5px. Done steps: a small green check and
  `--fg-2`; the current one: `--accent-ink`, 600, `aria-current="step"`; later ones `--muted`. Screen
  readers hear "done" / "not started yet" (visually hidden). No row until the stages are known, none once
  the job has ended; no motion of its own. A Pack tab job (`.job.pack`) has a `.tag.lz4` in Pack yellow
  (zstd or LZ4 pack with its level, or unpack). An edit or extract job
  (`.job.edit`) has a `.tag.edit` **Edit**, **Edit · rebuild** (error box led by
  "Can't apply the changes:") or **Extract** ("Can't extract:"); a transfer (`.job.transfer`) a
  `.tag.transfer` **Transfer**, no format pill, "from" its first source, "Can't transfer:".
  A `flink://` path shows as "<device name>: /<path>" (`usePeerPath`), and so does every `flink://` in
  the backend's text: errors (`errorText`), a job's error, checks and log (`readable`, `peers.ts`).
  Running: **Cancel**, whose first click turns it into a danger "Stop job?". Finished: **Dismiss**, Show
  in Finder (Explorer on Windows, folder on Linux; none in the app for an output on a peer or a NAS),
  the error box (always led by its title, then core's text; a cancelled edit's "cancelled; …" is a muted
  `.note-line` instead), or the size, file count and output path, then a `details.fold.verified`
  "Fast (Full) verification passed: N checks" (the checks inside, a fast one's coverage first).
  A paired peer's job that uses this device's files (Connect's `remote_jobs`) is listed and counted in
  its tab too, below this device's (`.job.remote`): greyed (80% and grey, all but its Cancel; `--muted`
  stays 4.8:1), a `.tag.green` with the device's name, only Cancel while it runs (the same "Stop job?",
  sent to that peer), no buttons once finished; never in the quit prompt.
- **Rating chip** (`.chip.good` / `.ok` / `.bad`): the About tab's Compare formats rows (Console speed
  and Size on disk). Tint, icon and text together, every text ≥ 8.9:1.
- **About tab** (`Formats.tsx`), three cards. **About** (full width; the head: info icon, title, the
  language select at its end; no introduction line): the nine features as one grid of small inset tiles
  (`.feat-tiles` / `.feat-tile`: `--inset`, `--line` border; a round icon, the bold name, its text), no
  group headings, in three sets of three (Convert, Transfer, Edit / Streamed, Archives, USB and SD cards /
  Pack, Storage, Safe). From 1000px each set is a column, the three sharing the card's width at equal
  heights; 600–999px each set a row of three; a phone one tile a row; a slightly wider gap between sets
  is the only grouping cue. **Compare formats** (`.fmt-table`): one row per format (`.fmt-row`), an
  inset tile edged at its start in the format's hue (`.fmt-*` `--h-line`): the format and its tagline,
  then the two rating chips; from 760px a muted heading row (`.fmt-head`: Format / Console speed / Size
  on disk), below it the row stacks, the heading row is only read (visually hidden) and each chip carries
  its own small muted label (`.fmt-q`, skipped by screen readers). **Credits** last, a
  `.card.compact` of its own (plain selectable text, no links).
- **Empty state** (`.empty`): an inset well with a round blue icon (`.empty-icon`; `.bad` red for an
  error), a 16px title, one line of help, the accepted kinds as tags, and its buttons.
  - **Source pickers** (`PickButtons` in `common.tsx`): **Folder** (primary, folder icon) and
    **Image/Archive** (secondary, archive icon; one file dialog for images, packages and archives,
    `.001`/`.z01` first parts included); no NAS button (removed by user request: a saved NAS online is a
    tab of both pickers). Once a source is chosen the same buttons sit
    in the Source card's head as `.small` buttons.
- **Modal** (`.modal-backdrop`, `.modal`): a 440px card on a blurred dark backdrop. `Modal` +
  `openOverlay` (`common.tsx`) render it in its own root with the whole app behind it `inert`: Escape
  and a click outside cancel, Tab stays inside, focus returns to the opener, a quit request closes it
  first. A dialog opened from another (a Picker over File Transfer, a device's details → File
  Transfer) makes that one inert behind it and gives focus back to it. The pairing prompt
  (`.pair-layer`, Connect) is the one layer above every dialog: one opening, loading or closing under
  it leaves focus in the prompt, and a quit request unmounts it instead of answering it.
  - **How pack works** (`.modal.help`, 560px, `PackHelp.tsx`): above, under the Pack tab.
  - **How to connect** (`.modal.help`, `ConnectHelp.tsx`): the same frame, a few short paragraphs and
    no section headings (pairing and the check number, a NAS, File Transfer, phones in the web build
    only, what the network can see); Close.
  - **File Transfer** (`Transfer.tsx`, `.modal.transfer-modal`, 820px): the intro line, then **What to
    send** → **Send to** side by side from 860px (`.transfer-ends`; a muted arrow between, mirrored in RTL),
    stacked below it with the arrow pointing down. Each side (`.transfer-end`, an inset panel): its label,
    while another device is online a wrapping device switch (`ViewSwitch` `.devices`: This device, then
    each online paired Forge and saved NAS by name, cut short with an ellipsis; chosen in the explorer's
    blue) that sets where its picker opens and follows the chosen paths' device (none shown, and Start off, while
    that device isn't online; switching device drops the old pick), the paths chosen in an
    `.out-box` (mono `.transfer-path` rows, LTR, scrolls past 120px; "Nothing chosen yet" muted), and its
    buttons (Choose files / Choose a folder; Send to: Choose a folder, off with `connect.sendNeedsDevice`
    while no other device is online). The body scrolls; the footer (`.transfer-foot`) stays (under 600px
    wide or 600px tall the whole dialog grows and the backdrop scrolls it instead): in the web
    build this browser's Upload to this device / Download from this device with its hint (disabled, with
    a note, in the PS5's own browser; an upload's progress line with Cancel in its place), then Close
    (`aria-disabled` while an upload runs) and a primary Start.
  - **Password** (`.modal.password`, `password.tsx`): opens when inspecting an archive answers "needs
    a password". The lock icon and "This archive needs a password", the path in an `.out-box`, one line
    (muted; `.bad` "The password is not correct." after a failed try, `aria-live`), a full-width
    `type="password"` input (`.pw-input`, focused, selected again after a wrong try), **Cancel** and a
    primary **Unlock** ("Checking…" while it runs). Cancel leaves the source locked: the Source card
    shows the red reason and a `.small` **Enter password** (lock icon) in `.row.locked`. The password
    stays in memory with that source (inspect, naming and its jobs), never stored or shown.
  - **Delete** (`confirmDelete` in `delete.tsx`, `role="alertdialog"`, shared by Edit, the Library and
    the File explorer): the trash icon in the warn amber (`.head-icon.warn`), "Delete this folder?" / "Delete this image?" /
    "Delete this archive?" (the explorer: also "Delete this file?"), the whole path in an
    `.out-box`, a muted line (for good, no Trash or undo; a folder: some of it may be gone if deleting
    stops part way; an archive: its other parts stay), and two buttons: **No** (focused first) and
    **Yes, delete** (`.danger`). Afterwards a muted "Deleted <path>".

- **Storage tab** (`Storage.tsx`, `.storage`): one card, the Library's or the File explorer's
  (`.storage-card`; both kept mounted; on a computer one chosen folder for both). Its head
  (`.storage-head`, wraps): the view switch, then the view's actions at the end (`.storage-actions`).
- **Library** (`Library.tsx`, `.library`): head actions **Refresh**, **Choose folder** (a computer with a
  folder), then the Cards / List buttons (`.library-views`, icon-only, after a divider, `aria-pressed`,
  selected `--library-ink-hi` on `--raise-hi` with a `--library-line` ring). On the PS5 the
  internal storage's free space leads the head's actions, before Refresh (`FreeSpace`, `.library-storage`:
  drive icon, "Free **312 GB** of 825 GB" in decimal GB, `--muted` 6.5:1 with the free amount in `--fg-2`;
  the File explorer's head shows the same). Under the head, on a computer only (`.library-meta`, muted
  12.5px), "**12** games in" + the chosen folder's `PathLine`; the PS5 has no count line (the chips
  count, by user request). Then the
  kind chips (`.kind-chips`, `role="group"` "Show kind"; 28px pills (40px on touch) with a count, quiet until chosen,
  `aria-pressed`): **All**, then only the kinds present (Folder, ffpkg, exfat, ffpfs, ffpfsc, fpkg, an
  archive), **LZ4** and **zstd** (packed) and **Additional content**; chosen, a kind takes its `.fmt-*` hue, LZ4 and zstd yellow,
  Additional content `--raise-hi`, All the Library green. A chosen kind that is gone shows All.
  Then the scrolling well (`.library-well`: `--inset`, 14px padding, height to the viewport; at least
  160px and, in Cards, one whole card row plus 56px of the second when there is one (`--fit`, measured,
  since a card's height follows its column's width), so a short window scrolls the page; focusable, `role="region"`, `aria-label` "Games"); the list scrolls inside it, never the page. Under
  900px the well grows with its list and the page scrolls instead (no box to scroll on a phone).
  - **Cards** (`.library-cards`, `minmax(240px, 1fr)` columns, 16px gaps): each card (`--card`, 14px
    radius, no glow, no outline) has a square cover (10px radius, a `padding-top: 100%` box for older
    WebKit), the name (600, wraps anywhere), the title ID and version muted, the path line (copies
    of one game differ only there), and a foot with the format pill (and a violet Additional content tag)
    and the actions. **List** (`.library-rows`): compact grid rows
    (`.library-row`, 8px 12px padding): 48px cover, name over the title ID, version and LZ4 / zstd / Additional
    content tags (`.library-sub`), the format pill (104px column), path line, actions; under 720px three
    rows (cover beside the name, then the pill and actions, then the path whole). Covers load lazily, one at a time; a
    placeholder with the disc icon stands in at the same size.
  - **Pack badge** (`.lz4-badge`, an entry with LZ4 or zstd packs): "LZ4" or "zstd" in bold `--lz4-ink` on a
    liquid-glass pill: a faint white gradient, a light rim brighter on top, a soft dark text shadow,
    no colour of its own. On a card it sits on the cover's top-right corner (24px) over a light dark
    tint with a backdrop `blur(14px) saturate(160%) brightness(0.55)` (`-webkit-` prefixed for the PS5
    browser), so it stays readable on any cover; in a row it follows the title ID (20px, ≥ 7:1 on
    `--card`). It reads "LZ4 asset packs" or "zstd asset packs" (its tooltip too) to a screen reader; the format pill stays the entry's own.
  - **Actions**: icon-only buttons with `aria-label` and `title` ("Edit <name>" with the pencil icon,
    "Convert <name>", "Delete <name>", the last `.danger`). Edit and Convert switch to that tab with the game loaded;
    Delete opens the shared Delete dialog; while the delete runs that entry's Edit and Convert and
    every other trash button are off, its own trash button reads "Deleting <name>" (`aria-disabled`,
    so it keeps focus); then focus moves to the next entry (or Refresh) and a muted "Deleted <path>"
    sits above the list; a failure shows an orange "Can’t delete:" tag + the reason (`.backend`). At the 240px card minimum the foot wraps: the
    actions take their own line, at the end.
  - **States**: scanning, a centred spinner and "Scanning <folder name>" with `Dots` in the well (`role="status"`,
    no `aria-busy`, so it is announced). Before the platform is known, a spinner and "Loading" with `Dots`. A computer with no folder yet: a centred `.empty` panel with the folder icon, **Choose a
    folder with your library to scan** and a primary **Choose folder**. A chosen folder refused or
    failed: the same panel with the red icon, **Can’t scan this folder**, core's message and **Choose
    another folder**. A failed Refresh keeps the list under an orange `.tag` "Can’t scan the
    Library:" + reason. A hit scan limit: an orange tag "Scan limit reached. This list may be incomplete." and the limits
    in muted text. Anything skipped: a "Skipped locations and files" fold under the list. No games (or, on the PS5, no roots): the `.empty` panel with the folder
    icon and the reason, plus **Choose another folder** on a computer. Refresh and Choose folder go
    `aria-disabled` while a scan runs (they keep focus); a panel button the spinner replaces hands focus to the well.
- **File explorer** (`Files.tsx`, `.explorer.files.storage-card`, `aria-label` "File explorer"): Edit's
  explorer (Up and crumbs, the filter and count, the `xgrid` well) without a pending bar and without
  Edit's note; head actions: on a computer the root's `PathLine`, then **Refresh**, then on a computer **Choose
  folder**. From 880px two
  panes (`.files-panes.tree`: 220px + the list, 14px gap, each scrolling): the folder tree (`nav.files-tree`,
  "Folders" 12.5px, on `--inset`, 12px mono rows, 10px a level) shows the chain from the root to the folder shown, each folder
  on it with its subfolders (`list_dir`, fetched lazily root down, cached while the view shows; listed
  again after Refresh, a move, a delete heard or the view hidden, each old list shown until its answer
  replaces it; forgotten for a new root); the current one `aria-current`, the explorer's blue
  tint and ring (hover leaves it so), the tree's one tab stop (the arrows, Home and End move between its folders); a click
  opens any and keeps focus in the tree; the crumbs hide (Up stays). A switch of view moves focus that was on the view switch to the
  shown view's copy. Below 880px no tree. Row actions, icon-only (`aria-label` + `title`): Rename (pencil; inline name input, F2),
  Move to (the `move` icon; the folder picker), Delete (trash, `.danger`), then for a folder or an
  image/archive a ⋯ (`dots`) `ChooseMenu` with Edit and Convert ("More actions for <name>"; it opens
  upwards, or from its other edge, where the well would clip it: `.menu.up`, `.menu.start`), then a
  folder's open chevron; 158px action column, wrapping under the name below 600px or on touch. A link
  row: a plain 20px `.tag.xmark` "Link", open only (its tip, why, also in visually hidden text). The grid is named after the folder shown.
  A folder past 10,000 entries: a warn `.note-line` (only the first 10,000 listed); an empty folder or no
  filter match: a muted line in the well.
  A folder's listing keeps the grid (head row, `aria-busy`) and shows the spinner only after 150ms;
  focus that was on a row waits on the grid. **Refresh** is `aria-disabled` + `aria-busy` with `Dots`
  until its answer. The trash tip on a Mac names fn+Delete (its delete key is Backspace: Up). An image ShadowMount+ has mounted (the PS5): a plain `.tag.xmark`
  "Mounted" (its tip: stop the game and turn ShadowMount+ off to delete, rename or move it), no Rename,
  Move or Delete (its ⋯ keeps the 4th place); Edit and Convert stay; a folder holding one shows a note
  above the list (info icon) saying why. While an action runs, the row it changes keeps only its own
  button, `aria-disabled` (the trash reads "Deleting <name>"), its ⋯ and chevron off and it can't be
  opened; every other row's Rename, Move and Delete are off. After an action a muted line ("Renamed to <path>", ...; not when another folder shows by then, and focus moves to a row only if it is still in the list) or an
  alert line "Can’t rename:" / "Can’t move:" / "Can’t delete:" + the reason. Root on a computer: the
  chosen folder, else the home folder (the app) or the server's roots (the http build); with neither
  (no home, or that start can't be listed: its alert line first) the Library's empty panel (**Choose
  folder**). On the PS5 the root lists the browse roots ("Drives"), nothing to change there. A card with
  no listing (that panel, an unknown platform's alert + **Refresh**) is `.bare`: it hugs its content
  instead of filling the window.

### Web build only (`npm run build:http`, the PS5 payload's UI)

The web build names the product "Dump Forge" (the PS5 tile's name); the desktop app "PS5 Dump Forge".

- **Launcher** (`main-http.tsx`, before the app while the tile's page starts the payload): a modal
  card with the spinner and "Starting Dump Forge…" (`role="status"`); on failure (`role="alert"`) the
  warn icon, "Can't start Dump Forge" (or "Dump Forge didn't answer"), the reason muted, a primary
  Retry.
- **Header controls** (`.top-actions`): "Stop Dump Forge" (`.small`, 14px, power icon) before the
  GitHub mark; under 600px and at 900–1049px (where its label beside the longer languages' tabs
  would squeeze the logo and Connect's state away) it shrinks to its icon (the label stays as visually
  hidden text, and a `title` tip); the orange `.tag` "Offline, retrying…" sits before it (announced by an always-mounted, visually hidden
  `role="status"`). A debug build (never released) adds an English "Input probe" (`.small`) before Stop.
  With no job running it stops at once; with jobs it opens the quit modal listing them (Keep
  running / Stop Dump Forge); if they all finish while it is open, it asks "Stop Dump Forge?" and
  says how it starts again (the PS5: its tile; a computer's `serve`: run it again). Then "has
  stopped" with the same. Once Stop is confirmed the dialog takes no keys or clicks:
  Escape stays the browser's, so the PS5's Circle leaves the page. A failed stop turns it into
  "Couldn't stop Dump Forge" with the reason, Close and Try again (`.danger`).
- **Folder and file browser** (`.modal.picker`, 560px): breadcrumbs (`.crumbs`, mono 12px: "Drives",
  then each folder as a `.link`, the current one plain with `aria-current`; long paths wrap, never
  clipped, and scroll inside the crumbs once they would push the buttons off screen), then an
  inset list (`ul.rows.pick-list`, scrolls inside, max 420px; at most 2,000 rows, one Tab stop, the arrows move): Parent folder (or All
  drives), folders, files with their size (tabular, muted). File mode lists only the accepted
  extensions (Edit's Add file pickers: every file; Add files toggles a selection within the folder
  shown: the row tinted and ringed, its icon a check, `aria-pressed`, then a primary "Choose N files");
  Footer: Cancel and,
  in folder modes, "Choose this folder" (`.primary`; off at a NAS's shares or exports, an FTP
  server's root excepted). The next browse opens where the last one ended. A failed listing shows
  `.bad.pick-error` (`role="alert"`) with the reason, and its last crumb stays a button that tries
  again. Without device tabs each saved NAS is a root after the drives (drive icon, its name muted).
  The desktop app uses this modal too whenever device tabs show (its local roots: the home folder and
  the volume folders); it has no NAS source button (removed by user request: a NAS is browsed on its
  tab while it is online).
  While a paired peer or saved NAS is online the same modal, in both builds, leads with device tabs
  (`.seg.pick-tabs`: the view switch's size, wraps; `TabList`, arrow keys): This device, then each online
  device; each tab's list in a `.pick-panel`. A remote tab starts at its root (the device name as the
  crumb). The desktop's local tab adds **System dialog** (`.pick-native`, left in the footer).
- **Show path** (instead of Show in Finder on a finished job): the output path in an `.out-box`
  (`.path-shown`), focused and selected, to copy by hand (plain HTTP has no clipboard API).

Icons come from `icons.tsx`: 24-unit grid, stroke 1.8, `currentColor`, shown at 18px (15px in small
spots). Add a path there rather than importing an icon set.

## Cover-derived glow (`glow.ts`)

- The cover (a `data:` URL) is drawn to a 24×24 canvas; pixels are bucketed by hue into 12 bins of
  30°, skipping near-greys, near-blacks and near-whites.
- The two most weighted saturated hues (the second at least 60° from the first and at least 15% of
  its weight, else the first again; nothing saturated: the average colour, unless that is grey) become
  `--glow-1` and `--glow-2`, dimmed until the card's brightest point keeps muted text ≥ 4.9:1
  (relative luminance ≤ 0.028). The last 16 covers are cached.
- The layer (`.card.hero::before`) fades in (`.lit`) once its colours are known; while a source is
  read it fades out in the last colours, so a cover's colours never follow a flash of the brand ones.
- No cover, or any failure: the brand glow (blue `rgba(61,107,255,.13)` + orange `rgba(255,122,47,.08)`).
- Old WebKit (the PS5's browser): the cover loads through `onload` (no `decode()`, 5 s timeout), and
  the `::before` layer uses four offsets (not `inset`) inside a `z-index: 0` stacking context.

## Motion

Only short fades and transforms: the hero glow (0.5s), the switch and fold chevrons (0.15s), the
spinners, the dots after a waiting text (`Dots`, `.dots`: ".", "..", "..." in turn over 1.2s, each dot
in its place; e.g. "Reading the source", the Library's "Scanning", the pickers' and explorers'
"Loading", Connect's searching, probing and pairing waits, the Pack action button while it starts;
a static "..." with reduced motion), and a job bar's sheen (`.bar
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
- Button and menu labels name the action plainly, no trailing "…"; only a state in progress ends in
  one ("Checking…", "Stopping Dump Forge…", "Offline, retrying…") or takes the animated `Dots`.
- Say "game", never "dump" (the product names excepted).
- Every UI copy change updates all eleven files in `app/src/locales/` (`en.tsx` is the source).
