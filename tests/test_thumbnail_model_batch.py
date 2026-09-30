"""Tests for ThumbnailModel.set_thumbnails batch API."""

from __future__ import annotations

from pathlib import Path

from PySide6.QtCore import QModelIndex

from tarragon.models.thumbnail_model import ThumbnailModel, ThumbnailUpdate
from tarragon.renderers.cache import RESOLUTION_FULL, RESOLUTION_PREVIEW, RESOLUTION_THUMBNAIL


class TestSetThumbnailsBatch:
    """The batch API stores entries and emits one dataChanged per affected row."""

    def test_batch_stores_all_entries(self) -> None:
        """set_thumbnails stores cache paths for every row and each role reads them back."""
        model = ThumbnailModel()
        model.set_paths([Path("/a/one.jpg"), Path("/b/two.jpg")])

        model.set_thumbnails(
            [
                ThumbnailUpdate("/a/one.jpg", Path("/cache/256/one.png"), RESOLUTION_THUMBNAIL),
                ThumbnailUpdate("/a/one.jpg", Path("/cache/full/one.png"), RESOLUTION_FULL),
                ThumbnailUpdate("/b/two.jpg", Path("/cache/1024/two.png"), RESOLUTION_PREVIEW),
            ]
        )

        assert model.data(model.index(0, 0), ThumbnailModel.ThumbnailRole256) == str(Path("/cache/256/one.png"))
        assert model.data(model.index(0, 0), ThumbnailModel.ThumbnailRoleFull) == str(Path("/cache/full/one.png"))
        assert model.data(model.index(1, 0), ThumbnailModel.ThumbnailRole1024) == str(Path("/cache/1024/two.png"))

    def test_batch_emits_one_data_changed_per_affected_row(self) -> None:
        """A batch touching N rows emits exactly N dataChanged signals (deduped per row)."""
        model = ThumbnailModel()
        model.set_paths([Path("/a/one.jpg"), Path("/b/two.jpg"), Path("/c/three.jpg")])
        emitted: list[tuple[int, list[int]]] = []

        def spy(top: QModelIndex, bottom: QModelIndex, roles: list[int]) -> None:
            emitted.append((top.row(), list(roles)))

        model.dataChanged.connect(spy)

        model.set_thumbnails(
            [
                ThumbnailUpdate("/a/one.jpg", Path("/c/256.png"), RESOLUTION_THUMBNAIL),
                ThumbnailUpdate("/a/one.jpg", Path("/c/1024.png"), RESOLUTION_PREVIEW),
                ThumbnailUpdate("/b/two.jpg", Path("/c/256.png"), RESOLUTION_THUMBNAIL),
            ]
        )

        assert len(emitted) == 2, f"expected one emission per distinct row, got {len(emitted)}"
        rows = {row for row, _roles in emitted}
        assert rows == {0, 1}

    def test_batch_emits_role_union_for_multi_resolution_row(self) -> None:
        """A row updated in multiple resolutions emits one dataChanged with all role changes."""
        model = ThumbnailModel()
        model.set_paths([Path("/a/one.jpg")])
        emitted: list[list[int]] = []

        def spy(_top: QModelIndex, _bottom: QModelIndex, roles: list[int]) -> None:
            emitted.append(list(roles))

        model.dataChanged.connect(spy)

        model.set_thumbnails(
            [
                ThumbnailUpdate("/a/one.jpg", Path("/c/256.png"), RESOLUTION_THUMBNAIL),
                ThumbnailUpdate("/a/one.jpg", Path("/c/full.png"), RESOLUTION_FULL),
            ]
        )

        assert len(emitted) == 1
        assert set(emitted[0]) == {ThumbnailModel.ThumbnailRole256, ThumbnailModel.ThumbnailRoleFull}

    def test_batch_matches_repeated_set_thumbnail_outcome(self) -> None:
        """set_thumbnails produces the same final state as repeated set_thumbnail calls."""
        batch_model = ThumbnailModel()
        single_model = ThumbnailModel()
        paths = [Path("/a/one.jpg"), Path("/b/two.jpg")]
        batch_model.set_paths(paths)
        single_model.set_paths(paths)

        updates = [
            ThumbnailUpdate("/a/one.jpg", Path("/c/256.png"), RESOLUTION_THUMBNAIL),
            ThumbnailUpdate("/a/one.jpg", Path("/c/1024.png"), RESOLUTION_PREVIEW),
            ThumbnailUpdate("/b/two.jpg", Path("/c/256.png"), RESOLUTION_THUMBNAIL),
        ]
        batch_model.set_thumbnails(updates)
        for update in updates:
            single_model.set_thumbnail(update.source_path, update.cache_path, resolution=update.resolution)

        assert batch_model._thumbnails == single_model._thumbnails
        for row in range(2):
            for role in (
                ThumbnailModel.ThumbnailRole256,
                ThumbnailModel.ThumbnailRole1024,
                ThumbnailModel.ThumbnailRoleFull,
            ):
                assert batch_model.data(batch_model.index(row, 0), role) == single_model.data(
                    single_model.index(row, 0), role
                )

    def test_batch_stores_entries_for_paths_not_in_model_without_emitting(self) -> None:
        """Entries for paths outside the path list are stored but emit no dataChanged."""
        model = ThumbnailModel()
        model.set_paths([Path("/a/one.jpg")])
        emitted: list[object] = []
        model.dataChanged.connect(lambda *args: emitted.append(args))

        model.set_thumbnails([ThumbnailUpdate("/z/away.png", Path("/c/256.png"), RESOLUTION_THUMBNAIL)])

        assert emitted == []
        assert str(Path("/z/away.png")) in model._thumbnails

    def test_empty_batch_is_noop(self) -> None:
        """set_thumbnails with an empty list changes nothing and emits nothing."""
        model = ThumbnailModel()
        model.set_paths([Path("/a/one.jpg")])
        emitted: list[object] = []
        model.dataChanged.connect(lambda *args: emitted.append(args))

        model.set_thumbnails([])

        assert emitted == []
        assert model._thumbnails == {}

    def test_set_thumbnail_wrapper_still_works(self) -> None:
        """The single-item wrapper remains a thin set_thumbnails call."""
        model = ThumbnailModel()
        model.set_paths([Path("/a/one.jpg")])

        model.set_thumbnail("/a/one.jpg", Path("/c/256.png"), resolution=RESOLUTION_THUMBNAIL)

        assert model.data(model.index(0, 0), ThumbnailModel.ThumbnailRole256) == str(Path("/c/256.png"))
