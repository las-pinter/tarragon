# Mockup checklist

The owner checks the app against each mockup in `design/mockups/index.html`.

- The owner fills the last three columns by hand. Agents do not.
- The result words are `pass`, `fail`, and `pass with delta`.
- A `pass with delta` row names the heading of `docs/design-deltas.md` that records the delta.
- The agent pre-check column is a hint. It is never a sign-off.
- A `fail` row stays open until the owner picks a fix or records a delta.
- `$SEED` is the scratch folder from the seed recipe below.

| ID | Mockup | Look for | Agent pre-check (not a sign-off) | Owner result | Initials | Date |
|----|--------|----------|------|------|------|------|
| D1 | theme.html | Open the app: colors, spacing, and radii match; the font is Roboto, not Segoe UI (deltas, UI font and Monospace face). | - | | | |
| D2 | main-window.html | Open `$SEED/pics`: Library 220 px, Preview 490 px, Log hidden at first; docks resize and move, tabs do not float (delta, Dock regions). | - | | | |
| D3 | menubar.html | Open each menu: File holds Open Folder and Quit, not Open Folder only (delta, section File menu); View and Settings match. | - | | | |
| D4 | library.html | Check the folder tree, the selected-folder tint, and favorite labels (deltas, Library sidebar layout, Favorite labels and roots). | - | | | |
| D5 | favorites-reorder.html | Drag a favorite between two rows: the new order stays; the grip shows on every row, no hint line (delta, section Favorites drag). | - | | | |
| D6 | empty-no-folder.html | No folder open: the Open a folder card matches; Preview says "Select an image to preview it" (delta, Preview empty state and picture box). | - | | | |
| D7 | scanning-busy.html | Open `$SEED/big`: the indeterminate busy bar and the loading cells show (a small folder ends too fast to see). | - | | | |
| D8 | grid-single-select.html | Click one tile: it shows as selected and Preview shows its metadata; double-click on a tile opens the image in the editor. | - | | | |
| D9 | grid-multi-select.html | Ctrl-click several tiles: the count matches; the check mark is painted (delta, Selection check glyph); Enter with several opens nothing. | - | | | |
| D10 | grid-hover.html | Hover a tile: the fill, border, and shadow fade, but it does not scale (delta, Grid hover scale); Enter with one tile selected opens it. | - | | | |
| D11 | gallery-tabs.html | Switch between the Folder and All Images tabs: Filter Folders shows only in All Images (deltas, Active tab underline, All Images scope). | - | | | |
| D12 | info-bar.html | Info bar: name and count, the amber pill ("1 filter active"), Name and Date pills (deltas, Filter count pill, Sort pill styling). | - | | | |
| D13 | filter-colors.html | Click the color chips: the row has twelve swatches, with pink and transparent (delta, section Filter colour row), and the grid filters. | - | | | |
| D14 | filter-tags.html | Open Filter Tags and check tags: amber chips show in order; a Manage tags row is added (delta, section Tag manager and tag rules). | - | | | |
| D15 | filter-folders.html | In All Images, use Filter Folders: chips show the last two path parts, a tooltip shows the full path (delta, Folder labels). | - | | | |
| D16 | preview-single.html | Select one tile: pill x shows on hover; twelve color squares (deltas, Preview tag pills and the add menu, Preview colour squares). | - | | | |
| D17 | preview-mosaic.html | Select more than nine tiles: the mosaic and "+3 more" caption show; no badge or loading box yet (delta, Preview mosaic). | - | | | |
| D18 | tag-edit.html | In Preview, choose Create new in + add: an inline input, Enter adds, Escape cancels; name rules apply (delta, Tag manager and tag rules). | - | | | |
| D19 | context-grid.html | Right-click a tile: the mockup shows Regenerate Thumbnail, the app shows no menu (delta, section Editor templates). | - | | | |
| D20 | context-preview.html | Open the Preview "+ add" tag menu: it lists tags some files lack, no color tags (delta, Preview tag pills and the add menu). | - | | | |
| D21 | settings.html | Open Preferences: the groups match; the window is 520 points wide, saves only on OK (delta, section Preferences dialog). | - | | | |
| D22 | settings-cache.html | Preferences, Cache: the cap row, jpeg-only format, Purge Cache busy strip, OK, Cancel (delta, Preferences dialog). | - | | | |
| D23 | log-panel.html | Open the Log dock: lines by level, Debug toggle; no critical level, not monospace (deltas, Log levels and toolbar, Monospace face). | - | | | |
| D24 | empty-folder.html | Open `$SEED/empty`: the empty-folder card and button match; the hint lists ten extensions (delta, section Empty states). | - | | | |
| D25 | busy-states.html | Check the busy strip. Scan: `$SEED/big`. Purge: Purge Cache with a filled cache. Decode: `$SEED/formats` and the large PSB. | - | | | |
| D26 | preview-formats.html | Open `$SEED/formats` and select the PSD, CLIP, and KRA: the Format line shows the extension, such as PSD (delta, Preview format detail). | - | | | |
| D27 | error-state.html | Open `$SEED/broken`, select `broken.png`: the card, Retry, Open in editor; the hint text differs (delta, section Error card). | - | | | |

