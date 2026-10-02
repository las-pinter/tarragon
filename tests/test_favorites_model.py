"""Row-level mutation tests for FavoritesModel: signals and persistence."""

from __future__ import annotations

from collections.abc import Generator
from pathlib import Path
from unittest.mock import MagicMock

import pytest
from PySide6.QtCore import QMimeData, QModelIndex, Qt

from tarragon.db.database import Database
from tarragon.models.favorites_model import FAVORITE_MIME_TYPE, FavoritesModel


@pytest.fixture
def db() -> Generator[Database, None, None]:
    """Provide an isolated in-memory database with schema initialised."""
    conn = Database(Path(":memory:"))
    conn.init_schema()
    yield conn
    conn.close()


@pytest.fixture
def model(db: Database) -> FavoritesModel:
    """Provide a FavoritesModel backed by an empty database."""
    return FavoritesModel(db)


class TestAddFavoriteRows:
    """add_favorite appends at the end with row-level insert signals."""

    def test_add_favorite_emits_insert_rows_signals(
        self, model: FavoritesModel, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        """add_favorite announces beginInsertRows/endInsertRows, not a reset."""
        begin = MagicMock()
        end = MagicMock()
        reset = MagicMock()
        monkeypatch.setattr(model, "beginInsertRows", begin)
        monkeypatch.setattr(model, "endInsertRows", end)
        monkeypatch.setattr(model, "beginResetModel", reset)

        model.add_favorite("/new/path.png", label="New")

        begin.assert_called_once_with(QModelIndex(), 0, 0)
        end.assert_called_once()
        reset.assert_not_called()

    def test_add_favorite_appends_at_end_with_max_plus_one(self, model: FavoritesModel, db: Database) -> None:
        """New favorites land at the end with sort_order greater than existing."""
        model.add_favorite("/first.png")
        model.add_favorite("/second.png", label="Second")

        assert model.favorite_paths() == ["/first.png", "/second.png"]
        favorites = db.list_favorites()
        assert [f["path"] for f in favorites] == ["/first.png", "/second.png"]
        orders = [f["sort_order"] for f in favorites]
        assert orders[0] > 0
        assert orders[1] > orders[0]

    def test_add_favorite_does_not_persist_reset(self, model: FavoritesModel, monkeypatch: pytest.MonkeyPatch) -> None:
        """Add path alone does not call load_from_db."""
        load = MagicMock()
        monkeypatch.setattr(model, "load_from_db", load)

        model.add_favorite("/x.png")

        load.assert_not_called()


class TestRemoveFavoriteRows:
    """remove_favorite(row) removes a single row with row-level signals."""

    def test_remove_favorite_emits_remove_rows_signals(
        self, model: FavoritesModel, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        """remove_favorite announces beginRemoveRows/endRemoveRows, not a reset."""
        model.add_favorite("/gone.png")
        begin = MagicMock()
        end = MagicMock()
        reset = MagicMock()
        monkeypatch.setattr(model, "beginRemoveRows", begin)
        monkeypatch.setattr(model, "endRemoveRows", end)
        monkeypatch.setattr(model, "beginResetModel", reset)

        model.remove_favorite(0)

        begin.assert_called_once_with(QModelIndex(), 0, 0)
        end.assert_called_once()
        reset.assert_not_called()
        assert model.rowCount() == 0

    def test_remove_favorite_persists_to_db(self, model: FavoritesModel, db: Database) -> None:
        """remove_favorite deletes the row from the database too."""
        model.add_favorite("/gone.png")
        model.add_favorite("/stay.png")

        model.remove_favorite(0)

        assert [f["path"] for f in db.list_favorites()] == ["/stay.png"]

    def test_remove_favorite_invalid_row_is_noop(self, model: FavoritesModel, monkeypatch: pytest.MonkeyPatch) -> None:
        """Out-of-range rows are ignored and not persisted."""
        model.add_favorite("/stay.png")
        remove = MagicMock()
        monkeypatch.setattr(model._db, "remove_favorite", remove)

        model.remove_favorite(5)
        model.remove_favorite(-1)

        assert model.rowCount() == 1
        remove.assert_not_called()

    def test_remove_favorite_does_not_persist_reset(
        self, model: FavoritesModel, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        """Removal does not call load_from_db."""
        model.add_favorite("/gone.png")
        load = MagicMock()
        monkeypatch.setattr(model, "load_from_db", load)

        model.remove_favorite(0)

        load.assert_not_called()


class TestMoveFavorite:
    """move_favorite reorders the model list and persists the reindex."""

    @pytest.fixture
    def three_favorites(self, model: FavoritesModel) -> FavoritesModel:
        model.add_favorite("/a.png")
        model.add_favorite("/b.png")
        model.add_favorite("/c.png")
        return model

    def test_move_favorite_up_reorders_and_persists(self, three_favorites: FavoritesModel, db: Database) -> None:
        """Moving the last row to the front is reflected in model and db."""
        three_favorites.move_favorite(2, 0)

        assert three_favorites.favorite_paths() == ["/c.png", "/a.png", "/b.png"]
        favorites = db.list_favorites()
        assert [f["path"] for f in favorites] == ["/c.png", "/a.png", "/b.png"]
        assert [f["sort_order"] for f in favorites] == [0, 1, 2]

    def test_move_favorite_down_reorders(self, three_favorites: FavoritesModel) -> None:
        """Moving the first row down preserves the remaining relative order."""
        three_favorites.move_favorite(0, 2)

        assert three_favorites.favorite_paths() == ["/b.png", "/c.png", "/a.png"]

    def test_move_favorite_emits_move_rows_signals(
        self, three_favorites: FavoritesModel, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        """move_favorite announces beginMoveRows/endMoveRows, not a reset."""
        begin = MagicMock()
        end = MagicMock()
        reset = MagicMock()
        monkeypatch.setattr(three_favorites, "beginMoveRows", begin)
        monkeypatch.setattr(three_favorites, "endMoveRows", end)
        monkeypatch.setattr(three_favorites, "beginResetModel", reset)

        three_favorites.move_favorite(2, 0)

        begin.assert_called_once_with(QModelIndex(), 2, 2, QModelIndex(), 0)
        end.assert_called_once()
        reset.assert_not_called()

    def test_move_favorite_does_not_persist_reset(
        self, three_favorites: FavoritesModel, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        """A move does not call load_from_db."""
        load = MagicMock()
        monkeypatch.setattr(three_favorites, "load_from_db", load)

        three_favorites.move_favorite(2, 0)

        load.assert_not_called()

    def test_move_favorite_same_row_is_noop(
        self, three_favorites: FavoritesModel, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        """Moving a row to its own position does not re-persist the order."""
        reorder = MagicMock()
        monkeypatch.setattr(three_favorites._db, "reorder_favorites", reorder)

        three_favorites.move_favorite(1, 1)

        reorder.assert_not_called()
        assert three_favorites.favorite_paths() == ["/a.png", "/b.png", "/c.png"]

    def test_move_favorite_invalid_rows_are_noop(
        self, three_favorites: FavoritesModel, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        """Out-of-range rows are ignored and not persisted."""
        reorder = MagicMock()
        monkeypatch.setattr(three_favorites._db, "reorder_favorites", reorder)

        three_favorites.move_favorite(3, 0)
        three_favorites.move_favorite(0, 9)

        reorder.assert_not_called()
        assert three_favorites.favorite_paths() == ["/a.png", "/b.png", "/c.png"]


class TestDragAndDrop:
    """Mime payload, flags, and drop handling for internal reordering."""

    def test_flags_enable_drag_and_drop(self, model: FavoritesModel) -> None:
        """Valid rows are draggable and the list accepts drops."""
        model.add_favorite("/a.png")

        flags = model.flags(model.index(0))
        assert flags & Qt.ItemFlag.ItemIsDragEnabled
        assert flags & Qt.ItemFlag.ItemIsDropEnabled

    def test_mime_types_and_payload_roundtrip(self, model: FavoritesModel) -> None:
        """mimeData encodes the dragged path under the custom mime type."""
        model.add_favorite("/a.png")

        assert model.mimeTypes() == [FAVORITE_MIME_TYPE]
        mime = model.mimeData([model.index(0)])
        assert mime.hasFormat(FAVORITE_MIME_TYPE)
        assert bytes(mime.data(FAVORITE_MIME_TYPE)) == b"/a.png"

    def test_supported_drop_actions_is_move(self, model: FavoritesModel) -> None:
        """Only MoveAction is advertised."""
        assert model.supportedDropActions() == Qt.DropAction.MoveAction

    def test_drop_mime_data_reorders(self, model: FavoritesModel, db: Database) -> None:
        """Dropping a favorite before row 0 moves it to the front and persists."""
        model.add_favorite("/a.png")
        model.add_favorite("/b.png")
        model.add_favorite("/c.png")

        mime = QMimeData()
        mime.setData(FAVORITE_MIME_TYPE, b"/c.png")
        accepted = model.dropMimeData(mime, Qt.DropAction.MoveAction, 0, 0, QModelIndex())

        assert accepted
        assert model.favorite_paths() == ["/c.png", "/a.png", "/b.png"]
        assert [f["path"] for f in db.list_favorites()] == ["/c.png", "/a.png", "/b.png"]

    def test_drop_mime_data_below_item_is_noop(self, model: FavoritesModel) -> None:
        """Dropping a row directly below itself is a no-op move."""
        model.add_favorite("/a.png")
        model.add_favorite("/b.png")
        model.add_favorite("/c.png")

        mime = QMimeData()
        mime.setData(FAVORITE_MIME_TYPE, b"/a.png")
        assert model.dropMimeData(mime, Qt.DropAction.MoveAction, 1, 0, QModelIndex())

        assert model.favorite_paths() == ["/a.png", "/b.png", "/c.png"]

    def test_drop_mime_data_on_item_parent(self, model: FavoritesModel) -> None:
        """A drop onto an item (valid parent) inserts before that item."""
        model.add_favorite("/a.png")
        model.add_favorite("/b.png")
        model.add_favorite("/c.png")

        mime = QMimeData()
        mime.setData(FAVORITE_MIME_TYPE, b"/c.png")
        assert model.dropMimeData(mime, Qt.DropAction.MoveAction, -1, 0, model.index(1))

        assert model.favorite_paths() == ["/a.png", "/c.png", "/b.png"]

    def test_drop_mime_data_rejects_foreign_data(self, model: FavoritesModel) -> None:
        """Mime payloads without the favorite-path format are rejected."""
        model.add_favorite("/a.png")

        foreign = QMimeData()
        foreign.setText("anything")
        assert not model.dropMimeData(foreign, Qt.DropAction.MoveAction, 0, 0, QModelIndex())
        assert model.favorite_paths() == ["/a.png"]

    def test_drop_mime_data_rejects_out_of_range_rows(
        self, model: FavoritesModel, db: Database, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        """Drops beyond the list or at a negative row without a parent are no-ops.

        The guard rejects before any conversion or persistence, so the model
        order and the database sort orders stay untouched and no crash occurs.
        """
        model.add_favorite("/a.png")
        model.add_favorite("/b.png")
        model.add_favorite("/c.png")

        reorder = MagicMock()
        monkeypatch.setattr(model._db, "reorder_favorites", reorder)

        mime = QMimeData()
        mime.setData(FAVORITE_MIME_TYPE, b"/a.png")

        assert not model.dropMimeData(mime, Qt.DropAction.MoveAction, 99, 0, QModelIndex())
        assert not model.dropMimeData(mime, Qt.DropAction.MoveAction, -5, 0, QModelIndex())

        assert model.favorite_paths() == ["/a.png", "/b.png", "/c.png"]
        reorder.assert_not_called()
        favorites = db.list_favorites()
        assert [f["path"] for f in favorites] == ["/a.png", "/b.png", "/c.png"]
        assert [f["sort_order"] for f in favorites] == [1, 2, 3]

    def test_drop_mime_data_ignore_action_returns_true(self, model: FavoritesModel) -> None:
        """IgnoreAction drops are accepted without changing anything."""
        model.add_favorite("/a.png")

        mime = QMimeData()
        mime.setData(FAVORITE_MIME_TYPE, b"/a.png")
        assert model.dropMimeData(mime, Qt.DropAction.IgnoreAction, 0, 0, QModelIndex())
        assert model.favorite_paths() == ["/a.png"]
