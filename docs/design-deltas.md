# Design deltas

The mockups in `design/mockups/` stay frozen and remain the design of record.
This file records every place where the application differs from them. Each
entry gives the state before the change, the state after it, and the reason.
Every colour, layout region, label, and interaction stays as designed unless
an entry below says otherwise.

## Cache directory label

Before: the settings cache field showed the resolved per-user cache directory.
That text named a folder below the user's home directory.

After: the field reads `Default cache directory`, as the mockup shows.

Reason: the design shows a fixed label, not a live path. A path in the design
would differ on every machine and would show one user's folder.

## Outside references in the design folder

Before: `design/README.md` pointed to an archive of alternative designs and to
a separate design-alternatives document, both outside the repository. The
comment block in `design/mockups/theme.css` named the source files of the
previous implementation.

After: both files name no source outside the repository. The palette values are
unchanged.

Reason: a repository document must stand alone, and a path outside the
repository may not resolve for a reader.

## File menu

Before: the mockup menu bar gives the File menu one item, Open Folder, and says
it has no Quit item. The OS draws the window controls, so the mockup title bar
shows none.

After: the File menu holds Open Folder and Quit. Quit sends a window close
request. Open Folder opens the folder dialog, as "Open Folder and roots" says.

Reason: the frame has no close control of its own, so the menu must offer one.

## UI font

Before: `design/mockups/theme.css` line 105 set `--font-sans` to a stack that
starts with Segoe UI, then falls back to other system fonts.

After: the interface uses the vendored Roboto family. Regular serves the
proportional family, Roboto Medium draws the panel titles, and Roboto Bold
draws the title bar. Those two weights come through named egui font families,
because egui 0.36.2 has no weight selection.

Reason: Segoe UI is a system font and no license permits shipping it. Roboto is
embedded in the crate under Apache-2.0.

## Monospace face

Before: `design/mockups/theme.css` line 106 set `--font-mono` to a system
monospace stack, so log lines had a fixed character width.

After: Roboto Regular serves the Monospace family. Log text stays readable but
is not monospaced.

Reason: no monospace face is vendored yet. Roboto Mono is Apache-2.0 and is
allowed, so the monospace delta closes when that file lands.

## Dock regions

Before: the design draws docks that a docking layout manages. The default sizes
are Library 220 px, Gallery in the centre, Preview 490 px, and Log hidden.

After: the four panels sit in an egui_dock area. A drag on a divider resizes
the panels beside it, and a drag on a title moves a panel. A tab cannot float
as a window. The Log starts hidden. The split shares come from the sidebar and
preview widths over the default window size, so a maximized or resized window
scales the panels.

Reason: the design asks for docks that the user can resize and move, and
egui_dock gives that.

## Dock titles and menu

Before: the mockup `.panel-title` strip shows one title for each panel. It has
no buttons and no menu.

After: the dock tab bar is the panel title. It takes the strip fill, the amber
text, and a height from the title font. The 8 px side padding is fixed inside
egui_dock. A leaf that holds two panels shows two titles side by side, which the
mockup never shows. Only the chosen panel's body shows, and the other panel
waits behind its title. The bar has no close, close-all, collapse, or add
buttons. A right click on a title gives Hide panel and Reset Window Layout. The
View menu also toggles each panel.

Reason: a move needs a title that the user can grab, so the title and the tab
are one control. Hide and reset need a menu because the bar has no buttons.

## Dock layout state

Before: the design does not say how a layout lasts between runs, or how a
hidden panel returns.

After: a hidden panel comes back at its default place next to the panels that
are shown, and at its default width while the Library is shown. With fewer
panels shown, it takes its place relative to the ones that remain. The layout
is saved with the window state at exit and restored at start. A saved layout
that does not load gives the first layout, which has the Log hidden. Reset
Window Layout resets the panels only. It does not change the window size or
position.

Reason: a layout that the user arranged should last. A bad saved layout must
not stop the start, and a reset should not move the window.

## Window state

Before: the design does not say where the window opens.

