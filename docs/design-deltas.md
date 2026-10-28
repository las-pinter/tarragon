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
request, and Open Folder is a placeholder until the folder picker lands.

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

After: plain egui panels draw the same regions at the same default sizes. The
Log panel spans the gallery width and stays hidden on the first frame. No panel
can move or resize yet.

Reason: no docking library is in the tree. The docking work owns the layout
later and replaces these panels.

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
segments.

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

After: there are eleven buckets. `pink` takes hues from 310 up to, but not
including, 345, with the token `#ff80ab` (`BUCKET_PINK`, which has no
stylesheet row). Colour extraction skips pixels with alpha below 128. An image
with 25 percent or more of such pixels is flagged transparent. The flag becomes
the `transparent` tag when the scan pipeline stores it, which it does not do
yet. `transparent` is a colour-filter value beside the buckets. It follows the
same AND rule, the tag cannot be deleted, and the filter bar will use a
checkerboard swatch for it, which is not drawn yet. The mockups stay frozen, so
this file records the difference.

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
