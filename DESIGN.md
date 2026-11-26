# PS5 Dump Forge UI design system

The rules behind `app/src/styles.css` and `app/src/queue.css`: the CSS holds the exact values, this
file says when to use what. The look follows kraken-mount's UI (`kraken-mount/src/ui.html`): its
three colour schemes in a dark and a light theme, filled surfaces with no borders, a side rail for the
screens, pill chips and quiet filled controls. The app icon (a blue ring around an orange gear) is the
one brand mark.

## Principles

- **Two themes, three schemes.** `<html data-theme="dark|light" data-scheme="abyss|ember|moss">`, set by
  `prefs.tsx` from Settings (dark + cobalt by default). Every component rule reads forge's tokens, which
  point at the scheme's palette, so no rule names a theme except where a hue needs a darker ink in light.
- **No borders.** Surfaces are told apart by fill alone (page, card, inset, raise). The only lines are
  1px inset dividers between rows of one list (below).
- **Selected = fill + weight.** A chosen tab, chip, segment, row or Cards/List button takes a soft fill
  (the accent's, or its hue's tint) and 600 weight. No ring, no edge, no underline.
- **One control family.** Buttons and inputs share one height, radius and fill; chips and segment options
  are one 32px pill.
- **One hero.** Only the game card (`.card.hero`) glows, in colours taken from its cover (`glow.ts`).
- **Colour never works alone.** A format has its hue and its label; a rating its tint, an icon
  (check, dash, cross) and text; a selected option its fill and its weight.
- **Contrast.** Text is at least 4.5:1 on its ground in both themes. The light theme was measured for this
  file (Light-theme contrast, below); its findings are listed, not fixed here.
- **Fits the window.** Default 1200x820, minimum 900x620 (`main.rs`); the web build down to 320px wide.
  No horizontal page scroll; long lists scroll inside their own box, except the Queue, which grows with
  the page.
- **Offline and CSP-safe.** System fonts, inline SVG icons, the logo bundled by Vite; no remote assets,
  no injected `<style>`. The only inline styles are CSS variables set through React's `style`.
- **Old WebKit.** The PS5's browser has no `oklch()` and no `color-mix()`: colours are hex or `rgba()`.

## Tokens

### Palette (kraken-mount's values)

Each `html[data-theme][data-scheme]` block sets the palette (nine schemes, each dark and light); `:root`
repeats dark + cobalt, the default scheme (`prefs.tsx` `DEFAULTS`, also its fallback for an unknown value),
until `prefs.tsx` sets the attributes. Two kraken names are renamed because forge already uses them:
kraken's `--accent-ink` (text on a solid accent) is `--on-accent`, its `--danger-ink` is `--on-danger`.

| Token | Use |
|---|---|
| `--bg` | Window ground (`--page`) and every inset well (`--inset`) |
| `--surface` | Cards and the rail (`--card`, `--rail`) |
| `--surface-2` | Controls, tags, hovers (`--raise`) |
| `--line` | Inset dividers; a control's hover fill (`--raise-hi`) |
| `--ink` / `--ink-2` / `--muted` | Text / secondary text / labels and meta |
| `--accent` / `--on-accent` / `--accent-soft` / `--accent-soft-hi` | The scheme's colour / text on a solid accent / the selected fill / `button.primary`'s hover fill |
| `--line-strong` | Light only: an off switch's track and an empty field's ring, each >= 3:1 on the card |
| `--focus` | The 2px focus ring |
| `--danger` / `--on-danger` / `--danger-bg` | Danger text, text on solid danger, danger fill |
| `--warn` / `--warn-bg`, `--ok` / `--ok-bg` | Warning and success text, their fills |
| `--shadow` | Floating layers only: menus, modals, the toast, the phone's bottom bar |

