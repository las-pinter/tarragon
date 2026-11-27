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

## Grid cache tier

Before: `design/mockups/settings-cache.html` listed the cache tiers as 256,
1024, and full. `design/mockups/log-panel.html` showed the log line
`tier 256 hit for sketch_014.png`.

After: the first tier is 320 px on the longest edge. The settings text reads
`Cache tiers: 320, 1024, full.` and the log line reads
`tier 320 hit for sketch_014.png`. The app no longer reads old 256 folders.

Reason: a 160 pt thumbnail at a display scale of 2.0 needs 320 px. The grid
tracks start at 168 pt and stretch to fill the row, so at a high display scale
or in a narrow pane a thumbnail can be drawn larger than its 320 px tier.

## Outside references in the design folder

Before: `design/README.md` pointed to an archive of alternative designs and to
a separate design-alternatives document, both outside the repository. The
comment block in `design/mockups/theme.css` named the source files of the
previous implementation. Two captions in `design/mockups/menubar.html` read
"The Python File menu" and "The Python Settings menu".

After: the files name no source outside the repository. The palette values are
unchanged. The captions read "The File menu" and "The Settings menu".

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

Roboto has no CJK or symbol glyphs. The mockup stack used system fonts, so it
could show them. The app keeps Roboto as the first face and adds system fonts
as the last choice. The window opens with Roboto. A thread then scans the
system font folders, and the fonts apply after one repaint. Each font joins
all four families: Proportional, Monospace, Roboto Medium, and Roboto Bold. A
fallback face draws only a char that Roboto and the faces before it lack.

The app keeps up to 3 CJK faces and up to 2 symbol faces. It matches a family
name exactly, in this order, and keeps one face for each font file. The list
below is a copy. `crates/tarragon-app/src/sysfonts.rs` owns the list.

- Linux CJK: Noto Sans CJK SC, Noto Sans CJK JP, Noto Sans CJK KR, Source Han
  Sans SC, WenQuanYi Micro Hei, WenQuanYi Zen Hei, Droid Sans Fallback.
- Linux symbols: Noto Sans Symbols, Noto Sans Symbols 2, Noto Sans Symbols2,
  DejaVu Sans.
- Windows CJK: Microsoft YaHei, Yu Gothic, Malgun Gothic. Symbols: Segoe UI
  Symbol.
- macOS CJK: PingFang SC, Hiragino Sans, Apple SD Gothic Neo. Symbols: Apple
  Symbols.
- Other systems: no names, so no fallback.

The Windows and macOS names were not checked on real machines.

The CJK order is the same for every locale: SC, then JP, then KR. The app does
not read the locale. A Han character that the three languages share shows the
Simplified Chinese form. A Noto Sans CJK `.ttc` file holds all three
languages. Only the SC face is kept, so the JP and KR forms never show. A
machine with none of the listed CJK fonts shows boxes for CJK text.

Reason: Segoe UI is a system font and no license permits shipping it. Roboto is
embedded in the crate under Apache-2.0. Roboto cannot draw CJK text. The OFL
license of the usual CJK fonts is off the license allowlist, so no CJK font
ships with the app. System fonts fill the gap.

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

After: the frame draws the wordmark in the top bar at the redesign's title size,
gives the panel titles the extra-small vertical padding and the Roboto Medium
face, and paints no separator borders.

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

## Grid tile

Before: `design/mockups/theme.css` line 765 scales a hovered cell by 1.02 over
150 ms, and a cell is a bordered card. `design/redesign/main-window.html` draws
no card: a square thumbnail with an 8 px radius, the name under it, a 2 px
`#e06540` selection outline 3 px outside the square, a hover outline in
`#3d3b44`, and a bottom bar with the pixel size and the file size.

