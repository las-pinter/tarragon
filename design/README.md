# Design

This folder holds the approved GUI design of record for Tarragon.

- `mockups/` holds 27 screens (D1-D27) plus `index.html` and `theme.css`.
- Open `mockups/index.html` to browse the screens.
- The mockups use plain HTML and CSS. They work offline and use no JavaScript.

These mockups define the intended design. The application code is the runtime
truth for implemented behavior. When the approved design changes, update the
mockup or record the divergence.

Treat `mockups/` as frozen. Do not restyle it or add screens to it.

`redesign/` holds the flat redesign that the owner approved on 2026-11-26:
six screens and a README with the design rules. It is the one approved
exception to the freeze, and the target look for a restyle of the app. Where
a screen in `redesign/` covers the same view as a screen in `mockups/`, the
redesign wins for look. `mockups/` stays the record for behavior and for the
states that the redesign does not draw. `docs/design-deltas.md` still applies
to both sets: a recorded delta wins over either drawing.
`redesign/canvas.html` is a concept for a planned feature that is not built
yet.