| Scheme | Theme | `--bg` | `--surface` | `--surface-2` | `--line` | `--ink` | `--muted` | `--accent` | `--accent-soft` |
|---|---|---|---|---|---|---|---|---|---|
| abyss | dark | `#091214` | `#101a1d` | `#182427` | `#2b383b` | `#e5edee` | `#929ea1` | `#4dbcd4` | `#093139` |
| abyss | light | `#eff7f8` | `#f9fcfd` | `#e6eff1` | `#cad7da` | `#101a1d` | `#505e61` | `#00687e` | `#cbebf3` |
| ember | dark | `#160f0b` | `#1f1612` | `#291f1b` | `#3e332e` | `#f0eae7` | `#a39994` | `#f2966c` | `#3e2418` |
| ember | light | `#faf4f1` | `#fefbfa` | `#f3ebe7` | `#ddd2cd` | `#1f1612` | `#645853` | `#a44c1d` | `#faded2` |
| moss | dark | `#0c120d` | `#131a14` | `#1c241d` | `#2f3831` | `#e8ede8` | `#969e97` | `#76be86` | `#1a321f` |
| moss | light | `#f1f7f2` | `#fafdfa` | `#e8efe9` | `#ced7cf` | `#131a14` | `#545e56` | `#206b38` | `#d5ecd9` |
| cobalt | dark | `#0a0f1a` | `#111827` | `#1a2232` | `#2c3647` | `#e8edf5` | `#95a0b2` | `#6ea8ff` | `#0f2a52` |
| cobalt | light | `#eef3fb` | `#fafcff` | `#e4ebf6` | `#c9d4e5` | `#0e1626` | `#4f5b70` | `#1c56c4` | `#d4e2fb` |
| dusk | dark | `#110d18` | `#191422` | `#221c2d` | `#372f44` | `#eee9f4` | `#a198ae` | `#b79bff` | `#2d2148` |
| dusk | light | `#f4f1f9` | `#fcfbfe` | `#ebe6f3` | `#d4cce2` | `#18121f` | `#5c5368` | `#6a3fc6` | `#e5dbfb` |
| rose | dark | `#170c11` | `#201319` | `#2a1b22` | `#402d36` | `#f3e8ec` | `#ab97a0` | `#ff86ad` | `#46182b` |
| rose | light | `#fbf1f4` | `#fffbfc` | `#f4e6eb` | `#e3cdd5` | `#200f16` | `#6a535c` | `#b4285c` | `#fbd9e5` |
| sand | dark | `#14110a` | `#1d1911` | `#27221a` | `#3c362b` | `#f1ede4` | `#a59f90` | `#e8c060` | `#3a2e10` |
| sand | light | `#f8f4ea` | `#fefcf7` | `#f0eadc` | `#ddd4c0` | `#1d1910` | `#635c4d` | `#7f5800` | `#f6e6bd` |
| volt | dark | `#0c0d0b` | `#141612` | `#1d201a` | `#30342b` | `#eceee8` | `#9a9f92` | `#c6f05a` | `#2c3a10` |
| volt | light | `#f3f6ee` | `#fcfdf9` | `#e9eee1` | `#d0d8c4` | `#12160c` | `#565e4a` | `#476a00` | `#e0efc0` |
| mono | dark | `#0e0e0f` | `#161618` | `#1f1f22` | `#333337` | `#ededee` | `#9a9a9f` | `#e4e4e7` | `#2e2e33` |
| mono | light | `#f3f3f4` | `#fcfcfc` | `#ebebed` | `#d4d4d8` | `#121214` | `#55555b` | `#1d1d20` | `#e1e1e5` |

Status colours are shared by the schemes of a theme: dark `--danger #ed756e`, `--warn #f0ba59`,
`--ok #76cf8a`; light `--danger #ac3031`, `--warn #8a5600`, `--ok #246e3a`.

Settings' scheme swatches (`button.dot` in `.swatches`, nine, cobalt first, wrapping five and four to a
row) each show their own scheme's accent for the current theme (`.dot[data-scheme]` rules at the top of `styles.css`); the chosen one holds a dot in
`--on-accent`.

### Forge's tokens, mapped onto the palette

Components use forge's names; the second `:root` block points them at the palette.

| Forge token | Maps to | Use |
|---|---|---|
| `--page` | `--bg` | `html`, `body` |
| `--card` | `--surface` | Cards, Build steps, Settings groups |
| `--rail` | `--surface` | The side rail (one fill with the cards) |
| `--inset` | `--bg` | Tiles, wells, chips, fold summaries, job tiles, the segmented track |
| `--raise` / `--raise-hi` | `--surface-2` / `--line` | Tags, menus / hover |
| `--control` / `--control-hover` | `--raise` / `--raise-hi` | Every button and input; inside `details.advanced` they step to `--card` |
| `--fg` / `--fg-2` | `--ink` / `--ink-2` | Text / paths, checks, unchosen chip labels |
| `--accent-ink` | `--accent` | Link text, folder icons, a job's current stage |
| `--good` / `--bad` / `--warn-ink` | `--ok` / `--danger` / `--warn` | Status icons and text |
| `--done-ink` / `--done-tint` | `--ok` / `--ok-bg` | `.tag.green`, `.note-line.good`, Edit's In place |
| `--danger-ink` / `--danger-tint` | `--danger` / `--danger-bg` | `.tag.red`, `button.danger`, Edit's Rebuild |
| `--transfer-ink` / `--transfer-tint` | `--done-ink` / `--done-tint` | `.tag.transfer`, `.tag.install` |
| `--bar-track` / `--bar-lo` / `--bar-hi` | `--line` / `--accent-soft` / `--focus` | The progress bar: track, then the fill from soft to bright around `--accent` |
| `--switch-off` / `--field-ring` / `--chevron` | `--raise-hi` / none / a grey chevron; light: `--line-strong` / `--line-strong` / a darker chevron | An off switch's track, an empty field's 1px inset ring, `select.lang-select`'s chevron |
| `--danger-tint-hi` | one step off `--danger-tint` | `button.danger`'s hover fill |
| `--sans` / `--mono` | `--font-sans` / `--font-mono` | Body / paths, IDs, logs |

