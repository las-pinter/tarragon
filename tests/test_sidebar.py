"""Tests for FavoritesModel and SidebarWidget."""

from __future__ import annotations

from pathlib import Path
from typing import Any, Generator

import pytest
from PySide6.QtCore import QModelIndex, Qt
from PySide6.QtWidgets import QApplication, QLabel, QListView, QPushButton, QTreeView

from tarragon.db import Database
from tarragon.models.favorites_model import FavoritesModel
from tarragon.widgets.sidebar import SidebarWidget

# ── Fixtures ──────────────────────────────────────────────────────────


@pytest.fixture()
def db() -> Generator[Database, None, None]:
    """Provide an isolated in-memory database with schema initialised."""
    conn = Database(Path(":memory:"))
    conn.init_schema()
    yield conn
    conn.close()


@pytest.fixture()
def populated_db(db: Database) -> Database:
    """Return a database pre-loaded with two favourite entries."""
    db.add_favorite("/photos/landscape.png", label="Landscapes")
    db.add_favorite("/photos/portrait.jpg")  # no label — should use filename
    return db


@pytest.fixture()
def model(db: Database) -> FavoritesModel:
    """Provide a FavoritesModel backed by an empty database."""
    return FavoritesModel(db)


@pytest.fixture()
def populated_model(populated_db: Database) -> FavoritesModel:
    """Provide a FavoritesModel backed by a database with two favourites."""
    return FavoritesModel(populated_db)


@pytest.fixture()
def sidebar(db: Database) -> Generator[SidebarWidget, None, None]:
    """Provide a SidebarWidget that is cleaned up after the test."""
    w = SidebarWidget(db)
    yield w
    w.close()


# ── FavoritesModel: loading ──────────────────────────────────────────


class TestFavoritesModelLoad:
    def test_loads_from_db(self, populated_model: FavoritesModel) -> None:
        """Model loads favourites from DB on construction."""
        assert populated_model.rowCount() == 2

    def test_empty_db_has_zero_rows(self, model: FavoritesModel) -> None:
        """Model with an empty database has rowCount() == 0."""
        assert model.rowCount() == 0


# ── FavoritesModel: data roles ───────────────────────────────────────


class TestFavoritesModelData:
    def test_display_role_returns_label(self, populated_model: FavoritesModel) -> None:
        """DisplayRole returns the user-provided label when available."""
        index = populated_model.index(0)
        display = index.data(Qt.ItemDataRole.DisplayRole)
        assert display == "Landscapes"

    def test_display_role_falls_back_to_filename(
        self,
        populated_model: FavoritesModel,
    ) -> None:
        """DisplayRole returns the file basename when label is None."""
        index = populated_model.index(1)
        display = index.data(Qt.ItemDataRole.DisplayRole)
        assert display == "portrait.jpg"

    def test_user_role_returns_path(self, populated_model: FavoritesModel) -> None:
        """UserRole returns the full path string."""
        index = populated_model.index(0)
        path = index.data(Qt.ItemDataRole.UserRole)
        assert path == "/photos/landscape.png"

    def test_invalid_index_returns_none(self, populated_model: FavoritesModel) -> None:
        """data() returns None for an invalid index."""
        result = populated_model.data(QModelIndex())
        assert result is None

    def test_unsupported_role_returns_none(
        self,
        populated_model: FavoritesModel,
    ) -> None:
        """data() returns None for unsupported roles."""
        index = populated_model.index(0)
        result = index.data(Qt.ItemDataRole.DecorationRole)
        assert result is None


# ── FavoritesModel: add / remove ─────────────────────────────────────