After: at exit the app saves the maximized flag with the layout and the sort
mode. It does not save the position or the size. A window that was maximized
opens maximized. Any other window opens at the default size, 1280 by 800 px,
centered on the primary monitor. The next save drops any older saved position
and size. Known limits: a Wayland compositor places the window itself, so
centering may not apply. The primary monitor is used, not the last monitor of
the window. A minimized or full-screen window keeps the earlier saved value.

Reason: the window does not need to reopen where it was left. Tracking the
window edges and the frame offset was unreliable across window managers.

## Dock chrome

Before: the stylesheet sets four chrome details that the frame does not match.
The title bar text inherits the body size, and the frame draws it at the heading
size. Panel titles take 6 px by 8 px of padding, and the frame takes the
extra-small spacing of 4 px by 8 px. Panel titles use the semibold weight of
600, and the frame uses Roboto Medium, because egui cannot select a weight. The
title bar, the menu bar, the sidebar, the preview, and the log each take a one
pixel separator border in the stylesheet, and the frame paints none of them.

After: the frame draws the title bar text at the heading size, gives the panel
titles the extra-small vertical padding and the Roboto Medium face, and paints
no separator borders.

Reason: these four differences are visual only. Every colour, region, label,
and interaction stays as designed. egui 0.36.2 has no weight selection, so the
Medium face is the nearest vendored weight to 600.

## Spacing and radii

Before: the stylesheet numbers came from the Qt desktop theme: 4, 8, 12, 16,
and 24 px for spacing, and 0, 3, 4, 6, 8, and 10 px for corner radii.

After: egui uses the token values directly for item spacing, button padding,
window and menu margins, and the window corner radius. There is no second
metric layer.

Reason: one value set for the stylesheet and the code keeps the two in step.

## Grid hover scale

Before: `design/mockups/theme.css` line 765 scales a hovered cell by 1.02 over
150 ms, so the hovered cell lifts above its neighbours.

After: the tile keeps its size. The hover fade changes the fill, the border,
and the shadow only.

Reason: a scaled tile overlaps its neighbours, and the grid does not reflow
its clip rect for the larger cell. The difference is visual only.

## New row fade

Before: `design/mockups/grid-hover.html` line 54 fades a new row in over
200 ms, with a stagger between rows.

After: new rows draw at full opacity on their first frame.

Reason: the virtual grid builds a cell when it enters the viewport, at any
scroll position, so a row has no start time to stagger. The difference is
visual only.

## Active tab underline

Before: the design marks the active tab with the secondary fill and the amber
text only. It draws no underline.

After: the active tab also draws a one pixel amber line along its bottom edge.

Reason: the secondary fill sits next to the tertiary fill of the inactive tab,
so the line makes the active tab easier to find. The difference is visual
only.

## Sort pill styling

Before: `design/mockups/theme.css` lines 523-529 give a pill 2 px by 8 px of
padding, the amber dark fill, and the amber text. The design has one pill look
and no inactive state.

After: the active pill takes 10 pt by 3 pt of padding on the tertiary fill with
the amber text and a one point amber border. The inactive pill takes the same
padding on the secondary fill with the secondary text and no border.

Reason: the gallery has two sort modes, so the design needs one pill to read as
active and one as inactive. The larger padding keeps the two pills apart at the
caption size.

## Selection check glyph

Before: the multi-select badge is an 18 pt coral circle with an 11 pt bold `v`
glyph, as `design/mockups/theme.css` line 810 shows.

After: the badge paints the same circle and draws the check with two line
segments. The filter menu tick, a `v` in `design/mockups/filter-tags.html`
line 47, is also a painted check. It is amber when checked. An unchecked row
keeps a blank of the same width.

Reason: the vendored Roboto family has no check glyph, and a text `v` reads as
a letter at the caption size. The difference is visual only.

## Log levels and toolbar

Before: `design/mockups/log-panel.html` shows five levels, with a critical level
in the coral-muted colour. The toolbar Clear button and Debug checkbox use an
11 px font.