Spacing and shape come from kraken too: `--space-1`..`--space-8` (4, 8, 12, 16, 24, 32, 48, 64px),
`--radius-sm` 6px (tags), `--radius` 10px (buttons, inputs, boxes), `--radius-lg` 16px (cards, rail),
`--dur` 180ms with `--ease-out`.

### Semantic hues

Fixed hues, tuned per theme: the dark values in `:root`, darker inks in `html[data-theme="light"]`.
Each is an ink on its own tint.

| Tokens | Use |
|---|---|
| `--edit-ink` / `--edit-tint` | Edit blue: `.tag.edit` (an edit or extract job), `.tag.blue`, the pending bar's icon |
| `--orange-ink` / `--orange-tint` | `.tag.orange`: Backport, Patch, warnings such as Offline |
| `--violet-ink` / `--violet-tint` | `.tag.violet`: DLC, Additional content |
| `--lz4-ink` / `--lz4-tint` / `--lz4-line` | The pack hue: `.tag.lz4` on a pack job |
| `--danger-wash` / `--danger-text` / `--danger-title` | A job's `.error-box`, a blocking finding (`ul.findings li.block`) |
| `--warn-wash` | `ul.findings li`, `.tile.damaged` |
| `--rate-good-ink` / `--rate-good-tint`, `--rate-ok-ink` / `--rate-ok-tint`, `--rate-bad-ink` / `--rate-bad-tint` | About's rating chips (`.chip.good`, `.chip.ok`, `.chip.bad`) |
| `--pk-ink`, `--pk-green`, `--pk-cyan`, `--pk-glow` | A packed source's AMPR tile (`.tile.neon.packed`) |
| `--wash` / `--wash-hi` | An explorer row's hover / focus fill |
| `--orange` | The brand glow's orange (the logo's shadow), never text |

**Format hues.** `.fmt-*` sets `--h` (text), `--h-bg` (tint) and `--h-line`; a light value for each
follows under `html[data-theme="light"] .fmt-*`. They mark a format wherever it is named as a fact:
the `.tag.fmt` pills (`FormatPill` in `common.tsx`), a job's format pill, About's
Compare formats rows and the Kraken level chips (below).

| Format | Class | Dark `--h` | Light `--h` |
|---|---|---|---|
| Folder | `fmt-folder` | `#ececf4` | `#2e3438` |
| exfat | `fmt-exfat` | `#ffa3e6` | `#9a1278` |
| ffpkg | `fmt-ffpkg` | `#a3c0ff` | `#2443b0` |
| ffpfs | `fmt-ffpfs` | `#7ee0e0` | `#0b6467` |
| ffpfsc | `fmt-ffpfsc` | `#ffc580` | `#844700` |
| FPKG | `fmt-pkg` | `#8fe0ab` | `#1d6838` |
| zstd packs | `fmt-zstd` | `#7fe3d8` | `#0c645c` |
| LZ4 packed folder | `fmt-lz4` | `#d6e875` | `#536200` |
| Archive (zip, 7z, rar) | `fmt-archive` | `#e8c9a6` | `#74501f` |

**Semantic tags** (`.tag`, 22px, `--radius-sm`, `--raise` + `--fg-2` when plain): `.tag.blue` a running
stage, `.tag.green` Done, Connected and another device's name, `.tag.red` Failed, `.tag.orange`,
`.tag.violet` as above. Job types: `.tag.edit`, `.tag.lz4`, `.tag.transfer`, `.tag.install`.

## Surfaces

Three fills, one step apart: `--bg` (the page under everything, and `--inset`: the wells inside a card,
such as tiles, lists, chips, job tiles, fold summaries and `.out-box`), `--surface` (`--card`: cards and
the rail) and `--surface-2` (`--raise`: controls, tags, hovers). In dark each step is lighter, in light
darker for the wells and controls. A box inside an inset well steps back to `--card` (a job's fold
summaries and checks), so neighbours never share a fill.

