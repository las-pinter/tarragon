# Tarragon Color Tagging System

## Overview

Tarragon includes a deterministic dominant color tagging system that analyzes rendered images and assigns color-based tags automatically. The algorithm uses pure math — no machine learning, no randomization — making it fully reproducible for any given input image.

Color tags are extracted after a successful thumbnail render and persisted to the database with the `auto_color` source, distinguishing them from user-assigned tags.

## Algorithm

The color tagging algorithm is implemented in `color_tagger.py` → `extract_dominant_color_tags()`.

### Step-by-Step

1. **Convert to RGB** — The input PIL Image is converted to RGB mode, discarding any alpha channel. This ensures consistent color analysis regardless of the source format.

2. **Downsample to 64x64** — The image is resized to 64x64 pixels using Lanczos resampling. This reduces the pixel count to 4,096, making the subsequent quantization fast while preserving the overall color distribution.

3. **Quantize** — The 64x64 image is quantized to `palette_size` colors (default 8) using the **Median Cut** algorithm (`Image.Quantize.MEDIANCUT`). This produces a reduced color palette that represents the dominant colors in the image.

4. **Count pixels per color** — The quantized image is converted back to RGB, and `getcolors()` returns a list of `(count, (r, g, b))` tuples. The total pixel count is the sum of all counts.

5. **Convert to HSV** — Each palette color is converted from RGB (0–255) to HSV:
   - **H** (Hue): 0–360 degrees
   - **S** (Saturation): 0.0–1.0
   - **V** (Value/Brightness): 0.0–1.0

6. **Classify into color buckets** — Each color is assigned to a bucket based on its HSV values (see bucket definitions below).

7. **Accumulate shares** — The pixel count for each bucket is divided by the total pixel count to get a share (0.0–1.0). Multiple palette colors may map to the same bucket, and their shares are summed.

8. **Filter by minimum share** — Only buckets with a share >= `min_share` (default 0.10, i.e., 10%) produce a tag.

9. **Format and sort** — Tags are formatted as `color:<bucket_name>` and sorted alphabetically.

### Example Output

```python
["color:blue", "color:green", "color:neutral"]
```

## Color Buckets

The hue spectrum (0–360) is divided into 10 chromatic buckets plus a neutral catch-all:

| Bucket | Hue Range | Notes |
|--------|-----------|-------|
| red | 0–15 | Low end of the hue wheel |
| red | 345–360 | Wrap-around for red (high end) |
| orange | 15–33 | Between red and yellow |
| yellow | 33–50 | Warm bright colors |
| green | 50–85 | Standard green range |
| teal | 85–105 | Blue-green transition |
| cyan | 105–140 | Cool blue-green |
| blue | 140–200 | Standard blue range |
| purple | 200–270 | Blue-red transition |
| magenta | 270–310 | Red-purple range |
| neutral | (see below) | Low saturation, very bright, or very dark |

**Gap region**: Hues between 310–345 that don't fall into any chromatic bucket are classified as neutral.

## Neutral Detection

A color is classified as **neutral** (grayscale, white, black) when any of these conditions are met:

| Condition | Threshold | Rationale |
|-----------|-----------|-----------|
| Low saturation | `S <= neutral_s_threshold` (default 0.15) | Desaturated colors appear gray |
| Very bright | `V >= 0.92` | Near-white colors lack chromatic identity |
| Very dark | `V <= 0.08` | Near-black colors lack chromatic identity |

The `neutral_s_threshold` is configurable via settings (default 0.15, range 0.0–1.0).

## HSV Conversion

The RGB-to-HSV conversion is implemented in `_rgb_to_hsv()` and operates on normalized RGB values (0.0–1.0):

```python
def _rgb_to_hsv(r: float, g: float, b: float) -> tuple[float, float, float]:
    c_max = max(r, g, b)
    c_min = min(r, g, b)
    delta = c_max - c_min

    # Hue (0-360)
    if delta == 0:
        h = 0.0
    elif c_max == r:
        h = (60.0 * ((g - b) / delta)) % 360
    elif c_max == g:
        h = (60.0 * ((b - r) / delta) + 120.0) % 360
    else:  # c_max == b
        h = (60.0 * ((r - g) / delta) + 240.0) % 360

    # Saturation (0.0-1.0)
    s = 0.0 if c_max == 0 else (delta / c_max)

    # Value (0.0-1.0)
    v = c_max

    return h, s, v
```

## Configurable Parameters

All parameters are stored in the `settings` database table and accessed via the `Settings` class:

| Setting | Default | Range | Description |
|---------|---------|-------|-------------|
| `color_tag_enabled` | `True` | bool | Master switch for automatic color tagging |
| `color_tag_palette_size` | `8` | 2–32 | Number of colors in the quantized palette |
| `color_tag_min_share` | `0.10` | 0.0–1.0 | Minimum pixel share for a color to produce a tag |
| `color_tag_neutral_s_threshold` | `0.15` | 0.0–1.0 | Saturation below which a color is classified as neutral |

### Parameter Effects

- **palette_size**: Higher values produce more granular color detection but may fragment dominant colors into similar buckets. Lower values produce broader, more general tags.
- **min_share**: Higher values require a color to occupy a larger portion of the image before generating a tag. Lower values produce more tags but may include minor accent colors.
- **neutral_s_threshold**: Higher values classify more colors as neutral (fewer color tags). Lower values are more aggressive at assigning chromatic buckets.

## Database Integration

Color tags are persisted in the `file_tags` table with `source = 'auto_color'`:

1. After a successful render, `ThumbnailService._on_done()` calls `extract_dominant_color_tags()`.
2. The returned tag list (e.g., `["color:blue", "color:neutral"]`) is passed to `Database.replace_auto_color_tags()`.
3. This method first **deletes** all existing `auto_color` tags for the file, then **inserts** the new ones.
4. Tag names are ensured in the `tags` table via `INSERT OR IGNORE` / `ON CONFLICT`.

This replace-all approach ensures that re-rendering an updated file produces fresh color tags without accumulating stale entries.

## Query Integration

Color tags participate in the gallery filtering system via `QueryService`:

- The `ColorFilterBar` widget provides clickable color swatches.
- Active color swatches produce a set of color tag names (e.g., `{"color:red", "color:blue"}`).
- `QueryService.query()` applies color tag filters with **OR semantics** — a file matches if it has **any** of the selected color tags.
- This is combined with other filters (folder scope, filename search, manual tags) via AND semantics.

## Performance

The color tagging algorithm is fast because it operates on the already-rendered master image (max 2048px long edge) rather than the original file:

- Downsample to 64x64: negligible (4096 pixels).
- Median cut quantization: ~1–5ms for 4096 pixels.
- HSV conversion and bucket mapping: ~1–2ms for 8 palette colors.
- Total overhead per image: ~5–20ms.