After: the Log panel has four levels: debug, info, warn, and error. The critical
level is not used. The Clear button and the Debug checkbox use the default egui
sizes. A long line ends with an ellipsis, while the mockup clips it with no
ellipsis.

Reason: the log source has no critical level, because tracing stops at error.
The default widget sizes keep the toolbar the same as the other egui controls.

## Pink bucket and transparency

Before: there were ten colour buckets, and hues from 310 to 345 degrees fell to
neutral. The frozen mockups show ten swatches in `design/mockups/theme.css`
lines 67-77, and `design/mockups/filter-colors.html` says "Ten buckets". Colour
extraction ignored alpha.

After: there are eleven buckets. `pink` takes a light, soft colour: a hue from
292 up to 360 or from 0 up to 15, saturation 0.60 or less, and value 0.80 or
more. The full rule is in "Colour tag timing and settings". Its token is
`#ff80ab` (`BUCKET_PINK`, which has no stylesheet row). Colour extraction skips
pixels with alpha below 128. An image with 25 percent or more of such pixels is
flagged transparent. The flag becomes the `transparent` tag when the scan
pipeline stores it, which it does not do yet. `transparent` is a colour-filter
value beside the buckets. It follows the same AND rule, the tag cannot be
deleted, and the filter bar draws a checkerboard swatch for it after the eleven
bucket swatches. The mockups stay frozen, so this file records the difference.

Reason: pink hues were wrongly neutral. Transparent pixels added colours that
are not visible. Artists need to find images with transparency.

## Library sidebar layout

Before: `design/mockups/library.html` shows one column. The folder tree and the
FAVORITES section share it, and the favorites follow the tree.

After: the FAVORITES section sits in its own panel above the buttons. The panel
fits its rows, up to half the dock, and scrolls on its own. The folder tree
scrolls above it. Sidebar rows are painted directly. They draw no focus outline
and publish no screen-reader information. The section heads FOLDERS and
FAVORITES use the Roboto Medium face, the nearest vendored weight to the
mockup's 600, with the mockup's 0.04 em letter spacing.

Reason: a tall folder tree would push the favorites out of view, so the
favorites sit in their own panel and stay visible. The mockups define no focus
state for these rows.

## Favorites drag

Before: `design/mockups/favorites-reorder.html` shows the `::` grip
(`.grip`, `design/mockups/theme.css` line 437) only in the reorder state. The
head reads "(drag to reorder)", and a hint line follows the list. The drop line
(`.drop-line`, lines 430-435) takes 8 px of layout: 3 px margins and a 2 px
line. The lifted row (`.fav-row.dragging`, lines 424-428) has a dashed border
that follows the row radius.

After: every favorite row shows the grip. The head reads FAVORITES, and no hint
line is drawn. The drop line overlays the row edge and takes no layout, so rows
stay still during a drag. The dashed border of the lifted row has square
corners.

Reason: the grip is the only drag handle, so it shows on every row. egui 0.36.2
has no helper for a rounded dashed rectangle. A dashed path needs hand-built arc
points, so the border keeps square corners. A permanent grip shows that the
order can change, so the head text and the hint line add nothing.

## Favorite labels and roots

Before: the mockups do not define how a favorite gets its label, and they do
not define the tree roots. `design/mockups/library.html` lines 38-41 show only
sample paths such as `sketches/2026-10`.

After: a favorite shows the stored label when it is not blank. Otherwise it
shows the path below the longest tree root that contains it, for example
`sketches/2026-10`. A favorite equal to a root shows the folder name of that
root. A path below no root, or a root with no folder name such as `/`, shows
the full path. A favorite whose folder is missing shows dim text and a dim
icon. The tree roots are the home folder and, when it is outside home, the
folder given on the command line.

Reason: the sidebar needs a short name for each favorite and a start point for
the tree. The home folder holds a user's files by default, and the
command-line folder must stay reachable when it is outside home.

## Preview empty state and picture box

Before: the mockups show the Preview panel only with a selection and define no
empty state. In `design/mockups/preview-single.html` the picture box grows
with the pane, with a 260 px minimum set inline. The stylesheet default is
140 px.