No `border` draws an edge. What separates rows of one list is a 1px `--line` divider that stops short
of the list's edges (`inset-inline: 12px`): `.xrow::after` and `ul.rows > li::after` (the explorers,
the picker, plain row lists), none under the last row. The other lines are dividers of the same weight
inside a padded box: `.settings-row + .settings-row`, `.quit-jobs li + li`, `.rail-foot` above the
rail's foot, and `.storage-actions .library-views` before the Package installer's Cards/List buttons.

Shadows (`--shadow`) belong to layers that float: `.menu`, `.modal`, `.update-toast`, the phone's
bottom rail. A card has none.

## Controls: one family

- **Buttons** (`button`, `a.button`): 38px, `--radius`, `--control` fill, `--fg`, 500; hover
  `--control-hover`; pressed moves 1px down; off (`:disabled` or `aria-disabled="true"`) at 42% opacity
  with `cursor: not-allowed`. `aria-disabled` keeps focus, so a busy button (Refresh while scanning, a
  running Speed test) stays where the keyboard is; a busy one (`aria-busy="true"`, `Dots` after its
  label) stays at full opacity, so busy never reads as refused. Every hover changes the fill, never the
  opacity.
  - `button.primary`: the accent's soft fill with `--accent` text, 600; hover `--accent-soft-hi`. One per
    card.
  - `button.danger`: `--danger-tint` with `--danger-ink`; hover `--danger-tint-hi`.
  - `button.small`: 30px, 12.5px (the explorers' action columns are sized in 30px buttons);
    `.icon-only` makes it square.
  - `button.link`: a quiet text button in `--accent-ink`, underlined on hover (crumbs, Open Queue).
- **Inputs** (`input`, `select.lang-select`): the button's 38px, radius and `--control` fill, no border,
  13px (`input.mono` 12px); the placeholder `--muted`. In light an empty field gets a 1px inset
  `--field-ring` (>= 3:1 on the card); `aria-invalid="true"` rings it in `--bad`. The select draws its own
  chevron (`--chevron`, one per theme).
- **Chips and segment options**: one 32px pill (`border-radius: 999px`), 13px/500, quiet until chosen;
  chosen = a soft fill + 600.
  - `.seg` / `.seg-opt`: a segmented track (`--inset`, 3px padding) of pill options; chosen
    (`.on`, `aria-pressed`, `aria-checked`) `--accent-soft` + `--fg`. Used by the view switch
    (`.seg.view-switch`: Storage's views, Connect's filter), the picker's device
    tabs (`.seg.pick-tabs`) and Settings' theme choice. Edit's mode choice is a `.seg.formats.inner` of
    `.seg-choice.mode-hue` options: In place in the Done green (`.mode-green`), Rebuild in the danger red
    (`.mode-red`), each option's text in its hue chosen or not.
  - `button.kind-chip` (`.kind-chips`): the Queue's (`.queue-filter`) filters and
    Settings' Output names tokens (`button.naming-chip`); `--inset` until chosen, a count in `.n`;
    chosen in its kind's hue (`--h-bg` / `--h`), else the accent's soft fill.
  - `button.b-chip` (`.b-chips`, `build/build.css`): Build's Target chips for the packs and the format,
    36px, `--radius`, 13.5px/600 (the level chips' size), `--inset` until pressed; pressed (`aria-pressed`) = `--accent-soft` fill,
    `--accent` text and a 1.5px `--accent` border. Pack chips have no icon, format chips keep theirs.
    `button.b-more` (More, its chevron turning) toggles the rest on every click; closed, it still shows the
    chosen chip behind it.
  - **Recommended**: `.b-best`, a plain 8px `--ok` dot after the label (None and FPKG; title
    "Recommended", a visually hidden ", recommended"). A row's Recommended notice (`.b-rec`: the dot +
    the word, 12.5px/600 `--ok`, at the row's end, the switch's `aria-describedby`) marks Restore metadata
    online.
  - `.bm-chip` (`.bm-strip`, `devices.css`): a bookmark (the Transfer panes and the File explorer only), a
    folder name and an x (full opacity) in one `--accent-soft` pill, hover `--accent-soft-hi` (by fill),
    the folder shown 700 (40px on touch); its toggle
    (`button.bm-toggle`, a quiet icon button at the end of the crumbs bar) is `--accent` and filled while
    pressed.
- **Device tabs** (`DeviceTabs.tsx`, `devices.css`; the Transfer screen and the File explorer): a browser's
  tab strip (`.dev-tabs-row` > `.dev-tabs` of `button.dev-tab`: icon, name, a green `.dev-tab-dot`; a thin
  divider between unchosen tabs) over the card it shows (`.card.dev-window`, its start-top corner square).
  The chosen tab takes `--card` and 600 so it joins that card, its icon in `--accent`; hover `--surface-2`;
  an offline tab (`.offline`) at 55%, its dot `--muted`. Too many tabs shrink, then the strip scrolls
  sideways. `button.dev-tab-add` (+, quiet) at the end opens Connect.
- **Format chips in one colour.** Build's format row (`button.b-chip`, no `.fmt-*` class) and its
  packs row choose in the accent's soft fill, as every other choice does. A format keeps its hue where
  it is shown as a fact (`.tag.fmt`, About).
- **Level chips are toned.** The compression levels (`button.b-lv`, 36px, the existing level icons: the
  pack preset and the FPKG Kraken level) take a tone: fast green (`.b-green`, `--ok`), medium/balanced
  yellow (`.b-yellow`, `--warn`), max/smallest red (`.b-red`, `--danger`), stored gray (`.b-gray`,
  `--muted`); the icon in the tone, pressed = the tone's bg, text and border. The image inside an ffpfsc
  uses pills in its own format's hue (`FormatPicker`); the ffpfsc zlib level stays a `.range`.
- **Result panel** (`.b-result`, beside the choices in `.b-split`: choices | its content's width, at least
  200px, the choices' full height; one column under 1000px with the rail open, under 760px collapsed; `hr.b-sep`,
  a 1px `--line`, under both): an `--inset` box, a 14px gauge (`.b-track`, `--raise`; `.b-fill` an accent
  gradient from the bottom, amber `.over` with a `--fg` `.b-tick` at now when the build is larger), then a muted
  "Game now" (12px) and its size (600, `--fg-2`) at the top, "About, after the build" and the build's size
  (`.b-big`, 28px/700, tabular) at the bottom; no percentage tags; `Dots` while core's `pack_estimate` runs.
- **Switch** (`.switch`, `.switch-track` over a native checkbox): 34x20, `--switch-off` off (light:
  `--line-strong`, >= 3:1), `--accent` on;
  a refused one dims its track only, the label keeps its contrast.
- **Range** (`.range`): native, `accent-color: var(--accent)`, its value in `.range-value` (tabular).
- **Menu** (`.menu-wrap` > `.menu`): a floating `--raise` list with `--shadow`, 32px items; `.up` /
  `.start` when the scroller would clip it (`ChooseMenu` measures).
- **Focus**: a 2px `--focus` outline at 2px offset on `:focus-visible` only; inside a row or a tight
  control the offset turns inward (-2px). Focus that code moves (`tabindex="-1"`) shows no ring.

## Cards and folds

- **Card** (`.card`): `--card`, `--radius-lg`, 20px padding (16px under 600px), no border, no shadow.
  Its head (`.card-head`) holds an `h2` at 16px/600 (-0.01em), 14px above the body, at least 28px tall;
  actions go in `.head-actions` at its end, an icon before the title in `.head-icon`. `.card.compact`
  tightens the head to 10px. A Build step (`.card.build-step`, `.build-step-head` + `h3`) and a Settings
  group (`.settings-group`) are the same card. 16px between cards (`.screen`, `.cols`, `.build-steps`).
- **Hero** (`.card.hero`): the game card. Its `::before` layer holds the glow (Cover glow, below); the
  light theme makes the brand glow fainter. No "Source" header: the section is labelled `aria-label`, its
  head (`.src-head`) holds only the actions at its end. Build's card once a game is read is `.src-2b`
  (`source-card.css`): a 200px cover on the left (128px under 880px; under 600px above the text, Pick on
  its row), everything else on its right, Pick on the title's line at its end, compact tiles (7px 10px, 11px
  `dt`, 14px value: Title ID, Version, Firmware, Size; no AMPR tile), each as wide as its content and
  wrapping (a `.wide` tile takes its row). Build's card lists no findings (every one is a Summary line, by user decision); a source that
  can't be read, or isn't a game (no title, no title ID), is an error state on any card (`.empty`,
  `.empty-icon.bad`, the path, the error or the findings one per line, Pick). A packed game: a green `zstd`/`LZ4`
  tag right after the kind pill.
  Edit's card is the same, stacked (`.src-2b.stack`: the under-600px layout at every width, a 120px cover;
  Pick and Delete at the cover row's end). Pick in the head is `small primary` (accented, as the empty state's).