After: the tile is the redesign tile. The picture is fitted inside the square
on the field fill and never cropped, so a wide or tall picture leaves bands of
the field fill. Only the picture corners that meet a corner of the square are
round. The name sits 8 pt under the square in the tertiary text, and a selected
name takes the light coral and the Roboto Medium face. Hover shows the outline
and the info bar at once, with no scale, fade, or shadow. The bar reads
`W × H · size`, and only the file size when the pixel size is not known yet.
The pixel size comes from the stored row when the file has not changed since,
or from a new render. The format badge sits 8 pt inside the top-left corner on
the base fill with the format text colour. The grid tracks are at least 168 pt
wide, with 16 pt between columns and 20 pt between rows, and the grid keeps a
5 pt edge so an outline at the side is not cut off. The name stays at the
12 pt body size.

Reason: a scaled tile overlaps its neighbours. A cropped picture would hide
part of a reference image. A file that changed since its row was written may
have a new pixel size, so its old one is not shown.

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
`design/redesign/main-multi-select.html` draws a 22 px `#e06540` circle with a
dark check.

After: the badge is the redesign's 22 pt circle, 8 pt inside the top-right
corner of the square, with the check painted as a dark line. The filter menu
tick, a `v` in `design/mockups/filter-tags.html` line 47, is also a painted
check. It is amber when checked. An unchecked row keeps a blank of the same
width.

Reason: the vendored Roboto family has no check glyph, and a text `v` reads as
a letter at the caption size. The difference is visual only.

## Selection frame

Before: `design/mockups/theme.css` drew a selected tile with two frames: the
1.5 px coral border of `.cell.selected`, and a second 1 px coral outline on the
thumbnail (`.cell.selected .thumb`, with an offset of -1 px).

After: a selected tile draws one frame, the redesign's 2 pt outline 3 pt
outside the square (see "Grid tile").

Reason: the redesign draws one outline, and a second stroke doubled the frame.

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

Before: `design/redesign/main-window.html` shows one column. The FOLDERS list,
the FAVORITES list, and the "Add current folder" action share it, and the
favorites follow the tree.

After: the FAVORITES head, its rows, and "Add current folder" sit in their own
panel at the bottom of the sidebar. The panel fits its rows, up to half the
dock, and scrolls on its own. The folder tree scrolls above it. Rows, heads,
carets, folder outlines, stars, and the add action follow the page's sizes and
colours. Sidebar rows are painted directly. They draw no focus outline and
publish no screen-reader information. The section heads use the Roboto Medium
face, the nearest vendored weight to the page's 600. The row text stays at the
12 pt body size where the page gives 13 px. A tree row shows its folder's
image count at the right, as the page does; a folder with no images directly
in it, and a folder that cannot be listed, show none. Favorites show no
count, as on the page.

Reason: a tall folder tree would push the favorites out of view, so the
favorites sit in their own panel and stay visible. The page defines no focus
state for these rows. The body size is the app-wide 12 pt.

## Favorites drag

Before: `design/redesign/main-window.html` shows the favorites at rest only: no
grip, no reorder state, and no remove control.

After: the whole row is the drag handle. A press that moves past egui's drag
threshold starts a drag, and a press that does not is a click that opens the
folder. The cursor is a closed hand during a drag. The lifted row follows the
pointer, held inside the list, and the other rows slide to the places they take
if the row is dropped there; the gap they leave is the drop target, so no drop
line is drawn. Inside the favorites area, a pointer above or below the list
means the first or the last place. On release, Escape, or a drop outside the
favorites area, the rows slide to their final places. Rows also slide up when a
favorite above them is removed. The lifted row has a dashed amber border with
square corners. A right click on any favorite opens a menu with Remove; no
Remove button is drawn, so Remove has no keyboard path, like the rest of the
favorite rows.

Reason: the page draws no handle, so the row itself takes the drag. egui 0.36.2
has no helper for a rounded dashed rectangle. A dashed path needs hand-built arc
points, so the border keeps square corners.

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
140 px. `design/redesign/main-window.html` draws the picture on a deep
`#0d0c10` stage with a 10 px radius, 12 px of padding, and a 560 px minimum
height.