After: with nothing selected the panel shows the muted hint "Select an image to
preview it" in the middle. The picture box keeps the mockup's growth and fixes
the 260 px minimum in the design tokens. The metadata and the tags sit below
it. A pane that is too short keeps the 260 px box and scrolls. The picture is
scaled to fit inside the box with its aspect ratio kept.

Reason: a blank panel gives no hint about what to do.

## Preview colour squares

Before: `design/mockups/preview-single.html` shows ten squares, one for each
original bucket. The checkerboard has no colour values in the stylesheet.

After: the row has twelve squares: the eleven colour buckets, then a
checkerboard square for `transparent` at the end. The two checkerboard greys
are `#b0b0b0` and `#707070`, which have no stylesheet row. The row is hidden
when colour tags are turned off in the settings. The opacity of a square follows
the mockup: 1.0 when all selected images carry the colour, 0.5 when some do,
and 0.3 when none do.

Reason: the app has eleven buckets and the transparency tag, so the row shows
them all. The row hides when colour tags are off, because its squares would act
on tags that do not exist then.

## Preview tag pills and the add menu

Before: in the mockup the remove x on a tag pill always shows, and a pill has
3 px of vertical padding. The menu has a minimum width of 210 px.

After: the x shows only while the pointer is over the pill. Its room is always
reserved, so the pill does not change size on hover. A pill has 2 px of vertical
padding, which is half of the 4 px spacing step. The "+ add" menu keeps the
tags that only some of the selected images carry, so the user can apply them to
all of them. It leaves out a tag only when every selected image has it. The
210 px minimum width is dropped. The menu is as wide as its widest row, up to
the width of the pane, and a long name ends in an ellipsis. The menu shows at
most eight rows and then scrolls. "Create new..." stays in view below them.

Reason: a permanent x on every pill adds noise, and the mockup has no spacing
token for 3 px. A tag that only some images carry still needs a way to reach the
rest. A fixed wide menu could run past the edge of a narrow pane.

## Preview mosaic

Before: `design/mockups/preview-mosaic.html` shows a grid of cells for a
multi-selection. Each cell has a file-type badge. The mockup shows no caption
under the grid.

After: when more images are selected than the grid shows, a caption such as
"+3 more" sits under the grid. The cells draw no file-type badge yet, because
the Preview has no format data for them. There are no boxes for a picture that
is still loading or for an empty cell yet.

Reason: the caption tells the user that the grid is not the whole selection.
The badge and the loading and empty boxes wait for data the Preview does not
have yet.

## Filter count pill

Before: `design/mockups/info-bar.html` shows a pill that reads "N filters
active". The mockup has no singular form.

After: one filter reads "1 filter active".

Reason: "1 filters active" reads as an error.

## Filter colour row

Before: `design/mockups/filter-colors.html` shows ten swatches. The row is always
there.

After: the row has twelve swatches of the same size: the eleven buckets, then the
checkerboard `transparent` swatch. The outline states match
`design/mockups/theme.css` lines 616-630. The row is hidden when the
`color_tag_enabled` setting is off.

Reason: the app has eleven buckets and the transparency tag. With colour tags
off, the row has nothing to filter.

## Folder labels

Before: `design/mockups/filter-folders.html` shows the last two path parts on
the folder chips only. The mockup does not show a tooltip. No mockup shows the
folder menu rows.

After: the folder menu lists the distinct DB folders as checkable rows. The rows
use the same last two path parts. A chip tooltip shows the full path.

Reason: two folders can share the last two parts. The full path tells them
apart.

## Info text overflow

Before: `design/mockups/info-bar.html` does not say what happens when the
window is too narrow for the info text and the pills.

After: the info text ends in an ellipsis and takes the width that is left. The
filter pill and the sort pills keep their size.

Reason: a pill that shrinks or wraps becomes hard to read and hard to click.

## Filter bar rows

Before: `design/mockups/main-window.html` lines 57-71 put the swatches and the
"Filter Tags" button in one `filterbar` row.

After: the bar stacks three rows below the info bar with 12 pt gaps: colours,
then tags, then folders. The tag button and its chips share the tags row. The
folder button and its chips share the folders row.