- **Tiles** (`.tiles` > `.tile`): facts the user decides on (title ID, version, firmware, size) in an
  `--inset` box, 12px radius: a muted `dt` above an 18px/600 tabular `.value`. `.tile.wide` spans the row;
  `.tile.neon.packed`, `.tile.damaged` and `.tile.unpacked` are the AMPR tile's states.
- **Inset boxes**: `.out-box` (an output name or path, mono), `.field.backport` / `.field.lz4` /
  `.field.verify` (a switch apart from the lines around it), `ul.checks`, `.fold-box`, `.edit-why`.
- **Folds** (`details.fold`): an `--inset` summary with a small chevron, 10px radius; a job's folds sit on
  `--card` inside its tile.
- **The Advanced fold** (`details.advanced`, `Advanced` in `prefs.tsx`): pro settings out of the way,
  under Build's packs and options steps. A `--raise` box at `--radius`, no border; its summary shows the
  title (`.advanced-title`, 13.5px/600), what is inside (`.advanced-hint`, muted) and at its end a badge
  (`.tag.advanced-badge`): `.muted` "Defaults", or `.blue` "Changed" once a setting inside differs.
  Controls inside step to the card fill (`--control: var(--card)`), and the inset boxes and tracks
  inside sit on `--card`, so every level stays one step off its ground. `.inline` drops the box: a plain
  row summary on the card, controls back to `--raise`. Closed by default; Settings' "Always show
  advanced options" (`advancedOpen` in `forge.ui`) opens every fold.