After: with nothing selected the panel shows the muted hint "Select an image to
preview it" in the middle. The picture sits on the redesign's deep stage, with
its radius and padding, and takes the height the details leave. The picture
has 4 pt corners and is scaled to fit inside the stage with its aspect ratio
kept. The stage keeps the 260 pt minimum of the first mockups, not the page's
560 px. A pane too short for the stage and the details scrolls.

Reason: a blank panel gives no hint about what to do. A 560 pt minimum would
push the name and the open button out of view on a laptop screen.

## Preview colour bars

Before: `design/mockups/preview-single.html` shows ten squares, one for each
original bucket, and a click on a square toggled that colour tag.
`design/redesign/main-window.html` shows small rounded bars of the image's own
colours, each as wide as its share of the picture.

After: the row shows one 18 by 8 pt bar for each colour the selection carries,
in the order of the filter swatches. All bars have one width, because the app
stores no colour share. A colour that only some of the selected images carry
is drawn at half opacity. The `transparent` tag draws a checkerboard bar in
`#b0b0b0` and `#707070`. The bars sit at the start of the tag row. They are
not buttons: a click does nothing. The row has no bars when colour tags are
turned off in the settings.

Reason: the share of each colour is not stored, and the page's bars show
colours, not controls. A colour tag set by hand did not survive the next
render, which writes the colour tags from the picture again.

## Preview tag pills and the add menu

Before: in the mockup the remove x on a tag pill always shows, and a pill has
3 px of vertical padding. The menu has a minimum width of 210 px.
`design/redesign/main-window.html` gives each tag a colour pair of the palette
and ends the row with a "+ Tag" pill, but says nothing about how a tag gets
its colour.

After: a pill takes one of the seven tag colour pairs of `palette.html`
(vinous, sage, navy, umber, plum, teal, olive), picked from the tag name, so
a tag keeps its colour across selections and runs. Pills have the page's
padding and radius and use the Roboto Medium face. The x shows only while the
pointer is over the pill. Its room is always reserved, so the pill does not
change size on hover. The "+ Tag" pill follows the last tag on the same row.
Its menu keeps the tags that only some of the selected images carry, so the
user can apply them to all of them. It leaves out a tag only when every
selected image has it. The 210 px minimum width is dropped. The menu is as
wide as its widest row, up to the width of the pane, and a long name ends in
an ellipsis. The menu shows at most eight rows and then scrolls.
"Create new..." stays in view below them.

Reason: the page shows colours but no rule, and a colour that changed with
every selection would carry no meaning. A permanent x on every pill adds
noise. A tag that only some images carry still needs a way to reach the rest.
A fixed wide menu could run past the edge of a narrow pane.

## Preview mosaic

Before: `design/redesign/main-multi-select.html` and the redesign README fill
the whole Preview height with the mosaic: one column for two images, two for
three to six, three for seven to nine, rows that stretch to the height, and
first cells two columns wide when the last row is not full. Each cell is
filled edge to edge with its picture and carries the format badge. Under it,
"6 selected" over "18.6 MB in all" sits beside an "Edit tags" button and an
"Open all in Krita" button. The page shows no tag row for a multi-selection
and gives no rule past nine images.

After: the mosaic follows the README rule and takes the height the footer
leaves, with the 260 pt minimum of the single picture. Past nine images the
column count is the square root rounded up, so 10 to 16 images take four
columns; the `max_multi_preview` setting still caps the cells. Each top row
that has a free slot starts with a cell two columns wide, so every row is
full. A cell is the field fill with an 8 pt radius, the picture is fitted
whole inside it, never cropped, and the grid's format badge sits in the top
left. The footer follows the page. The size line adds " · +3 more" when the
cap hides some of the selection. The open button names the app when every
selected file opens in the same named app, and reads "Open all" otherwise, or
when the name would take more than 40 percent of the row; it opens the files
as "Open N in editor" does. "Edit tags" shows the tag row (the
colour bars, the pills, and "+ Tag") under the footer and hides it again, and
the row stays open across selections until it is hidden. There are no boxes
for a picture that is still loading.