Reason: the chips of each kind wrap in their own row. The folder row shows only
in All Images scope, or when it has chips.

## Gallery minimum width

Before: the mockups set no minimum width for the Gallery panel.

After: the Gallery panel is never narrower than 260 pt. The dock keeps this
width when the user drags a separator.

Reason: below about 224 pt the info bar clips on the left. The pills and the
info text need the width.

## All Images scope

Before: the mockups do not say which files All Images lists.

After: All Images shows the whole library, read from the `thumbnails` rows. It
includes files that no longer exist on disk. They stay in the list until a
later rescan step removes their rows. The Python application behaves the same.

Reason: the rows give the library without a scan of every folder. A scan of
100,000 files would be slow.

## Tag manager and tag rules

Before: no mockup shows how to rename or delete a tag.
`design/mockups/tag-edit.html` shows only "Create new..." and its input.

After: a "Manage tags..." row sits at the bottom of the Filter Tags menu, under
a separator. A click closes the menu and opens a small window inside the app
window. The window lists the user tags in name order with their image counts.
Each row has an inline Rename and Delete. "Create new..." stays in the Preview
add menu.

- Name rules: a name is trimmed, must not be empty, and may have at most 64
  characters. Names compare with case, so `Cat` and `cat` are two tags. The
  eleven colour names and `transparent` are refused for create and rename,
  in any case. The manager and the Preview "Create new..." input show the
  reason inline.
- Rename: Enter commits and the first Escape cancels. A name that another tag
  uses is refused with "Name already used". Tags never merge. The tag keeps its
  images, and an active filter chip keeps its place with the new name. A rename
  of a tag that no longer exists drops its row.
- Delete: the row asks "Delete 'x' from N images?" with Delete and Cancel, or
  "Delete 'x'?" when the count is 0. The first Escape cancels the question. Delete removes the tag from every file, and
  its filter chip goes. A failed delete shows a message on its row in place of
  the count.
- Only user tags can be renamed or deleted. The colour tags and `transparent`
  cannot.
- Escape: with no rename field or delete question open, one Escape closes the
  window and any open filter menu together.

Reason: the owner asked for rename and delete. A colour name as a user tag
would clash with the colour tags.

## Preview picture source

Before: the mockups show a picture and say nothing about where it comes from or
how much memory the pictures may use.

After: the picture is decoded from the 1024 tier file. When that file is
missing, the 320 tier file is used. The picture is drawn inside the box with its
aspect ratio kept, and it may be scaled up to touch the box. The pictures of
the selection share a budget of 128 MiB of textures. When a decode fails, the
Preview shows no picture for that file, and the next read of the selection
tries it again. A file that is rendered again at the same size keeps its old
texture until the size changes or the texture is evicted. This is a known limit.

Reason: the 1024 tier is the cache tier made for the preview. The 320 tier is
the grid thumbnail, which is better than an empty box. The budget caps the
texture memory.

## Preview colour square click

Before: the mockup shows the colour squares and does not say what a click does.

After: a click on a colour square toggles that colour tag on all selected
files. When every selected file has the colour, the click removes it from all of
them. Otherwise the click adds it to all of them. The Python application did the
same. The next render of a file replaces its colour tags, so a manual change
does not survive it.

Reason: the squares show the state of the colour tags, so a click that changes
them is the expected action. The render replaces the colour tags because they
come from the picture.

## Colour tag timing and settings

Before: nothing says when the colour tags of a file are written, or when a
change of the colour settings takes effect.

After: the colour tags of a file are written after a render. A file that is
served from the cache is read once for its colours when it has no colour tag
yet. The four colour settings (on or off, palette size, minimum share, and
neutral saturation) are read at each folder open. A change takes effect at the
next open. A colour counts as neutral when its saturation is at or below the
neutral setting, or when its HSV value is 0.08 or less. A bright saturated
colour keeps its hue bucket.

The hue buckets use these ranges in degrees. Each range includes its start and
excludes its end. Red also takes 345 up to 360.