- **Empty state** (`.empty`): a flat `--inset` well (no glow: only the game card glows), a round
  `.empty-icon` (`.bad` for a failure), `.empty-title` at 16px/600 and a muted line; the source's format
  chips in `.kinds`.

## App shell: the side rail

The screens live in a side rail (`.shell` > `nav.rail` beside `.content`), kraken's Workbench; it
replaced the old sticky header and its tab bar. `.content` scrolls on its own; `main` caps the column at
1264px with a 32px gutter (`--gutter`, 20px under 1000px).

- **Rail** (`.rail`, `--rail-w` 15rem): a `--rail` card with `--radius-lg`, 12px off the window's start
  edge (the right in RTL). `.rail-head`: the logo (`.brand`, 30px, its orange glow a `drop-shadow` that
  follows its shape) and the collapse button (`button.rail-toggle`). `.rail-nav`: one vertical tablist
  (`RailNav` in `App.tsx`; Up, Down, Home, End) of Build, Queue, Transfer, Edit, Storage, Connect, Settings,
  About. No screen has a page header: the rail names it.
- **Items** (`.rail .rail-item`): 40px, `--radius`, a 20px icon and a `.rail-label`; `--fg-2` until
  hovered (`--surface-2`). Selected (`aria-selected="true"`): `--accent-soft`, `--fg`, 600, its icon in
  `--accent`. A tab's unfinished jobs show as `.count` at the label's end (`--fg` on `--page`; on the
  selected item `--accent` + `--on-accent`).
- **Foot** (`.rail-foot`, after a divider): Connect's state as a button that opens Connect
  (`button.rail-item.conn-status`: a `.conn-dot`, green `.on` while a device is online, and the `.tag`
  "Disconnected" or `.tag.green` "N devices"), the GitHub link (`.gh-link`), and in the web build the orange Offline tag and Stop.
- **Collapsed** (`html.rail-collapsed`, Settings or the toggle; `--rail-w-collapsed` 4.5rem): icons only.
  The labels shrink to nothing but stay for screen readers, and a count becomes a 9px accent dot on its
  icon.
- **Phones** (under 700px): the rail becomes a bottom bar of icons (60px, floating 8px off the edges,
  `--shadow`); all 8 screens fit at 320px (the items shrink, icon only); the page scrolls. The head,
  GitHub and Connect's state leave the bar: the Connect item carries a status dot (`.conn-badge`, green
  `.on`), and the web build's Offline tag and Stop float as small chips just above the bar's end.
- **PS5**: no L2 / R2 badges (the triggers still step through the screens); `html.ps5` zooms the page 1.07,
  one text size up.

## Screens

- **Build** (`.screen.build`, `build/*.tsx`, `build/build.css`; Convert and Pack in one): a column of step
  cards with no numbers (`.build-steps` > `.card.build-step`): Source (`.src-2b`), Target (Packs and
  Format `.b-chips`, the level chips, the result panel; a small Compare formats in its head, How pack works
  among the opened pack chips), Backport (the Remove backport switch, off with a muted `.b-why` line when it
  doesn't apply), Options (Restore metadata online with its Recommended notice, Delete the source, the
  Advanced fold with Full verification), Summary. Settings' "Always show advanced options" shows every chip
  (no More, no How pack works) and opens every fold. Above the Build button: `ul.build-summary` (the accent's
  soft fill, no list bullet, one coloured dot per item, `li.warn` and `li.block` in their colours),
  `.build-space` (free
  space, `.warn` when short) and `.build-row` with the one `button.primary`. A start that waits shows
  `.build-queued` with Open Queue.