Reason: a cropped cell would hide part of a reference image. The page gives
no rule past nine, and the square root keeps the cells near square. The tag
row is the only place that removes a tag from many images at once, so it
stays one click away.

## Filter count pill

Before: `design/mockups/info-bar.html` shows a pill that reads "N filters
active". The mockup has no singular form.

After: one filter reads "1 filter active".

Reason: "1 filters active" reads as an error.

## Filter colour row

Before: `design/mockups/filter-colors.html` shows ten swatches. The row is always
there. `design/redesign/main-window.html` draws twelve round 24 px swatches,
with a 2 px light ring 2 px outside a held swatch, and no hover or focus state.

After: the row has the redesign's twelve round swatches: the eleven buckets,
then the checkerboard `transparent` swatch, cut from 4 by 4 cells with the
top-left cell dark, as the page's pattern draws it. A held swatch takes the
light ring. A hovered swatch takes the same ring in the raised surface colour,
and a swatch with the keyboard focus takes it in coral, which wins over the
held ring. The row is hidden when the `color_tag_enabled` setting is off.

Reason: the app has eleven buckets and the transparency tag. With colour tags
off, the row has nothing to filter. The page defines no hover or focus state,
and a swatch can be reached with the keyboard, so it needs both.

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

## Filter bar row

Before: `design/mockups/main-window.html` lines 57-71 put the swatches and the
"Filter Tags" button in one `filterbar` row. `design/redesign/main-window.html`
draws one wrapping row on the panel fill: the swatches, a divider, a chip for
each held filter, then the "Tags" and "Clear" text actions. The chip area grows,
so the actions sit at the right end. "Clear" always shows, and no row reaches
the folder filter.

After: the bar is the redesign row, with its padding, gaps, chips, divider, and
radius. The chips list the held colours in swatch order, then the tags and the
folders in the order they were added. A colour chip shows the small swatch, a
tag chip a coral dot, and a folder chip an amber dot. In the All images scope a
coral "Folders" action sits between "Tags" and "Clear" and opens the folder
menu. A held folder keeps its chip in the Folder scope. "Clear" shows only while
the bar holds a filter, and it keeps the search text, which the search field
owns. The actions sit at the right of the first line when the bar is wide enough
for them beside a full line of swatches. A narrower bar wraps the swatches and
chips, and the actions take a line of their own below them, at its right end.
The divider shows only between swatches and chips. A chip name too wide for one
line ends in an ellipsis, and its tooltip shows the whole name. The chip x and
the text actions take the keyboard focus and show the coral focus ring. The
action and chip text stays at the 12 pt body size where the page gives 13 px
and 12 px.

Reason: the folder filter needs a way in, and the page's text action is the
smallest one that matches it. A "Clear" with nothing to clear does nothing. In a
narrow gallery, actions kept beside the swatches would squeeze them into many
short lines.

## Gallery minimum width

Before: the mockups set no minimum width for the Gallery panel.

After: the Gallery panel is never narrower than 260 pt. The dock keeps this
width when the user drags a separator.

Reason: below about 224 pt the info row clips on the left. The pills and the
selection text need the width.

## All images scope

Before: the mockups do not say which files All images lists.

After: All images shows the whole library, read from the `thumbnails` rows. It
includes files that no longer exist on disk. They stay in the list until a
later rescan step removes their rows. The Python application behaves the same.

Reason: the rows give the library without a scan of every folder. A scan of
100,000 files would be slow.

## Tag manager and tag rules

Before: no mockup shows how to rename or delete a tag.
`design/mockups/tag-edit.html` shows only "Create new..." and its input.

After: a "Manage tags..." row sits at the bottom of the Tags menu, under
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

## Preview details and open button

Before: the mockups show a Metadata block of label and value rows, and give a
format without saying where it comes from. `design/redesign/main-window.html`
puts the name over one line of "pixel size · file size · format · date" beside an
"Open in Krita" button and an arrow button.