## Sittings

| Sitting | Rows | Group |
|---------|------|-------|
| 1 | D1-D7 | Shell |
| 2 | D8-D12 | Grid |
| 3 | D13-D17 | Filters and Preview |
| 4 | D18-D23 | Edit, menus, settings, log |
| 5 | D24-D27 | States |

## Seed recipe

Run these steps from the repository root. They build the test folders.

```sh
export SEED="$(mktemp -d)"
G="cargo run --release --example corpus_gen -p tarragon-scan --"
$G --out "$SEED/pics" --files 24
$G --out "$SEED/more" --files 12
$G --out "$SEED/big" --files 10000

mkdir "$SEED/empty"
echo "not an image" > "$SEED/empty/notes.txt"

mkdir "$SEED/broken"
printf 'not a png' | head -c 8 > "$SEED/broken/broken.png"
ls "$SEED/pics" | head -n 2 | while read -r f; do cp "$SEED/pics/$f" "$SEED/broken/"; done

mkdir "$SEED/formats"
cp tests/fixtures/psd/rgb_8bit.psd tests/fixtures/clip/preview.clip \
   tests/fixtures/kra/merged.kra "$SEED/formats/"
```

For the large PSB, run `sh tests/fixtures/psd/build_large_psb.sh "$SEED/formats/large.psb"`.
It needs `python3` and the packages in `tests/fixtures/psd/requirements.txt`.

Start the app with these variables:

- `TARRAGON_DB="$SEED/test.db"` keeps the real database out of the test.
- `TARRAGON_FAKE_PICK=<folder>` replaces the folder dialog. Use it only where a
  sitting says so. The D1 to D7 sitting tests the real dialog.

For the rows that open a file, give the app an editor that records the path. Do this before the start step.
A template splits on spaces and double quotes only, so the editor is a small script.
The insert creates the table when the database is new. The app keeps the row.

```sh
printf '#!/bin/sh\necho "$1" >> "%s/opened.txt"\n' "$SEED" > "$SEED/record.sh"
chmod +x "$SEED/record.sh"
export TARRAGON_DB="$SEED/test.db"
python3 -I -c '
# The table is the same as in crates/tarragon-db/src/schema.rs. Keep it in sync.
import sqlite3, sys
db = sqlite3.connect(sys.argv[1])
db.execute("CREATE TABLE IF NOT EXISTS editor_associations (extension TEXT PRIMARY KEY, command_template TEXT NOT NULL)")
db.execute("INSERT OR REPLACE INTO editor_associations VALUES (?, ?)", ("*", sys.argv[2] + " {file}"))
db.commit()
' "$TARRAGON_DB" "$SEED/record.sh"
```

Start the app last, in the same shell, so `TARRAGON_DB` is still set. Build once,
then give it a folder. The log goes to `$SEED/tarragon.log`.

```sh
cargo build --release
target/release/tarragon "$SEED/pics"
```

After a double-click or Enter, `$SEED/opened.txt` holds the opened path.
