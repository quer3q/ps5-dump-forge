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

- **Header** (`.topbar`): the 30px logo on the left, the screen tabs centred as a segmented control.
  No wordmark, because the window title already says "PS5 Dump Forge".
  The version is in the page title ("PS5 Dump Forge v…", shown by the PS5 browser's title bar), not in the header.
- **Segmented control** (`.seg`, `.seg-opt`, `.on`): used for the screen tabs, the Inspect sub-tabs and
  the format picker (`.seg.formats`).
  - In the format picker the selected segment fills with its format tint.
  - Unselected segments are plain muted text, without dots.
  - Native radios or `role="tab"` sit underneath, so the keyboard works.
  - Segments are sized by their labels, then share the spare width; six formats fit half of a 900px window.
  - The image inside a `.ffpfsc` is a smaller picker of the same kind (`.seg.formats.inner`), shown only
    for that target.
- **Card** (`.card`, `.card-head` = 18px icon + 15px title + actions on the right, `.card.compact`).
  The hero variant is `.card.hero`; its glow fades in once the colours are known (`.lit`).
- **Buttons.**

  | Kind | Class | Use |
  |---|---|---|
  | Primary | `.primary` | White. One per card (Build). |
  | Secondary | default | Dark. Choose folder…, Change…, Show in Finder (Explorer, folder). |
  | Danger | `.danger` | Red tint. "Stop job?", "Cancel jobs and quit". |
  | Small | `.small` | Job rows. |
  | Link | `.link` | Quiet text action ("Compare formats"). |

  Every button is a pill and may lead with an icon.
- **Inputs**: pill, `--inset` fill, mono font for paths. A generated output name shows whole in
  `.out-box`, with its folder on the `.out-dir` line below it. Never clip a file name.
- **Switch** (`.switch`): a native checkbox drawn as a switch, blue when on. A disabled switch dims
  its track only; the reason sits under it as a `.note-line.warn` ("Remove backport", shown under the
  format picker only for a source with backport libraries, is disabled when core refuses removal).
  "Full verification" (off by default) sits below it; for `.fpkg` it shows on and disabled, with a
  `.note-line.muted` saying packages are always fully verified.
- **Tags** (`.tag`, `.tag.fmt`, semantic colours): 22px pills, 12px/500.
- **Stat tiles** (`.tiles`, `.tile`, `.value`, `.note`): two per row (four per row at medium widths);
  `.tile.wide` spans the row.
- **Alert bar** (`.alert-bar`, `.alert-bar.warn`): one line of notice.
  - The blue-to-orange gradient is kept dark under the text.
  - Orange is only a glow at the right edge.
  - The warn variant is amber and is used for the `.fpkg` requirements and build time.
- **Findings** (`ul.findings`): `li` warns (amber), `li.block` fails every target (red), `li.info` is
  good news (blue).
- **Folds** (`details.fold`): the DLC list, backport files, emulators in fakelib, job logs, verified
  checks and folders checked. They are collapsed by default; a list inside one that can grow is a scrollable, focusable box.
- **Job row** (`.job`): format tag, name, `from` path, progress bar (`progress`, blue gradient), status
  tag, meta line (`%` · speed · time left). Once finished, the row shows Show in Finder (Show in Explorer
  on Windows, Show in folder on Linux), the error box or "Fast (Full) verification passed: N checks"; a
  fast one lists its coverage first among the checks.
- **Rating chip** (`.chip.good` / `.ok` / `.bad`): used in the About formats table. Tint, icon and text
  together, every text ≥ 8.9:1.
- **Empty state** (`.empty`): an inset well with a round blue icon, a title, one line of help, the
  accepted kinds as tags, and two buttons.
- **Modal** (`.modal-backdrop`, `.modal`): 440px card on a blurred dark backdrop; the screens behind it
  are `inert`.

### Web build only (`npm run build:http`, the PS5 payload's UI)

- **Launcher** (`main-http.tsx`, shown before the app while the tile's page starts the payload): a
  modal card on the backdrop, the spinner and "Starting PS5 Dump Forge…" (`role="status"`). On failure
  (`role="alert"`): the warn head icon, "Can't start PS5 Dump Forge" (or "PS5 Dump Forge didn't
  answer"), the reason as muted text, and a primary Retry.
- **Header controls** (`.top-actions`, the header's right column): "Stop PS5 Dump Forge" (`.small`
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
    The file mode lists only the accepted extensions.
  - Footer: Cancel and, in folder mode, "Choose this folder" (`.primary`). Esc cancels, focus
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
  formats tab (`Formats.tsx`).
- The debug package is labelled `.fpkg` everywhere it is shown. Files are still saved as `.pkg`, and
  the internal id and the CLI flag stay `pkg`.
- Errors lead with a sentence a user understands ("Can't build this game as .exfat:") and then quote
  the core's lines unchanged.
