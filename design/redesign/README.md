# Flat redesign

This folder holds the approved flat redesign of the Tarragon interface. It
keeps the palette of `../mockups/theme.css` and changes how the palette is
used.

Each page is plain HTML and CSS. Each page works offline and uses no
JavaScript. Each page carries all its colors and needs no shared stylesheet.

| Page | Shows |
|------|-------|
| `main-window.html` | Main window with one image selected. |
| `main-multi-select.html` | Main window with six images selected. |
| `palette.html` | Surface, accent, bucket, text, and tag colors, and the type scale. |
| `components.html` | Buttons, tabs, fields, toggles, thumbnail states, context menu, tag editor, empty folder. |
| `settings.html` | Preferences dialog, Color tagging group. |
| `canvas.html` | Concept for the inspiration canvas, a planned feature that is not built yet. |

## Design rules

- The color filter row has twelve swatches: the eleven buckets, pink
  `#ff80ab` included, then a checkerboard `transparent` swatch
  (`docs/design-deltas.md`).
- Flat surfaces. No shadows, no gradients, and no borders between panels.
  Three patterns are not panel styling, so the rule does not cover them: the
  checkerboard swatch, the dot grid behind the canvas board, and the thin
  outline on the palette page's dark swatch chips.
  Depth comes from five surface steps: `#16151a` base, `#1c1b22` panel,
  `#211f29` field, `#2a2836` hover and menu, `#3d3b44` raised.
- Coral marks action and selection. A selected thumbnail has a 2 px
  `#e06540` outline. A selected row has a `#4a1b0c` fill with `#f0997b`
  text. A primary button has a `#e06540` fill with dark `#16151a` text,
  because white text on that coral fails the 4.5:1 contrast bar.
- Amber marks favorites and progress.
- `#8d8a98` is the lowest text color, and only on the base, panel, and field
  surfaces, where it passes 4.5:1. Text on the hover, menu, and raised
  surfaces, and on the `#4a1b0c` selected fill, uses `#a8a5b0` or lighter.
  Disabled text is exempt from the contrast bar. `#65626f` is not used for
  text.
- Section labels are 11 px, semibold, upper case, with wide letter spacing.
- Corner radii: 6 px for rows, 8 px for buttons, fields, and thumbnails, and
  10 px to 12 px for panels, menus, and dialogs.
- The preview panel gives most of its height to the image. Name, size,
  format, and date share one line under the image, beside the Open button.
- With several images selected, the preview mosaic fills the full panel
  height. Two images use one column, three to six use two columns, and seven
  to nine use three columns. Rows stretch to fill the height. When the last
  row is not full, the first cells span two columns so that no slot stays
  empty.

## Fonts

The pages name Roboto and Roboto Mono, the faces the app can ship under
Apache-2.0. A machine without them shows a system font. The handwriting on
`canvas.html` uses the generic `cursive` family; the canvas design has not
chosen a face yet.

## Sample content

Thumbnails are flat shapes in the bucket colors. They stand in for real
images. File names, counts, and sizes are sample data.