After: the name uses the title size in Roboto Medium and ends in an ellipsis.
The line under it reads `W × H · size · FORMAT · date`, and a value that is not
known is left out. The format is the file extension in capitals, or
`Unknown`. The date is the modified time of the file, as stored with its row
or read from the disk, in the time zone the app had at start, as
`Mon D, YYYY`. The coral button opens the file:

- With an editor row for the extension, or for `*`, it opens with that
  template, and the button names the program with a capital first letter,
  such as "Open in Krita".
- With no row, it opens with the system opener. On Linux the button names
  the default app from its desktop entry, looked up in the background, such
  as "Open in GIMP". The button reads "Open" until the name is known, when no
  name is found, after an `xdg-mime` query passes its deadline, and on Windows
  and macOS.

The arrow button lists the extension row, the `*` row when it differs, and
the default app, by name when known and as "Default app" otherwise. A row
opens the file with that app.

Reason: the file extension is the only format fact the Preview can read
without opening the file. The app stores no display name for an editor, so the
program name and the desktop entry are the names it has. A session that
crosses a change of daylight saving time keeps the start offset, as the log
times do.

## Busy strip

Before: the mockups `scanning-busy.html` and `busy-states.html` show a strip with
`Working...` for a scan, a purge, and a decode. It is hidden when idle. The
mockup stylesheet lists it between the info bar and the filters.

After: the gallery shows the strip while a folder or the library opens, and while
a purge waits or runs. It stays until the last one ends. It is the last block of
the top panel, below the filter bar, so only the grid moves down by the strip
height while it shows. The filter buttons stay still. The strip takes no space when
idle. The cache clean-up at start does not show it. The Preferences purge strip
uses the same painter. No decode strip exists. The strip ends when the file list is
ready, not when the tiles render. Each tile shows its own loading cell. While a
folder opens, the gallery header names that folder. The count still belongs to the files
on screen, so a switch from one folder to another shows the new name with the old
count, and a first open shows 0. The mockup shows the count of the new folder. The
All images scope keeps its own name. When an open fails, the header shows the old folder again.

Reason: one state machine runs all the thumbnail work, so one flag is enough. Below
the filter bar, the strip does not move the controls under the pointer. A decode
strip would need a count of tile renders, and the tile cells show that already.

## Preferences dialog

Before: the mockup dialog has a close mark in its title bar and four groups of settings. It
has no cap row, shows PNG for the cache format, and has no footnote.

After: the window is 520 points wide, with the default title bar and no close mark. Cancel and
Escape close it and discard the input. OK saves all 13 settings in one transaction. The groups
scroll on a short screen, and the footnote and the buttons stay in view. Purge Cache acts at
once and shows a busy strip in the dialog with the label `Purging cache...`. The gallery strip
shows too (see "Busy strip"). Cancel, OK, and Escape wait while it runs. A refused purge shows
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

After: there is no such screen. The user picks the app in the desktop settings. The app
reads rows of `editor_associations`. The key is the lower-case extension without a dot,
and `*` is the fallback. The extension row wins over `*`. `{file}` in a template is
replaced with the path. Without `{file}`, the path is the last argument. A missing or blank row is skipped, and with no row left the system opener runs.
A double-click on a tile selects that image only and opens it by these rules. A failed start
logs a warning and does not retry. A right-click on a tile opens the grid context menu, which
has its own entry below.

Many files: the menu item Open N in editor opens the selected files with as few process
starts as the apps allow. One selected file opens by the rules above, with no default-app
lookup. For more files, each file gets a route, and files with the same route share a start.

- A template with `{file}` stays one start per file.
- A template with `{files}` is one start for all its files. `{files}` must be an argument
  of its own, and the template must not contain `{file}`. The paths replace `{files}` in
  the order of the selection.
- A file that has no editor row for its extension or `*` uses the default app of the
  desktop. A user's own editor row wins over the default app.