class TestFavoritesModelMutate:
    def test_add_favorite(self, model: FavoritesModel) -> None:
        """Adding a favourite via the model increases rowCount."""
        model.add_favorite("/new/path.png", label="New One")
        assert model.rowCount() == 1
        assert model.index(0).data(Qt.ItemDataRole.DisplayRole) == "New One"

    def test_add_favorite_without_label(self, model: FavoritesModel) -> None:
        """Adding a favourite without a label uses the filename as display."""
        model.add_favorite("/new/photo.jpg")
        assert model.index(0).data(Qt.ItemDataRole.DisplayRole) == "photo.jpg"

    def test_remove_favorite(self, model: FavoritesModel) -> None:
        """Adding then removing a favourite decreases rowCount to 0."""
        model.add_favorite("/remove/me.png", label="Remove Me")
        assert model.rowCount() == 1
        model.remove_favorite("/remove/me.png")
        assert model.rowCount() == 0

    def test_remove_only_one(self, populated_model: FavoritesModel) -> None:
        """Removing one favourite leaves the other intact."""
        assert populated_model.rowCount() == 2
        populated_model.remove_favorite("/photos/landscape.png")
        assert populated_model.rowCount() == 1
        assert populated_model.index(0).data(Qt.ItemDataRole.DisplayRole) == "portrait.jpg"

    def test_favorite_paths(self, populated_model: FavoritesModel) -> None:
        """favorite_paths() returns all stored paths."""
        paths = populated_model.favorite_paths()
        assert paths == ["/photos/landscape.png", "/photos/portrait.jpg"]

    def test_load_from_db_reflects_external_change(
        self,
        model: FavoritesModel,
        db: Database,
    ) -> None:
        """load_from_db picks up changes made directly via the database."""
        db.add_favorite("/external/path.tga", label="External")
        model.load_from_db()
        assert model.rowCount() == 1
        assert model.index(0).data(Qt.ItemDataRole.DisplayRole) == "External"


# ── SidebarWidget: structure ─────────────────────────────────────────


class TestSidebarWidgetStructure:
    def test_creation(self, sidebar: SidebarWidget) -> None:
        """SidebarWidget is created without error and has expected children."""
        assert isinstance(sidebar, SidebarWidget)

    def test_has_list_view(self, sidebar: SidebarWidget) -> None:
        """SidebarWidget contains a QListView."""
        list_view = sidebar.findChild(QListView)
        assert list_view is not None

    def test_has_header_label(self, sidebar: SidebarWidget) -> None:
        """SidebarWidget has a QLabel header with 'Favorites' text."""
        labels = sidebar.findChildren(QLabel)
        texts = [lbl.text() for lbl in labels]
        assert "Favorites" in texts

    def test_has_add_button(self, sidebar: SidebarWidget) -> None:
        """SidebarWidget has an 'Add Current Folder' button."""
        buttons = sidebar.findChildren(QPushButton)
        texts = [b.text() for b in buttons]
        assert "Add Current Folder" in texts

    def test_has_remove_button(self, sidebar: SidebarWidget) -> None:
        """SidebarWidget has a 'Remove' button."""
        buttons = sidebar.findChildren(QPushButton)
        texts = [b.text() for b in buttons]
        assert "Remove" in texts

    def test_list_view_has_favorites_model(self, sidebar: SidebarWidget) -> None:
        """The QListView inside SidebarWidget uses a FavoritesModel."""
        list_view = sidebar.findChild(QListView)
        assert list_view is not None
        assert isinstance(list_view.model(), FavoritesModel)


# ── SidebarWidget: functionality ─────────────────────────────────────


