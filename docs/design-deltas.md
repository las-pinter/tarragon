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
