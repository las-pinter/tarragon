"""Backward-compatibility shim — moved to tarragon.services.color_tagger.

All public names are re-exported from the canonical location.
"""

from tarragon.services.color_tagger import extract_dominant_color_tags

__all__ = ["extract_dominant_color_tags"]
