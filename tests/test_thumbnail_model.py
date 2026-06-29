"""Tests for ThumbnailModel — non-GUI unit tests for model logic."""

from __future__ import annotations

from pathlib import Path

import pytest
from PySide6.QtCore import QModelIndex, Qt
from tarragon.models.thumbnail_model import ThumbnailModel


class TestThumbnailModel:
    """ThumbnailModel unit tests — no QApplication required."""

    def test_empty_model_row_count(self) -> None:
        """A freshly-constructed model has rowCount() == 0."""
        model = ThumbnailModel()
        assert model.rowCount() == 0

    def test_no_parent_instantiation(self) -> None:
        """Model can be instantiated without a parent argument."""
        model = ThumbnailModel()
        assert model is not None
        assert model.rowCount() == 0

    def test_set_paths_updates_row_count(self) -> None:
        """set_paths() with 3 paths gives rowCount() == 3."""
        model = ThumbnailModel()
        paths = [Path("/a/one.jpg"), Path("/b/two.png"), Path("/c/three.gif")]
        model.set_paths(paths)
        assert model.rowCount() == 3

    def test_set_paths_replaces_old_paths(self) -> None:
        """set_paths() replaces existing paths (length changes correctly)."""
        model = ThumbnailModel()
        model.set_paths([Path("/old/a.png")])
        assert model.rowCount() == 1

        model.set_paths([Path("/new/b.jpg"), Path("/new/c.jpg")])
        assert model.rowCount() == 2

    def test_display_role_returns_basename(self) -> None:
        """data() with DisplayRole returns the file basename."""
        model = ThumbnailModel()
        model.set_paths([Path("/some/long/path/image.jpg")])

        index = model.index(0, 0)
        result = model.data(index, Qt.DisplayRole)

        assert result == "image.jpg"

    def test_path_role_returns_full_path_string(self) -> None:
        """data() with PathRole returns the full path as a string."""
        model = ThumbnailModel()
        model.set_paths([Path("/some/long/path/image.jpg")])

        index = model.index(0, 0)
        result = model.data(index, ThumbnailModel.PathRole)

        assert result == "/some/long/path/image.jpg"
        assert isinstance(result, str)

    def test_invalid_index_returns_none(self) -> None:
        """data() returns None for an invalid index."""
        model = ThumbnailModel()
        model.set_paths([Path("/test.png")])

        invalid_index = model.index(999, 0)
        assert model.data(invalid_index, Qt.DisplayRole) is None
        assert model.data(invalid_index, ThumbnailModel.PathRole) is None

    def test_path_role_constant_value(self) -> None:
        """PathRole constant equals Qt.UserRole + 1."""
        assert ThumbnailModel.PathRole == Qt.UserRole + 1

    # ------------------------------------------------------------------
    # Edge case tests — Gutslicka da Painboy's special brew
    # ------------------------------------------------------------------

    def test_set_paths_with_empty_list_resets_to_zero(self) -> None:
        """set_paths([]) results in rowCount() == 0."""
        model = ThumbnailModel()
        model.set_paths([Path("/a/one.jpg")])
        model.set_paths([])
        assert model.rowCount() == 0

    def test_set_paths_with_none_raises_type_error(self) -> None:
        """set_paths(None) raises TypeError because list(None) fails."""
        model = ThumbnailModel()
        with pytest.raises(TypeError):
            model.set_paths(None)  # type: ignore[arg-type]

    def test_data_with_negative_row_returns_none(self) -> None:
        """data() returns None when index row is negative."""
        model = ThumbnailModel()
        model.set_paths([Path("/test.png")])

        index = model.index(-1, 0)
        assert model.data(index, Qt.DisplayRole) is None
        assert model.data(index, ThumbnailModel.PathRole) is None

    def test_data_with_row_equal_to_count_returns_none(self) -> None:
        """data() returns None when index row == rowCount (boundary)."""
        model = ThumbnailModel()
        model.set_paths([Path("/test.png")])

        # rowCount is 1, so index(1, 0) is out of bounds
        index = model.index(1, 0)
        assert model.data(index, Qt.DisplayRole) is None
        assert model.data(index, ThumbnailModel.PathRole) is None

    def test_data_with_empty_model_returns_none(self) -> None:
        """data() returns None on an empty model (no set_paths call)."""
        model = ThumbnailModel()
        index = model.index(0, 0)
        assert model.data(index, Qt.DisplayRole) is None
        assert model.data(index, ThumbnailModel.PathRole) is None

    def test_data_with_unicode_path_returns_correct_values(self) -> None:
        """data() handles Unicode file names and paths correctly."""
        model = ThumbnailModel()
        model.set_paths([Path("/data/图片/照片.jpg")])

        index = model.index(0, 0)
        assert model.data(index, Qt.DisplayRole) == "照片.jpg"
        assert model.data(index, ThumbnailModel.PathRole) == "/data/图片/照片.jpg"
        assert isinstance(model.data(index, ThumbnailModel.PathRole), str)

    def test_data_with_special_characters_in_path(self) -> None:
        """data() handles spaces and special characters in paths."""
        model = ThumbnailModel()
        model.set_paths([Path("/data/file with spaces (1).png")])

        index = model.index(0, 0)
        assert model.data(index, Qt.DisplayRole) == "file with spaces (1).png"
        assert model.data(index, ThumbnailModel.PathRole) == "/data/file with spaces (1).png"

    def test_data_with_deeply_nested_path(self) -> None:
        """data() handles deeply nested paths (50+ levels deep)."""
        model = ThumbnailModel()
        deep_path = Path(*["a"] * 50) / "image.jpg"
        model.set_paths([deep_path])

        index = model.index(0, 0)
        assert model.data(index, Qt.DisplayRole) == "image.jpg"
        assert model.data(index, ThumbnailModel.PathRole) == str(deep_path)

    def test_data_with_unsupported_roles_returns_none(self) -> None:
        """data() returns None for all roles other than DisplayRole and PathRole."""
        model = ThumbnailModel()
        model.set_paths([Path("/test/file.jpg")])
        index = model.index(0, 0)

        unsupported_roles = [
            Qt.UserRole,
            Qt.ToolTipRole,
            Qt.StatusTipRole,
            Qt.WhatsThisRole,
            Qt.DecorationRole,
            Qt.SizeHintRole,
            Qt.FontRole,
            Qt.TextAlignmentRole,
            Qt.BackgroundRole,
            Qt.ForegroundRole,
            Qt.CheckStateRole,
        ]
        for role in unsupported_roles:
            assert model.data(index, role) is None, f"Expected None for role {role}"

    def test_row_count_ignores_parent_index(self) -> None:
        """rowCount() returns same result regardless of parent index argument."""
        model = ThumbnailModel()
        model.set_paths([Path("/a.jpg"), Path("/b.jpg")])
        valid_index = model.index(0, 0)
        default_index = QModelIndex()

        assert model.rowCount() == 2
        assert model.rowCount(valid_index) == 2
        assert model.rowCount(default_index) == 2

    def test_multiple_rapid_set_paths_calls(self) -> None:
        """Multiple rapid set_paths() calls leave model in consistent state."""
        model = ThumbnailModel()

        for i in range(50):
            model.set_paths([Path(f"/path/file_{i}.jpg")])

        assert model.rowCount() == 1
        index = model.index(0, 0)
        assert model.data(index, Qt.DisplayRole) == "file_49.jpg"
        assert model.data(index, ThumbnailModel.PathRole) == "/path/file_49.jpg"

    # ------------------------------------------------------------------
    # Multi-resolution thumbnail role tests
    # ------------------------------------------------------------------

    def test_set_thumbnail_with_resolution_256(self) -> None:
        """set_thumbnail with resolution=256 stores path in ThumbnailRole256 only."""
        model = ThumbnailModel()
        model.set_paths([Path("/test/image.jpg")])

        cache_path = Path("/cache/256/test_abc/image.png")
        model.set_thumbnail("/test/image.jpg", cache_path, resolution=256)

        index = model.index(0, 0)
        assert model.data(index, ThumbnailModel.ThumbnailRole256) == str(cache_path)
        assert model.data(index, ThumbnailModel.ThumbnailRole1024) == ""
        assert model.data(index, ThumbnailModel.ThumbnailRoleFull) == ""

    def test_set_thumbnail_multiple_resolutions(self) -> None:
        """set_thumbnail stores paths independently for each resolution tier."""
        model = ThumbnailModel()
        model.set_paths([Path("/test/image.jpg")])

        model.set_thumbnail("/test/image.jpg", Path("/cache/256/test/image.png"), resolution=256)
        model.set_thumbnail("/test/image.jpg", Path("/cache/1024/test/image.png"), resolution=1024)
        model.set_thumbnail("/test/image.jpg", Path("/cache/full/test/image.png"), resolution=None)

        index = model.index(0, 0)
        assert model.data(index, ThumbnailModel.ThumbnailRole256) == "/cache/256/test/image.png"
        assert model.data(index, ThumbnailModel.ThumbnailRole1024) == "/cache/1024/test/image.png"
        assert model.data(index, ThumbnailModel.ThumbnailRoleFull) == "/cache/full/test/image.png"

    # ------------------------------------------------------------------
    # Bug 3 regression: set_paths preserves cached thumbnails
    # ------------------------------------------------------------------

    def test_set_paths_preserves_thumbnails_for_remaining_paths(self) -> None:
        """set_paths() keeps cached thumbnails for paths still in the list."""
        model = ThumbnailModel()
        model.set_paths([Path("/a/one.jpg"), Path("/b/two.jpg"), Path("/c/three.jpg")])

        # Set thumbnails for all three
        model.set_thumbnail("/a/one.jpg", Path("/cache/256/one.png"), resolution=256)
        model.set_thumbnail("/b/two.jpg", Path("/cache/256/two.png"), resolution=256)
        model.set_thumbnail("/c/three.jpg", Path("/cache/256/three.png"), resolution=256)

        # Now filter to just two paths (removing /c/three.jpg)
        model.set_paths([Path("/a/one.jpg"), Path("/b/two.jpg")])

        # Thumbnails for remaining paths should be preserved
        idx_a = model.index(0, 0)
        idx_b = model.index(1, 0)
        assert model.data(idx_a, ThumbnailModel.ThumbnailRole256) == "/cache/256/one.png"
        assert model.data(idx_b, ThumbnailModel.ThumbnailRole256) == "/cache/256/two.png"

    def test_set_paths_preserves_thumbnails_for_removed_paths(self) -> None:
        """set_paths() keeps cached thumbnails even for paths no longer in the list.

        Regression test: previously set_paths() pruned _thumbnails for paths
        not in the new list, causing thumbnails to disappear when a color
        filter was removed and the full path list restored.
        """
        model = ThumbnailModel()
        model.set_paths([Path("/a/one.jpg"), Path("/b/two.jpg")])
        model.set_thumbnail("/a/one.jpg", Path("/cache/256/one.png"), resolution=256)
        model.set_thumbnail("/b/two.jpg", Path("/cache/256/two.png"), resolution=256)

        # Replace with completely different paths
        model.set_paths([Path("/c/three.jpg")])

        # Internal dict should still contain old entries (not pruned)
        assert "/a/one.jpg" in model._thumbnails
        assert "/b/two.jpg" in model._thumbnails
        assert model.rowCount() == 1

    def test_set_paths_with_same_paths_preserves_all_thumbnails(self) -> None:
        """Calling set_paths with the same paths preserves all cached thumbnails."""
        model = ThumbnailModel()
        paths = [Path("/a/one.jpg"), Path("/b/two.jpg")]
        model.set_paths(paths)

        model.set_thumbnail("/a/one.jpg", Path("/cache/256/one.png"), resolution=256)
        model.set_thumbnail("/b/two.jpg", Path("/cache/1024/two.png"), resolution=1024)

        # Re-set the same paths (e.g. after a no-op filter)
        model.set_paths(paths)

        idx_a = model.index(0, 0)
        idx_b = model.index(1, 0)
        assert model.data(idx_a, ThumbnailModel.ThumbnailRole256) == "/cache/256/one.png"
        assert model.data(idx_b, ThumbnailModel.ThumbnailRole1024) == "/cache/1024/two.png"

    def test_set_paths_empty_list_preserves_cached_thumbnails(self) -> None:
        """set_paths([]) removes all paths but preserves cached thumbnails.

        Cached thumbnails are retained so that re-populating the model with
        the same paths (e.g. removing a filter) doesn't lose thumbnails.
        """
        model = ThumbnailModel()
        model.set_paths([Path("/a/one.jpg")])
        model.set_thumbnail("/a/one.jpg", Path("/cache/256/one.png"), resolution=256)

        model.set_paths([])

        assert model.rowCount() == 0
        # Thumbnails are preserved (not pruned)
        assert "/a/one.jpg" in model._thumbnails

    def test_set_paths_filter_unfilter_preserves_all_thumbnails(self) -> None:
        """Simulates color filter: filter to subset, then restore — all thumbnails survive.

        Regression test for Bug 1: filtering to 10 files pruned cache for the
        other 90, so removing the filter showed files without thumbnails.
        """
        model = ThumbnailModel()
        all_paths = [Path(f"/img/file_{i:03d}.jpg") for i in range(100)]
        model.set_paths(all_paths)

        # Set thumbnails for all 100 files
        for i in range(100):
            path_str = f"/img/file_{i:03d}.jpg"
            model.set_thumbnail(path_str, Path(f"/cache/256/file_{i:03d}.png"), resolution=256)

        # Filter to 10 files
        filtered_paths = all_paths[:10]
        model.set_paths(filtered_paths)
        assert model.rowCount() == 10

        # Remove filter — restore all 100 paths
        model.set_paths(all_paths)
        assert model.rowCount() == 100

        # ALL 100 thumbnails should still be present
        for i in range(100):
            idx = model.index(i, 0)
            thumb = model.data(idx, ThumbnailModel.ThumbnailRole256)
            assert thumb == f"/cache/256/file_{i:03d}.png", (
                f"Thumbnail for file_{i:03d}.jpg lost after unfilter"
            )
