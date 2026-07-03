# Tarragon Styling Architecture Report
**Date:** 2026-01-29  
**Researcher:** Snaggit da Kommando  
**For:** Wrenchbasha

---

## Executive Summary

The tarragon codebase has a **partially implemented theme system** with significant duplication and inconsistency. While a design token system exists (tokens.json), it's only used by 2 out of 10 widgets. Most styling is hardcoded in QSS files and inline setStyleSheet() calls, making the UI difficult to maintain and theme consistently.

---

## 1. Styling Locations Inventory

### 1.1 QSS Files
**Location:** `/home/dev/tarragon/src/tarragon/theme/app.qss` (311 lines)

**What it styles:**
- Base window backgrounds (QMainWindow)
- Dock widgets (QDockWidget)
- Buttons (QPushButton)
- Labels (QLabel)
- Tag pills (QLabel with tagRole property)
- Line edits (QLineEdit)
- Scroll bars (QScrollBar)
- Toolbars (QToolBar)
- Status bar (QStatusBar)
- List/Tree views (QListView, QTreeView)
- Group boxes (QGroupBox)
- Checkboxes/Radio buttons (QCheckBox, QRadioButton)
- Combo boxes (QComboBox)
- Scroll areas (QScrollArea)
- Log panel (QPlainTextEdit#logText)

**Issue:** All values are **hardcoded hex colors and pixel values** — does NOT reference tokens.json.

### 1.2 Inline setStyleSheet() Calls
Found in **6 widget files** with **15 total calls**:

| File | Line | What it styles |
|------|------|----------------|
| `tag_panel.py` | 254 | Color swatch background/border |
| `tag_panel.py` | 258 | Auto-color tag row border/padding |
| `tag_filter_bar.py` | 164 | Tag chip frame (background, border, padding) |
| `tag_filter_bar.py` | 173 | Tag chip label color |
| `tag_filter_bar.py` | 179 | Tag chip remove button (color, hover) |
| `preview_panel.py` | 116 | Image label (background, border, color) |
| `preview_panel.py` | 129 | Filename label (color, weight) |
| `preview_panel.py` | 134 | Dimensions label (color) |
| `preview_panel.py` | 138 | Size label (color) |
| `preview_panel.py` | 142 | Format label (color) |
| `preview_panel.py` | 147 | Panel background |
| `color_filter_bar.py` | 132 | Swatch button (background, border) |

**Issue:** Most use **hardcoded hex colors** instead of tokens.

### 1.3 QPalette Usage
**Location:** `/home/dev/tarragon/src/tarragon/main.py` (lines 103-134)

**What it does:**
- Sets dark palette for Fusion style
- Defines 20+ color roles (Window, WindowText, Base, Text, Button, Highlight, etc.)
- Includes disabled state colors
- All values are **hardcoded hex colors**

**Issue:** Duplicates colors from tokens.json without referencing them.

### 1.4 Hardcoded Colors in Widget Code
Found in **5 files** with **62 total hex color references**:

| File | Colors defined | Purpose |
|------|----------------|---------|
| `thumbnail_grid.py` | 7 colors (lines 22-28) | Theme tokens as QColor constants |
| `tag_filter_bar.py` | 4 colors (lines 165-188) | Chip styling |
| `tag_panel.py` | 11 colors (lines 284-295) | Color bucket map |
| `log_panel.py` | 6 colors (lines 30-37) | Log level colors |
| `color_filter_bar.py` | 12 colors (lines 16-43) | Swatch colors + borders |

**Issue:** Colors duplicated across files without central source.

---

## 2. Theme System Analysis

### 2.1 Current Structure
```
src/tarragon/theme/
├── __init__.py          # Exports ThemeLoader
├── tokens.json          # Design tokens (50 lines)
├── tokens.py            # Token loading functions (45 lines)
├── loader.py            # ThemeLoader class (54 lines)
└── app.qss              # QSS stylesheet (311 lines)
```

### 2.2 tokens.json Contents
**Sections:**
- **colors** (10 tokens): bg_primary, bg_secondary, bg_tertiary, surface_highlight, coral_strong, coral_muted, amber_accent, text_primary, text_secondary, text_tertiary
- **typography** (7 tokens): font_family, body_size, heading_size, small_size, weight_regular, weight_medium, weight_semibold
- **spacing** (5 tokens): xs=4, sm=8, md=12, lg=16, xl=24
- **radius** (4 tokens): none=0, sm=6, md=8, lg=8
- **motion** (3 tokens): duration_fast=150, duration_normal=250, easing="ease-out"
- **layout** (7 tokens): thumbnail_size=160, grid_gap=8, dock_width_min=180, preview_height_min=300, sidebar_width_px=220, preview_panel_width_px=280, multi_preview_max_default=9

### 2.3 Token Loading API
**File:** `tokens.py`

```python
def load_tokens() -> dict[str, Any]:
    """Load all tokens from tokens.json"""
    
def get_token(section: str, key: str) -> Any:
    """Get a single token value by section and key"""
```

### 2.4 How Themes Are Applied
**File:** `main_window.py` (lines 551-560)

```python
def _apply_theme(self) -> None:
    """Load and apply the QSS stylesheet from theme/app.qss."""
    from tarragon.theme.loader import ThemeLoader
    
    loader = ThemeLoader()
    qss_content = loader.load_qss()
    app = QApplication.instance()
    if isinstance(app, QApplication):
        app.setStyleSheet(qss_content)
```

**Flow:**
1. `main.py` sets QPalette with hardcoded colors (lines 103-134)
2. `main_window.py` calls `_apply_theme()` in constructor (line 86)
3. `_apply_theme()` loads `app.qss` via ThemeLoader
4. QSS is applied globally via `app.setStyleSheet()`

**Critical Issue:** `app.qss` contains **hardcoded values**, not token references. The token system exists but is **not connected** to the QSS.

### 2.5 Token Usage in Widgets
**Only 2 widgets use tokens:**

1. **preview_panel.py** (lines 22-27):
   ```python
   from tarragon.theme.tokens import get_token
   
   BG_PRIMARY: str = get_token("colors", "bg_primary")
   BG_SECONDARY: str = get_token("colors", "bg_secondary")
   TEXT_PRIMARY: str = get_token("colors", "text_primary")
   TEXT_SECONDARY: str = get_token("colors", "text_secondary")
   CORAL_STRONG: str = get_token("colors", "coral_strong")
   ```

2. **thumbnail_grid.py** (lines 22-29):
   - Defines its own QColor constants (NOT using get_token)
   - Values match tokens.json but are duplicated

**8 widgets do NOT use tokens:**
- tag_panel.py
- tag_filter_bar.py
- color_filter_bar.py
- sidebar.py
- settings_dialog.py
- log_panel.py
- gallery_tabs.py
- main_window.py

---

## 3. Padding/Margin Situation

### 3.1 Current Padding Values (setContentsMargins)

| Widget | Margins | Spacing | Notes |
|--------|---------|---------|-------|
| `preview_panel.py` | (8, 8, 8, 8) | 8 | Uses tokens for colors |
| `settings_dialog.py` | (12, 12, 12, 12) | 10 | Hardcoded |
| `tag_panel.py` | (0, 0, 0, 0) | - | Zero margins |
| `tag_filter_bar.py` | (0, 0, 0, 0) | 4 | Zero margins |
| `color_filter_bar.py` | (4, 4, 4, 4) | 4 | Hardcoded |
| `log_panel.py` | (4, 4, 4, 4) | 4 | Hardcoded |
| `sidebar.py` | (0, 0, 0, 0) | - | Zero margins |
| `gallery_tabs.py` | (0, 0, 0, 0) | - | Zero margins |

### 3.2 Padding in QSS (app.qss)

| Selector | Padding | Notes |
|----------|---------|-------|
| `QDockWidget::title` | `padding-left: 8px; padding-right: 8px;` | Hardcoded |
| `QPushButton` | `padding-left: 12px; padding-right: 12px; padding-top: 6px; padding-bottom: 6px;` | Hardcoded |
| `QLabel[tagRole="primary"]` | `padding: 3px 8px;` | Hardcoded |
| `QLabel[tagRole="secondary"]` | `padding: 3px 8px;` | Hardcoded |
| `QLineEdit` | `padding-left: 8px; padding-right: 8px; padding-top: 4px; padding-bottom: 4px;` | Hardcoded |
| `QToolBar` | `padding: 4px;` | Hardcoded |
| `QGroupBox` | `margin-top: 12px; padding-top: 16px;` | Hardcoded |
| `QPlainTextEdit#logText` | `padding: 4px;` | Hardcoded |

### 3.3 Padding in Inline Styles

| Widget | Element | Padding | Notes |
|--------|---------|---------|-------|
| `tag_panel.py:258` | Auto-color row | `padding: 2px;` | Hardcoded |
| `tag_filter_bar.py:165` | Tag chip | `padding: 2px 6px;` | Hardcoded |
| `tag_filter_bar.py:169` | Chip layout | `setContentsMargins(6, 2, 6, 2)` | Hardcoded |

### 3.4 Inconsistency Examples

**Example 1: Spacing values**
- `preview_panel.py`: 8px margins, 8px spacing
- `settings_dialog.py`: 12px margins, 10px spacing
- `tag_filter_bar.py`: 0px margins, 4px spacing
- `color_filter_bar.py`: 4px margins, 4px spacing
- `log_panel.py`: 4px margins, 4px spacing

**Example 2: Tag chip padding**
- QSS `QLabel[tagRole="primary"]`: `padding: 3px 8px;`
- Inline `tag_filter_bar.py` chip: `padding: 2px 6px;`
- Chip layout margins: `setContentsMargins(6, 2, 6, 2)`

**Result:** Visually inconsistent spacing across the UI.

---

## 4. Styling Duplication Analysis

### 4.1 Color Duplication

**tokens.json defines:**
```json
"bg_primary": "#16151A",
"bg_secondary": "#1c1b22",
"coral_strong": "#F0997B",
"amber_accent": "#FAC775",
"text_primary": "#ece9f2",
"text_secondary": "#A09CA3"
```

**Duplicated in:**

1. **main.py QPalette** (lines 104-133):
   ```python
   palette.setColor(QPalette.ColorRole.Window, QColor("#16151A"))  # bg_primary
   palette.setColor(QPalette.ColorRole.Base, QColor("#1c1b22"))    # bg_secondary
   palette.setColor(QPalette.ColorRole.BrightText, QColor("#F0997B"))  # coral_strong
   palette.setColor(QPalette.ColorRole.Link, QColor("#FAC775"))    # amber_accent
   palette.setColor(QPalette.ColorRole.WindowText, QColor("#ece9f2"))  # text_primary
   ```

2. **thumbnail_grid.py** (lines 22-28):
   ```python
   CORAL_STRONG = QColor("#F0997B")
   AMBER_ACCENT = QColor("#FAC775")
   BG_PRIMARY = QColor("#16151A")
   BG_SECONDARY = QColor("#1c1b22")
   TEXT_PRIMARY = QColor("#ece9f2")
   TEXT_SECONDARY = QColor("#A09CA3")
   ```

3. **app.qss** (throughout):
   ```css
   QMainWindow { background-color: #16151A; }
   QDockWidget { background-color: #1c1b22; color: #ece9f2; }
   QPushButton:hover { color: #FAC775; }
   ```

4. **Inline styles** in preview_panel.py, tag_filter_bar.py, etc.

**Total:** Same 10 colors defined in **4+ places**.

### 4.2 Color Map Duplication

**Color bucket map** (for color tags) defined in **2 files**:

1. **tag_panel.py** (lines 283-294):
   ```python
   color_map: dict[str, str] = {
       "red": "#E74C3C",
       "orange": "#F39C12",
       "yellow": "#F1C40F",
       # ... 10 colors total
   }
   ```

2. **color_filter_bar.py** (lines 33-44):
   ```python
   BUCKET_HUES: dict[str, str] = {
       "red": "#E74C3C",
       "orange": "#F39C12",
       "yellow": "#F1C40F",
       # ... 10 colors total (identical)
   }
   ```

**Issue:** If a color needs to change, must update in **2 places**.

### 4.3 Spacing Duplication

**tokens.json defines:**
```json
"spacing": {
  "xs": 4,
  "sm": 8,
  "md": 12,
  "lg": 16,
  "xl": 24
}
```

**Hardcoded spacing values found:**
- `4px`: tag_filter_bar.py, color_filter_bar.py, log_panel.py, app.qss (QToolBar)
- `8px`: preview_panel.py, settings_dialog.py, app.qss (QPushButton, QLineEdit, QDockWidget)
- `12px`: settings_dialog.py, app.qss (QGroupBox)
- `6px`: tag_filter_bar.py (chip margins)
- `10px`: settings_dialog.py (spacing)

**Issue:** Spacing tokens exist but are **never used**. Values hardcoded everywhere.

### 4.4 Typography Duplication

**tokens.json defines:**
```json
"typography": {
  "font_family": "Segoe UI, -apple-system, sans-serif",
  "body_size": 12,
  "heading_size": 16,
  "small_size": 10
}
```

**Hardcoded in app.qss:**
```css
QDockWidget { font-family: "Segoe UI", -apple-system, sans-serif; font-size: 12px; }
QPushButton { font-family: "Segoe UI", -apple-system, sans-serif; font-size: 12px; }
QLabel { font-family: "Segoe UI", -apple-system, sans-serif; font-size: 12px; }
QLineEdit { font-family: "Segoe UI", -apple-system, sans-serif; font-size: 12px; }
```

**Issue:** Font family and size repeated **10+ times** in QSS.

---

## 5. Recommendations for Cleaner Architecture

### 5.1 Connect QSS to Token System

**Problem:** `app.qss` has hardcoded values, not token references.

**Solution:** Generate QSS from tokens at runtime.

**Implementation:**
```python
# theme/qss_generator.py
def generate_qss(tokens: dict) -> str:
    """Generate QSS with token values substituted."""
    colors = tokens["colors"]
    spacing = tokens["spacing"]
    typography = tokens["typography"]
    
    qss_template = """
    QMainWindow {{
        background-color: {bg_primary};
    }}
    
    QPushButton {{
        background-color: {bg_tertiary};
        color: {text_primary};
        font-family: {font_family};
        font-size: {body_size}px;
        padding: {spacing_sm}px {spacing_md}px;
    }}
    """
    
    return qss_template.format(
        bg_primary=colors["bg_primary"],
        bg_tertiary=colors["bg_tertiary"],
        text_primary=colors["text_primary"],
        font_family=typography["font_family"],
        body_size=typography["body_size"],
        spacing_sm=spacing["sm"],
        spacing_md=spacing["md"],
        # ... etc
    )
```

**Benefits:**
- Single source of truth (tokens.json)
- Easy theme changes (edit tokens, not QSS)
- Consistent values across UI

### 5.2 Centralize Color Constants

**Problem:** Colors defined in tokens.json, main.py, thumbnail_grid.py, and inline styles.

**Solution:** Create a `theme/colors.py` module that exports QColor objects.

**Implementation:**
```python
# theme/colors.py
from PySide6.QtGui import QColor
from tarragon.theme.tokens import get_token

def get_color(name: str) -> QColor:
    """Get a QColor for a color token name."""
    hex_value = get_token("colors", name)
    return QColor(hex_value)

# Pre-defined constants for convenience
BG_PRIMARY = get_color("bg_primary")
BG_SECONDARY = get_color("bg_secondary")
CORAL_STRONG = get_color("coral_strong")
AMBER_ACCENT = get_color("amber_accent")
TEXT_PRIMARY = get_color("text_primary")
TEXT_SECONDARY = get_color("text_secondary")
```

**Usage in widgets:**
```python
from tarragon.theme.colors import BG_PRIMARY, TEXT_PRIMARY

label.setStyleSheet(f"color: {TEXT_PRIMARY.name()};")
```

### 5.3 Create Spacing Utilities

**Problem:** Spacing values hardcoded everywhere.

**Solution:** Create spacing constants from tokens.

**Implementation:**
```python
# theme/spacing.py
from tarragon.theme.tokens import get_token

# Spacing constants
SPACING_XS = get_token("spacing", "xs")  # 4
SPACING_SM = get_token("spacing", "sm")  # 8
SPACING_MD = get_token("spacing", "md")  # 12
SPACING_LG = get_token("spacing", "lg")  # 16
SPACING_XL = get_token("spacing", "xl")  # 24
```

**Usage in widgets:**
```python
from tarragon.theme.spacing import SPACING_SM, SPACING_MD

layout.setContentsMargins(SPACING_SM, SPACING_SM, SPACING_SM, SPACING_SM)
layout.setSpacing(SPACING_MD)
```

### 5.4 Consolidate Color Bucket Map

**Problem:** Color bucket map duplicated in tag_panel.py and color_filter_bar.py.

**Solution:** Move to a shared module.

**Implementation:**
```python
# theme/color_buckets.py
COLOR_BUCKETS = {
    "red": "#E74C3C",
    "orange": "#F39C12",
    "yellow": "#F1C40F",
    "green": "#27AE60",
    "teal": "#1ABC9C",
    "cyan": "#00BCD4",
    "blue": "#3498DB",
    "purple": "#9B59B6",
    "magenta": "#E91E63",
    "neutral": "#7F8C8D",
}

# Canonical order (for sorting)
COLOR_ORDER = [
    "red", "orange", "yellow", "green", "teal",
    "cyan", "blue", "purple", "magenta", "neutral",
]
```

**Usage:**
```python
from tarragon.theme.color_buckets import COLOR_BUCKETS, COLOR_ORDER
```

### 5.5 Standardize Padding/Margin Approach

**Problem:** Mixed use of QSS padding and Python setContentsMargins().

**Recommendation:**
- Use **QSS for widget-internal padding** (button text padding, label padding)
- Use **setContentsMargins() for layout spacing** (margins around widgets)
- Always use **token-based spacing constants** (SPACING_SM, SPACING_MD, etc.)

**Example:**
```python
# Good: Use tokens for layout margins
layout.setContentsMargins(SPACING_SM, SPACING_SM, SPACING_SM, SPACING_SM)
layout.setSpacing(SPACING_MD)

# QSS handles internal padding (from generated QSS)
# QPushButton { padding: 6px 12px; }
```

### 5.6 Refactor QPalette to Use Tokens

**Problem:** main.py QPalette has hardcoded colors.

**Solution:** Load from tokens.

**Implementation:**
```python
# main.py
from tarragon.theme.tokens import load_tokens
from PySide6.QtGui import QColor, QPalette

def create_palette_from_tokens() -> QPalette:
    """Create QPalette from design tokens."""
    tokens = load_tokens()
    colors = tokens["colors"]
    
    palette = QPalette()
    palette.setColor(QPalette.ColorRole.Window, QColor(colors["bg_primary"]))
    palette.setColor(QPalette.ColorRole.WindowText, QColor(colors["text_primary"]))
    palette.setColor(QPalette.ColorRole.Base, QColor(colors["bg_secondary"]))
    palette.setColor(QPalette.ColorRole.Text, QColor(colors["text_primary"]))
    palette.setColor(QPalette.ColorRole.Button, QColor(colors["bg_tertiary"]))
    palette.setColor(QPalette.ColorRole.ButtonText, QColor(colors["text_primary"]))
    palette.setColor(QPalette.ColorRole.BrightText, QColor(colors["coral_strong"]))
    palette.setColor(QPalette.ColorRole.Highlight, QColor(colors["coral_muted"]))
    palette.setColor(QPalette.ColorRole.HighlightedText, QColor(colors["text_primary"]))
    palette.setColor(QPalette.ColorRole.Link, QColor(colors["amber_accent"]))
    
    # Disabled state
    disabled_text = QColor(colors["text_tertiary"])
    palette.setColor(QPalette.ColorGroup.Disabled, QPalette.ColorRole.WindowText, disabled_text)
    palette.setColor(QPalette.ColorGroup.Disabled, QPalette.ColorRole.Text, disabled_text)
    palette.setColor(QPalette.ColorGroup.Disabled, QPalette.ColorRole.ButtonText, disabled_text)
    
    return palette

# In main():
app.setPalette(create_palette_from_tokens())
```

---

## 6. Implementation Priority

### Phase 1: Quick Wins (Low Risk)
1. **Create `theme/colors.py`** with QColor constants from tokens
2. **Create `theme/spacing.py`** with spacing constants from tokens
3. **Create `theme/color_buckets.py`** with consolidated color bucket map
4. **Update thumbnail_grid.py** to use `theme.colors` instead of local constants
5. **Update tag_panel.py and color_filter_bar.py** to use `theme.color_buckets`

### Phase 2: QSS Generation (Medium Risk)
6. **Create `theme/qss_generator.py`** to generate QSS from tokens
7. **Update `theme/loader.py`** to use generator instead of static app.qss
8. **Test thoroughly** — QSS generation must match current visual appearance

### Phase 3: Widget Refactoring (Medium Risk)
9. **Update preview_panel.py** to use `theme.colors` instead of get_token() calls
10. **Update all widgets** to use `theme.spacing` constants for margins/spacing
11. **Remove inline setStyleSheet()** where QSS can handle it

### Phase 4: QPalette Integration (Low Risk)
12. **Update main.py** to use `create_palette_from_tokens()`

---

## 7. Summary of Issues

| Issue | Severity | Impact |
|-------|----------|--------|
| QSS not connected to tokens | **High** | Can't theme via tokens.json |
| Colors duplicated in 4+ places | **High** | Hard to maintain consistency |
| Spacing tokens never used | **Medium** | Inconsistent padding/margins |
| Color bucket map duplicated | **Medium** | Must update in 2 places |
| Inline styles override QSS | **Medium** | Hard to track styling |
| Typography hardcoded 10+ times | **Low** | Repetitive but works |

---

## 8. Files Requiring Changes

**New files to create:**
- `theme/colors.py` — QColor constants
- `theme/spacing.py` — Spacing constants
- `theme/color_buckets.py` — Color bucket map
- `theme/qss_generator.py` — QSS generation from tokens

**Files to update:**
- `theme/loader.py` — Use QSS generator
- `main.py` — Use token-based palette
- `main_window.py` — No changes needed (already uses ThemeLoader)
- `widgets/preview_panel.py` — Use theme.colors
- `widgets/thumbnail_grid.py` — Use theme.colors
- `widgets/tag_panel.py` — Use theme.color_buckets, theme.spacing
- `widgets/tag_filter_bar.py` — Use theme.colors, theme.spacing
- `widgets/color_filter_bar.py` — Use theme.color_buckets, theme.spacing
- `widgets/settings_dialog.py` — Use theme.spacing
- `widgets/log_panel.py` — Use theme.colors, theme.spacing
- `widgets/sidebar.py` — Use theme.spacing
- `widgets/gallery_tabs.py` — Use theme.spacing

**Files to delete:**
- `theme/app.qss` — Replace with generated QSS (or keep as fallback)

---

## 9. Conclusion

The tarragon codebase has a **solid foundation** (tokens.json, ThemeLoader, get_token API) but it's **not fully utilized**. The main issues are:

1. **QSS is static** — doesn't reference tokens
2. **Colors duplicated** — defined in 4+ places
3. **Spacing inconsistent** — tokens exist but aren't used
4. **Inline styles scattered** — hard to track and maintain

**Recommended approach:**
- **Phase 1** (1-2 days): Create color/spacing/bucket modules, update 3-4 widgets
- **Phase 2** (2-3 days): Implement QSS generation, test thoroughly
- **Phase 3** (2-3 days): Refactor remaining widgets
- **Phase 4** (1 day): Update QPalette

**Total effort:** ~1-1.5 weeks for a complete, maintainable styling system.

**Benefits:**
- Single source of truth (tokens.json)
- Easy theme changes (edit tokens, not code)
- Consistent UI across all widgets
- Easier to add new themes/light mode in future

---

**Report compiled by:** Snaggit da Kommando  
**Date:** 2026-01-29  
**Status:** Ready for Wrenchbasha review