class TestSidebarWidgetFunctionality:
    def test_add_current_folder(self, sidebar: SidebarWidget) -> None:
        """Setting the current folder and clicking 'Add' adds it to the model."""
        sidebar.set_current_folder("/my/folder")
        sidebar._on_add_clicked()
        list_view = sidebar.findChild(QListView)
        assert list_view is not None
        model = list_view.model()
        assert model is not None
        assert model.rowCount() == 1
        assert model.index(0, 0).data(Qt.ItemDataRole.UserRole) == "/my/folder"

    def test_add_current_folder_without_path_does_nothing(
        self,
        sidebar: SidebarWidget,
    ) -> None:
        """Clicking 'Add' without setting a current folder does not add anything."""
        sidebar._on_add_clicked()
        list_view = sidebar.findChild(QListView)
        assert list_view is not None
        model = list_view.model()
        assert model is not None
        assert model.rowCount() == 0

    def test_remove_selected_favorite(self, sidebar: SidebarWidget) -> None:
        """Adding then selecting and removing a favourite works."""
        sidebar.set_current_folder("/remove/this")
        sidebar._on_add_clicked()
        list_view = sidebar.findChild(QListView)
        assert list_view is not None
        lv_model = list_view.model()
        assert lv_model is not None
        assert lv_model.rowCount() == 1

        # Select the item
        index = lv_model.index(0, 0)
        sel_model = list_view.selectionModel()
        assert sel_model is not None
        sel_model.select(
            index,
            sel_model.SelectionFlag.Select,
        )

        sidebar._on_remove_clicked()
        assert lv_model.rowCount() == 0

    def test_favorite_clicked_signal(self, sidebar: SidebarWidget) -> None:
        """Single-clicking a favourite emits favorite_clicked with the path."""
        sidebar.set_current_folder("/clicked/path.exr")
        sidebar._on_add_clicked()

        # Capture the signal
        captured_args: list[tuple[str, ...]] = []
        sidebar.favorite_clicked.connect(lambda *args: captured_args.append(args))

        list_view = sidebar.findChild(QListView)
        assert list_view is not None
        lv_model = list_view.model()
        assert lv_model is not None
        index = lv_model.index(0, 0)
        list_view.clicked.emit(index)

        assert len(captured_args) == 1
        assert captured_args[0] == ("/clicked/path.exr",)

    def test_favorite_single_click_navigation(self, sidebar: SidebarWidget) -> None:
        """Single-clicking a favorite navigates to its path (not double-click)."""
        sidebar.set_current_folder("/nav/target.exr")
        sidebar._on_add_clicked()

        captured: list[str] = []
        sidebar.favorite_clicked.connect(lambda path: captured.append(path))

        list_view = sidebar.findChild(QListView)
        assert list_view is not None
        lv_model = list_view.model()
        assert lv_model is not None
        index = lv_model.index(0, 0)

        # Single click should navigate
        list_view.clicked.emit(index)
        assert captured == ["/nav/target.exr"]

    def test_favorite_double_click_does_not_navigate(self, sidebar: SidebarWidget) -> None:
        """Double-clicking a favorite no longer emits favorite_clicked."""
        sidebar.set_current_folder("/double/click.exr")
        sidebar._on_add_clicked()

        captured: list[str] = []
        sidebar.favorite_clicked.connect(lambda path: captured.append(path))

        list_view = sidebar.findChild(QListView)
        assert list_view is not None
        lv_model = list_view.model()
        assert lv_model is not None
        index = lv_model.index(0, 0)

        # Double-click should NOT navigate (signal was changed to clicked)
        list_view.doubleClicked.emit(index)
        assert captured == []

    def test_favorite_clicked_invalid_index_does_not_emit(
        self, sidebar: SidebarWidget
    ) -> None:
        """Clicking with an invalid index does not emit favorite_clicked."""
        captured: list[str] = []
        sidebar.favorite_clicked.connect(lambda path: captured.append(path))

        # Call the handler directly with an invalid index
        sidebar._on_favorite_clicked(QModelIndex())
        assert captured == []

    def test_remove_button_does_nothing_when_nothing_selected(
        self,
        sidebar: SidebarWidget,
    ) -> None:
        """Clicking Remove with no selection does not crash."""
        sidebar.set_current_folder("/safe/path")
        sidebar._on_add_clicked()
        # Don't select anything — just click Remove
        sidebar._on_remove_clicked()
        list_view = sidebar.findChild(QListView)
        assert list_view is not None
        model = list_view.model()
        assert model is not None
        assert model.rowCount() == 1

    def test_folder_single_click_emits_navigated(
        self,
        sidebar: SidebarWidget,
        tmp_path: Path,
    ) -> None:
        """Single-clicking a folder in the tree emits folder_navigated."""
        # Create a subfolder so the tree has something to show
        subfolder = tmp_path / "subfolder"
        subfolder.mkdir()

        # Point the folder model at our temp directory
        sidebar._folder_model.setRootPath(str(tmp_path))
        tree = sidebar.findChild(QTreeView)
        assert tree is not None
        tree.setRootIndex(sidebar._folder_model.index(str(tmp_path)))

        # Capture the signal
        captured: list[str] = []
        sidebar.folder_navigated.connect(lambda path: captured.append(path))

        # Get the index for the subfolder and emit a single click
        child_index = sidebar._folder_model.index(str(subfolder))
        tree.clicked.emit(child_index)

        assert len(captured) == 1
        assert captured[0] == str(subfolder)