- The type comes from `xdg-mime query filetype`. The app comes from
  `xdg-mime query default`, and the app starts from the `Exec` line of its desktop entry.
  The lookup runs once for each lower-case extension, not once for each file.
- An `Exec` line with `%F` or `%U` is one start for all the files of that app. A line with
  `%f` or `%u` is one start per file.
- One start takes at most 500 paths and 100 000 bytes of path text. More files go into
  more starts. A path is never split.
- A path that appears twice is opened once.
- A file with no extension and no `*` row skips the lookup and keeps the system opener,
  one start per file.
- The system opener (`xdg-open`) stays in use for a file when `xdg-mime` is missing, fails,
  or gives no type or no default app; the desktop entry is missing or unusable (for
  example it is unreadable or too large, is not an application, has `Hidden=true`, has a
  `TryExec` program that is not found, or has no `Exec` line); the entry has
  `Terminal=true`; the `Exec` line has no file code; or an `xdg-mime` query runs past its
  2 s deadline. The timeout is logged. After one timeout, no more queries run, so the wait
  is one deadline at most.
- A failed start counts every file in it as failed. The other starts still run. The app
  shows one notice with the counts.
- On Windows and macOS there is no default-app lookup. A file with no editor row uses the
  system opener, one start per file. Editor rows work as above.

Reason: the app has no editor screen, and a file can still open in a program. One
start for many files keeps a large selection from opening many windows of one app.

## Grid context menu

Before: the mockup shows one menu on a tile, with one item, Regenerate Thumbnail. No
mockup shows the other items, the menu on empty space, or a notice line.

After: a right-click on a tile opens a menu at the pointer. A right-click on an
unselected tile selects only that tile first. A right-click on a selected tile keeps
the selection. The menu has four groups. A line separates the groups. Shortcut text is
drawn dim at the right.

| Group | Item (one file)       | Shortcut  | Many files selected           | Greyed when        |
| ----- | --------------------- | --------- | ----------------------------- | ------------------ |
| 1     | Open in editor        | Enter     | Open N in editor              |                    |
| 1     | Show in file manager  |           |                               | more than one file |
| 2     | Regenerate Thumbnail  |           | Regenerate N Thumbnails       |                    |
| 2     | Add tag...            |           |                               |                    |
| 2     | Copy path             |           |                               | more than one file |
| 3     | Copy                  | Ctrl+C    |                               |                    |
| 3     | Paste                 | Ctrl+V    |                               | the All scope      |
| 3     | Duplicate             |           |                               |                    |
| 3     | Rename...             | F2        |                               | more than one file |
| 4     | Delete                | Del       | Delete N items                |                    |
| 4     | Delete permanently... | Shift+Del | Delete N items permanently... |                    |

- Escape or a press outside the menu closes it with no action. A change of the
  selected files under the open menu closes it too.
- Right-click on empty space in the grid, or on the empty folder card, opens a menu
  with one item, Paste. It opens only while a folder is open. Paste is greyed in the
  All scope.
- Show in file manager opens the folder of the file through the system opener
  (`xdg-open` on Linux). Copy path puts the file path on the clipboard as text.
- Add tag... opens, at the pointer, the same menu as the Preview "+ Tag" button: the
  known tags and Create new.... The chosen tag goes to every selected file.
- Regenerate Thumbnail queues a new render of each selected file.
- Rename... opens a dialog with the name without its extension. The extension shows
  beside the field as locked text and cannot change. The dialog refuses an empty name,
  a name with a slash, a backslash or a NUL character, `.` or `..`, a name that starts
  or ends with a space or a dot, a whole name over 255 bytes, and a name that another
  file already has. A rename moves the file on disk and updates its row in the library.
- Duplicate copies each file next to itself under the first free copy name, such as
  `a copy.png`, then `a copy 2.png`. It never overwrites a file. In the All scope each
  copy goes into the folder of its source file.