- **Queue** (`Queue.tsx`, `queue.css`): `.queue-head` (the counts, `.queue-counts`, no title; `.queue-actions`, the
  `.queue-filter` chips), then one `.job` tile per job (`--inset`, 12px radius, newest first) in
  `.job-list`. A row: `.job-head` (type and status tags, `.job-name`, actions), the stage bar
  (`.segbar`: one `.segbar-seg` per stage, as wide as its weight; `.done` full, `.now` filled to `--p` (set
  inline per job), `.alive` sweeping while it runs), the stage breadcrumb (`ol.stage-crumbs`, in the reading
  direction, apart from the explorer's `.crumbs`: `li.done` with a
  green tick, `li.now` bold in the accent, `li.fail` red), the status line (`.job-status`, `.meta`; a
  finished job's `.result-line`), then the error (`.error-box` with `.error-title`), checks and log
  (`.log-bar`, `pre.log.log-panel`). A peer's mirrored job (`.job.remote`) is greyed at 80% but for its
  Cancel.
- **Edit**: the game card beside the explorer (`.cols.edit`, 1:2 from 1100px). The explorer
  (`.explorer`, `.explorer-well`, `.xgrid` of `.xrow`s: `.xname`, `.xsize`, `.xact`) marks changes with
  `.tag.xmark`, a drop target with the accent's soft fill (`.xrow.drop`, `.explorer-well.drop`); the
  `.pending-bar` above it carries the count, `.edit-mode` and the uploads' `.upload-line`.
- **Storage** (`.storage-card`, `.storage-head` with `.seg.view-switch` first on the PS5, `.storage-actions`):
  the File explorer (`.files-panes`, `.files-tree` beside the list from 880px, its current folder in the
  accent's soft fill; the device tabs above it, each tab's card a `.dev-window`) and on the PS5 the Package installer (`.library-well`,
  `.library-cards` of `.library-card` or `.library-rows` of `.library-row`). The PS5's free space reads in
  `.library-storage`.
- **Transfer** (`.transfer-screen`, `transfer.css`): the device tabs (`.transfer-tabs`) over one
  `.card.dev-window.transfer-window` holding `.transfer-panes`: two `.transfer-pane`s side by
  side from 880px with a 1px `--line` between them in the gap (inside the card's padding), stacked when narrower (`.transfer-head`, `.transfer-grid` of explorer rows with a checkbox,
  `.transfer-selected`; a selected row in `--accent-soft` with a 600 name, no side stripe; a drop target in
  the accent's soft fill, no dashed outline; one `button.primary` Copy per card, the side changed last; a
  device name in "Copy N items to <device>" / "Install on <PS5>" ellipsised, whole in its `title`; `.transfer-offline`; the web build's
  `.transfer-browser-pane` with `.transfer-dropzone` and `.transfer-download`), each side's `.transfer-bar`
  under them (`.transfer-bars`).
- **Connect**: two columns from 980px (`.connect-cols`): This device (`.connect-me`, the QR in
  `.connect-qr`, `.connect-ip`) and the add form (`.add-form`), then Devices (`.dev-list` of `.dev-row`,
  `.online-dot`) and Nearby (`.nearby`). The pairing check number is `.connect-code` (mono).
- **Settings** (`.settings-screen`, at most 860px): `.settings-group` cards of `.settings-row`s (a label and
  hint at the start, the control at the end, an inset divider between rows): Appearance (theme as a
  `.seg`, scheme as `.swatches`, the language), Advanced (always show the folds, the ampr_emu choice
  with the bundled ampr-emu-ng's version as a `.tag`), Output names and, on the PS5, notifications.
- **About**: Compare formats (`.fmt-table` of `.fmt-row`, ratings as
  `.chip.good` / `.ok` / `.bad`: tint, icon and text) and Credits (`.credit-list`).
- **Picker** (`.modal.picker`, 560px): device tabs (`.seg.pick-tabs`), `.crumbs`, this device's bookmark chips (`.bm-strip`, local tab only, no toggle), an inset list (`ul.rows.pick-list` of `button.pick-row`; `.chosen` in the accent's
  soft fill, `.off` greyed) and `.pick-foot`.
- **Dialogs** (`.modal-backdrop` > `.modal`, 440px, `--card` with `--shadow`): one question each, Cancel
  focused for a destructive one; `.modal.help` for How pack works and How to connect; the pairing prompt
  over everything (`.pair-layer`). The update toast (`.update-toast`) sits bottom-right (bottom-left in
  RTL), full width under 600px.
- **Web build**: Stop and the Offline tag in the rail's foot, the quit dialog (`.quit-jobs`), a finished
  job's path shown selected to copy (`.out-box.path-shown`; plain HTTP has no clipboard). The product is
  named "Dump Forge" there.

Icons come from `icons.tsx`: 24-unit grid, stroke 1.8, `currentColor`, 18px (15px in small spots, 20px
in the rail). Add a path there rather than importing an icon set.

## Type and spacing

System font (`--font-sans`), 14px/1.45 body; `--font-mono` for paths, file names, IDs and logs.

| Size / weight | Use |
|---|---|
| 20/600 | Game title (`.title`) |
| 18/600 | Tile values (`.value`, tabular) |
| 16/600 | Card, step, Settings group, empty-state and Queue titles |
| 14/500 | Rail items (600 selected), Settings labels |
| 13.5/600 | Job names, the Advanced fold's title |
| 13/500 | Buttons, inputs, chips and segment options (600 chosen) |
| 12.5 | Labels, meta, notes, hints in Settings |
| 12 mono | Paths |
| 11.5 | Hints, notes under tiles |
| 34/600 mono | Pairing's check number (`.connect-code`) |
| 15 mono | Connect's IP tile (`.connect-ip`) |

Spacing: 16px between cards and between a screen's columns (two from 880px), 20px inside a card, 14px
from a card's head to its body, 12px between groups in a card (`.field`), 8px between chips, tags and
tiles.

## Light-theme contrast

Measured for this file with the WCAG 2 formula on the light blocks of `styles.css`, each `rgba()` tint
composited over the ground it sits on (`--surface` for a card, `--bg` for an inset well), in all three
schemes. Checked: `--ink`, `--ink-2`, `--muted`, `--accent`, `--focus` and the status colours on
`--surface`, `--bg` and `--surface-2`; `--on-accent` on `--accent`; the primary button (`--accent` on
`--accent-soft`); every status, semantic and format ink on its own tint; `--muted` on the accent,
warning and danger fills; the brand glow's corner on the hero card. Lowest that pass:

| Pair | abyss | ember | moss |
|---|---|---|---|
| `--accent` on `--accent-soft` (`button.primary`) | 5.09 | 4.53 | 5.22 |
| `--muted` on `--line` (the `--raise-hi` hover fill) | 4.57 | 4.63 | 4.58 |
| `--focus` on `--surface-2` | 4.58 | 5.41 | 4.81 |
| `--warn` on `--warn-bg` | 5.17 | 5.17 | 5.17 |

**Findings under 4.5:1** (not fixed here):

- `.job.remote` (a peer's mirrored job, 80% opacity): `--muted` on `--inset` drops to 3.94 (abyss),
  3.98 (ember) and 3.94 (moss). The CSS comment's 4.8:1 holds for dark only.

Not measured, for the next pass:

- The cover glow in light: `glow.ts` dims a cover's colours against a fixed dark card (`CARD`
  `#17171f`, `MAX_L` 0.028), so a cover's glow on a light card is not held to 4.5:1 by that code.

## Cover glow (`glow.ts`)

- The cover (a `data:` URL) is drawn to a 24x24 canvas; pixels are bucketed by hue into 12 bins of
  30 degrees, skipping near-greys, near-blacks and near-whites.
- The two most weighted saturated hues become `--glow-1` and `--glow-2`, dimmed until the card's brightest
  point keeps muted text readable (they start at 0.54 / 0.33 alpha); the last 16 covers are cached.
- Its reach (by user decision): the glow holds full strength over about the card's top-right half
  (a flat stop at 25% / 20% of each gradient, then fading out by 80%), never brighter than that cap.
- The layer (`.card.hero::before`) fades in (`.lit`) once its colours are known, so a cover's colours
  never follow a flash of the brand ones. No cover, or any failure: the brand glow (blue + orange).
- Old WebKit: the cover loads through `onload` (no `decode()`), and the layer uses four offsets (not
  `inset`) inside a `z-index: 0` stacking context.

## Motion

Short fades and transforms only (`--dur` 180ms, `--ease-out`): the hero glow (0.5s), the switch and
fold chevrons, the rail toggle, the spinners, the waiting dots (`.dots`), and a running job's sheen
(`.segbar .segbar-seg.alive`, 1.8s). `prefers-reduced-motion: reduce` turns them all off.

## Copy

- Plain user language: say what a format is for, not its spec. About explains the formats.
- The debug package is labelled FPKG wherever it is shown; files are saved as `.pkg`, and the internal
  id and the CLI flag stay `pkg`. The UI names formats without the dot.
- Errors lead with a sentence a user understands ("Can't build this game as **exfat**:"), then quote
  core's lines unchanged.
- Button and menu labels name the action plainly, no trailing ellipsis; a state in progress takes the
  animated `Dots`.
- Say "game", never "dump" (the product names excepted).
- Every UI copy change updates all eleven files in `app/src/locales/` (`en.tsx` is the source).