| Bucket  | From | To  |
| ------- | ---- | --- |
| Red     | 0    | 15  |
| Orange  | 15   | 42  |
| Yellow  | 42   | 70  |
| Green   | 70   | 150 |
| Teal    | 150  | 175 |
| Cyan    | 175  | 198 |
| Blue    | 198  | 255 |
| Purple  | 255  | 292 |
| Magenta | 292  | 345 |

Pink is not a hue range. A colour that is not neutral is pink when its hue is
292 up to 360 or 0 up to 15, its saturation is 0.60 or less, and its value is
0.80 or more. A stronger or darker colour with the same hue stays magenta or
red.

Reason: the settings stay fixed while a folder loads. A file cached before
colour tags existed still gets its tags, and it is not rendered again. Each hue
range now holds its pure hue (red 0, orange 30, yellow 60, green 120, cyan 180,
blue 240, purple 270, magenta 300). The earlier ranges put pure blue in purple,
pure green in cyan, and pure yellow in green. A very bright saturated colour is
a real colour, so only low saturation or a very low value makes a colour
neutral.

## Folder open removes rows of files that left

Before: the rows and cache files of a file stayed after the file left its
folder.

After: a folder open deletes the `thumbnails` row and the cache files of each
file that left the folder. It looks at direct children of the folder only. The
parent folder must match exactly, with case. The tags of the file stay, so a file
that comes back keeps its tags. A scan that finds no files deletes nothing,
because an unmounted folder looks empty. A cache file that is outside the cache
folder is never deleted.

Reason: old rows and cache files would grow without limit. Keeping the tags
protects the work of the user when a file is moved away and back.

## Preview format detail

Before: the mockup shows a format in the details of a single image and does not
say where it comes from.

After: the format is the file extension in capitals. A file with no extension
shows the unknown mark.

Reason: the file extension is the only format fact the Preview can read
without opening the file.

## Preferences dialog

Before: the mockup dialog has a close mark in its title bar and four groups of settings. It
has no cap row, shows PNG for the cache format, and has no footnote or busy strip.

After: the window is 520 points wide, with the default title bar and no close mark. Cancel and
Escape close it and discard the input. OK saves all 13 settings in one transaction. The groups
scroll on a short screen, and the footnote and the buttons stay in view. Purge Cache acts at
once and shows a busy strip. Cancel, OK, and Escape wait while it runs. A refused purge shows
`The cache folder is not safe to purge.` and the log has the raw error. The cap row reads
`Cache size cap (GiB, 0 = off)`. Cache Format offers only `jpeg`. Number fields are text. The
registry clamps them on save, and OK stays off while one does not parse. The cache size cap and
the cache folder apply at the next start. Cache format, tile grid size, PSD workers, and large
canvas threshold are saved and no code reads them yet.

Reason: a purge cannot be undone, so it must not wait for OK. A setting with no reader must
say so, or the user expects an effect.

## Error card

Before: the hint reads `Retry: reselect the file or use Regenerate Thumbnail. Check the log
for the full trace.`

After: the hint reads `Retry renders the file again. Check the log for the full trace.` Retry
renders the file again. Open in editor is a ghost button and follows the editor rule in
"Editor templates". The details grid and the tag pills stay below the card.

Reason: the app has no Regenerate Thumbnail command, and a hint must name only what works.

## Crash card

Before: no mockup shows a crash report.

After: a modal window with a dim layer, anchored at the centre, shows the newest unseen crash
report once, at the next start. The colours follow the error card. It shows the report path
and the first eight lines of the report. Dismiss and Escape mark all unseen reports as seen.
Open report uses the editor rule and keeps the card open.

Reason: the user must learn that the last run crashed. Marking all reports seen keeps an
older report from showing at a later start.

## Editor templates

Before: the mockups show no screen to edit editor commands.

After: there is no such screen. The app reads rows of `editor_associations`. The key is the
lower-case extension without a dot, and `*` is the fallback. The extension row wins over `*`.
`{file}` in a template is replaced with the path. Without `{file}`, the path is the last
argument. A missing or blank row is skipped, and with no row left the system opener runs.
Double-click to open and the Regenerate Thumbnail menu are not built.