- Copy puts the files on the clipboard as a `text/uri-list`. Paste reads that list and
  copies each regular file of a supported type into the open folder, under a free
  name. It skips other entries and counts them in the notice. Paste does nothing in
  the All scope. Copy and Paste work on Linux, Windows, and macOS. On Linux the
  copied list lasts while Tarragon runs, unless a clipboard manager keeps it.
  On Linux a pasted entry whose name is not valid text, or one from another
  host, is left out of the list.
- Delete and Delete permanently... always ask first. Delete asks to move the files to
  the Trash, and its default button, Move to Trash, confirms. Delete permanently...
  asks for confirmation and warns that it cannot be undone. Its default button is
  Cancel, and Enter never confirms it. If the Trash refuses a file, a second prompt
  offers to delete that file permanently instead. Its default button is Cancel too.
  After a delete, the tile nearest to the removed place is selected.
- The notice line is one line of text at the foot of the gallery, above the grid. It
  shows the result of a write, a failure, or a busy message for six seconds. A new
  message replaces the old one. No mockup shows it.
- Ctrl+Z undoes the last Rename or Duplicate of the session, one step only. A new
  Rename or Duplicate replaces the step. Paste and every delete clear it. A finished
  undo clears the step. A refused Rename undo also clears it. A failed undo keeps it.
  Nothing redoes an undo. Undoing a Duplicate moves only the copies that are unchanged
  to the Trash. It keeps a copy that was edited or is gone. Undoing a Rename moves the
  file back, unless it is gone or another file has the old name. Ctrl+Z does nothing
  while a text field has focus or a dialog is open.

Reason: the mockup shows the menu with one item. Managing files in place needs the other
items. The notice line shows the result of work that has no window of its own.

## Keyboard shortcuts for the grid

Before: the mockup menu shows no shortcut text for these actions.

After: F2, Del, Shift+Del, Ctrl+C, Ctrl+V, and Ctrl+Z do what the menu items of the same
name do, under the rules in "Grid context menu". They act only when no widget other
than a tile has focus and no dialog or menu is open. Del, Shift+Del, and Ctrl+C need a
selected file. F2 needs exactly one. Ctrl+V needs one open folder. Ctrl+Z needs an
undo step.

Reason: the menu shows these keys, so they must work.

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

After: six shortcuts exist. The command key is Ctrl on Linux and Windows, and Cmd on macOS.
A chord with an extra Shift or Alt does not act.

| Shortcut | Action                                        | Off when                                       |
| -------- | --------------------------------------------- | ---------------------------------------------- |
| Ctrl+O   | Open Folder, as File > Open Folder...         | A dialog window is open                        |
| Ctrl+,   | Preferences, as Settings > Preferences...     | A dialog window is open                        |
| Ctrl+F   | Focus the search field and select its text    | A dialog window is open                        |
| Ctrl+K   | The same as Ctrl+F                            | A dialog window is open                        |
| Ctrl+A   | Select every visible image                    | A dialog window is open, or a text field has focus |
| Enter    | Open the selected image in its editor, when exactly one image is selected | A dialog window is open, or a widget other than a tile has focus |

- The windows are the crash card, Preferences, and Manage tags. While one is open, no
  shortcut acts.
- Ctrl+K is the key the redesign top bar prints in its search field, so both keys act.
- Ctrl+A leaves a focused text field its own select all. Focus on a tile or a button does
  not block it. An image that the search hides is not selected.
- No menu bar menu shows these shortcuts, because the mockup shows none. This list is the
  only place that names them. The grid context menu does show its own keys, see "Grid
  context menu".
- Enter needs no modifier. With none or several images selected, it does nothing and is not
  consumed. A focused button or text field keeps its own Enter.
- Not built: Escape to clear the selection, arrow keys, Ctrl+Q, and F5 to rescan. The File menu has a Quit item, which the mockup lacks. Each
  other key needs its own decision. The grid keys are in "Keyboard shortcuts for the grid".
- egui has no mnemonics, so Alt+F is not supported.

Reason: the shortcuts save a trip to the menu. The mockup wins on looks, so the menus stay
free of shortcut text.