Reason: the screen is outside the current scope, and a file can still open in a program.

## Open Folder and roots

Before: the mockup menu has Open Folder... but shows no dialog and no saved root.

After: File > Open Folder... opens the native folder dialog on its own thread, so the window
stays responsive. On Linux the dialog needs `xdg-desktop-portal` and a portal backend. A
folder outside the home folder and the saved roots is stored in `library_roots` and stays a
root. The database trims a trailing slash, except for `/` and a bare drive. No screen removes
a root.

Reason: the user must reach any folder, and it must stay in the tree after a restart.

## Empty states

Before: the mockups show one empty card for a folder with no supported file. It lists GIF and
BMP, and it opens with "Tarragon found files". The mockups show no card for a filter with no
result. They also show a status bar line: "No folder open" and "Folder scanned - 0 supported
files".

After: the grid area shows one of three cards when it has no cell. Each card has a folder icon,
a big line, and a hint.

| Card         | When                                            | Big line                           | Button                 |
| ------------ | ----------------------------------------------- | ---------------------------------- | ---------------------- |
| First run    | No filter, and nothing to list                  | Open a folder to begin             | Open Folder...         |
| Empty folder | No filter, and something to list, but no cell   | No supported images in this folder | Open Another Folder... |
| No result    | A search, a chip, or a filter result hides all  | No images match                    | none                   |

- First run hint: `Use File - Open Folder, or click a folder in the Library tree. Tarragon scans
  the folder, writes the index, and renders thumbnails.`
- Empty folder hint: `This folder has no file with a supported extension. Supported: JPG,
  JPEG, PNG, WEBP, TIFF, TIF, PSD, PSB, CLIP, KRA.` The list holds the ten extensions that the
  scanner indexes, not GIF and BMP. A folder with no file at all reads the same hint.
- No result hint: `Change or clear the search and the filters to see more images.` The card has
  no button. It follows the same scope rule as the filter pill: a folder chip counts only in
  the All scope. In the Folder scope, a stale folder chip does not change the card.
- Nothing to list means no open folder in the Folder scope, and no image in the library in the
  All scope. So the All scope with no image shows the first run card.
- The Open Folder buttons do the same as File > Open Folder...
- No card shows while a folder opens, so the empty card does not flash before the first cell.
- The status bar line is not built. The app has no status bar.

Reason: the empty card must name only the formats the scanner indexes. A filter with no result
needs its own text, and a button there would not help.

## Keyboard shortcuts

Before: the mockup menu bar shows no shortcut text and no Quit item. The Python application
had mnemonics only, such as `&File`.

After: four shortcuts exist. The command key is Ctrl on Linux and Windows, and Cmd on macOS.
A chord with an extra Shift or Alt does not act.

| Shortcut | Action                                        | Off when                                       |
| -------- | --------------------------------------------- | ---------------------------------------------- |
| Ctrl+O   | Open Folder, as File > Open Folder...         | A dialog window is open                        |
| Ctrl+,   | Preferences, as Settings > Preferences...     | A dialog window is open                        |
| Ctrl+F   | Focus the search field and select its text    | A dialog window is open                        |
| Ctrl+A   | Select every visible image                    | A dialog window is open, or a text field has focus |

- The windows are the crash card, Preferences, and Manage tags. While one is open, no
  shortcut acts.
- Ctrl+A leaves a focused text field its own select all. Focus on a tile or a button does
  not block it. An image that the search hides is not selected.
- No menu shows these shortcuts, because the mockup shows none. This list is the only place
  that names them.
- Not built: Escape to clear the selection, arrow keys, Enter to open in the editor, Delete,
  Ctrl+Q, and F5 to rescan. The File menu has a Quit item, which the mockup lacks. Each
  other key needs its own decision.
- egui has no mnemonics, so Alt+F is not supported.

Reason: the shortcuts save a trip to the menu. The mockup wins on looks, so the menus stay
free of shortcut text.
