"""Tests for ThumbnailGrid and ThumbnailDelegate"""

from __future__ import annotations

from collections.abc import Generator
from pathlib import Path
from typing import Any
from unittest.mock import MagicMock, patch

import pytest
from PySide6.QtCore import (
    QEvent,
    QItemSelection,
    QModelIndex,
    QPoint,
    QPointF,
    QRect,
    QSize,
    Qt,
)
from PySide6.QtGui import QContextMenuEvent, QMouseEvent
from PySide6.QtWidgets import (
    QListView,
    QStyle,
    QStyledItemDelegate,
    QStyleOptionViewItem,
    QToolTip,
)

from tarragon.models.thumbnail_model import ThumbnailModel
from tarragon.theme.colors import BG_PRIMARY, BG_SECONDARY
from tarragon.widgets.thumbnail_delegate import (
    GRID_GAP,
    HOVER_MARGIN,
    THUMBNAIL_SIZE,
    ThumbnailDelegate,
)
from tarragon.widgets.thumbnail_grid import ThumbnailGrid


@pytest.fixture
def delegate() -> ThumbnailDelegate:
    """Provide a fresh ThumbnailDelegate."""
    return ThumbnailDelegate()


@pytest.fixture
def grid() -> Generator[ThumbnailGrid, None, None]:
    """Provide a ThumbnailGrid that is closed after the test."""
    g = ThumbnailGrid()
    yield g
    g.close()


@pytest.fixture
def grid_with_model(grid: ThumbnailGrid) -> tuple[ThumbnailGrid, ThumbnailModel]:
    """Provide a ThumbnailGrid backed by a ThumbnailModel with sample paths."""
    model = ThumbnailModel()
    model.set_paths(
        [
            Path("/fake/images/photo_001.png"),
            Path("/fake/images/photo_002.jpg"),
            Path("/fake/images/photo_003.png"),
            Path("/fake/images/photo_004.jpg"),
            Path("/fake/images/photo_005.png"),
        ]
    )
    grid.set_model(model)
    return grid, model


@pytest.fixture
def mock_painter() -> MagicMock:
    """Provide a mocked QPainter for paint-method tests."""
    painter = MagicMock()
    painter.font.return_value = MagicMock()
    # Make elidedText pass through the input text by default
    painter.fontMetrics.return_value.elidedText.side_effect = lambda text, mode, width: text
    return painter


@pytest.fixture
def style_option() -> QStyleOptionViewItem:
    """Provide a QStyleOptionViewItem with a realistic rect and no special state."""
    opt = QStyleOptionViewItem()
    cell_w = THUMBNAIL_SIZE + GRID_GAP * 2 + HOVER_MARGIN * 2
    cell_h = THUMBNAIL_SIZE + GRID_GAP * 2 + 24 + HOVER_MARGIN * 2
    opt.rect = QRect(0, 0, cell_w, cell_h)
    opt.state = QStyle.StateFlag.State_None
    return opt


def _make_index(model: Any, row: int) -> Any:
    """Helper: return a valid QModelIndex for *row* in *model*."""
    return model.index(row)


class TestThumbnailGridCreation:
    """ThumbnailGrid construction and basic view configuration."""

    def test_thumbnail_grid_is_qlistview(self) -> None:
        """ThumbnailGrid is a QListView subclass."""
        assert issubclass(ThumbnailGrid, QListView)

    def test_thumbnail_grid_icon_mode(self, qapp: Any) -> None:
        """ThumbnailGrid uses IconMode view."""
        grid = ThumbnailGrid()
        try:
            assert grid.viewMode() == QListView.ViewMode.IconMode
        finally:
            grid.close()

    def test_thumbnail_grid_has_delegate(self, qapp: Any) -> None:
        """ThumbnailGrid has a ThumbnailDelegate set."""
        grid = ThumbnailGrid()
        try:
            delegate = grid.itemDelegate()
            assert isinstance(delegate, ThumbnailDelegate)
        finally:
            grid.close()

    def test_thumbnail_grid_extended_selection(self, qapp: Any) -> None:
        """ThumbnailGrid allows extended (multi) selection."""
        grid = ThumbnailGrid()
        try:
            assert grid.selectionMode() == QListView.SelectionMode.ExtendedSelection
        finally:
            grid.close()

    def test_thumbnail_grid_set_model(self, qapp: Any) -> None:
        """ThumbnailGrid accepts a ThumbnailModel."""
        grid = ThumbnailGrid()
        model = ThumbnailModel()
        try:
            grid.set_model(model)
            assert grid.model() is model
        finally:
            grid.close()


class TestThumbnailDelegateBasics:
    """ThumbnailDelegate construction and basic behaviour."""

    def test_thumbnail_delegate_is_qstyleditemdelegate(self) -> None:
        """ThumbnailDelegate is a QStyledItemDelegate subclass."""
        assert issubclass(ThumbnailDelegate, QStyledItemDelegate)

    def test_thumbnail_delegate_size_hint(self) -> None:
        """The sizeHint() method returns correct dimensions for a grid cell."""
        delegate = ThumbnailDelegate()
        hint = delegate.sizeHint(None, None)  # type: ignore[arg-type]
        expected_width = THUMBNAIL_SIZE + GRID_GAP * 2 + HOVER_MARGIN * 2
        expected_height = THUMBNAIL_SIZE + GRID_GAP * 2 + 24 + HOVER_MARGIN * 2
        assert hint == QSize(expected_width, expected_height)


class TestPaintEmptyModel:
    """Painting with an empty model is safe."""

    def test_paint_with_empty_model_does_not_crash(self, delegate: Any, mock_painter: Any, style_option: Any) -> None:
        """Painting when the model has 0 rows does not raise."""
        # No paths set - model is empty
        # Use an invalid index (row 0 on empty model)
        index = QModelIndex()
        delegate.paint(mock_painter, style_option, index)
        # Should complete without exception; fillRect called for background
        assert mock_painter.save.called
        assert mock_painter.restore.called


class TestPaintInvalidIndex:
    """Painting with an invalid index is safe and uses an empty name."""

    def test_paint_with_invalid_index_does_not_crash(self, delegate: Any, mock_painter: Any, style_option: Any) -> None:
        """Painting with an invalid QModelIndex does not raise."""
        invalid_index = QModelIndex()
        assert not invalid_index.isValid()
        delegate.paint(mock_painter, style_option, invalid_index)
        assert mock_painter.save.called
        assert mock_painter.restore.called

    def test_paint_with_invalid_index_uses_empty_name(
        self, delegate: Any, mock_painter: Any, style_option: Any
    ) -> None:
        """Painting with invalid index falls back to empty string for display name."""
        invalid_index = QModelIndex()
        delegate.paint(mock_painter, style_option, invalid_index)
        # drawText should be called with empty string (since data() returns None -> "")
        draw_text_calls = mock_painter.drawText.call_args_list
        assert len(draw_text_calls) >= 1
        # The first drawText call should contain empty string for the name
        first_call_args = draw_text_calls[0].args
        assert first_call_args[-1] == ""


class TestPaintNoneData:
    """Painting when model roles return None is handled safely."""

    def test_paint_when_path_role_returns_none(self, delegate: Any, mock_painter: Any, style_option: Any) -> None:
        """Painting when PathRole returns None does not crash (no pixmap drawn)."""
        model = ThumbnailModel()
        model.set_paths([Path("/fake/test.png")])
        index = model.index(0)

        # Patch index.data to return None for PathRole
        original_data = index.data

        def mock_data(role: int) -> Any:
            if role == ThumbnailModel.PathRole:
                return None
            if role == Qt.ItemDataRole.DisplayRole:
                return "test.png"
            return original_data(role)

        with patch.object(index, "data", side_effect=mock_data):
            delegate.paint(mock_painter, style_option, index)

        # drawPixmap should NOT be called since pixmap would be null/None
        mock_painter.drawPixmap.assert_not_called()
        # But save/restore still happen
        assert mock_painter.save.called
        assert mock_painter.restore.called

    def test_paint_when_display_role_returns_none(self, delegate: Any, mock_painter: Any, style_option: Any) -> None:
        """Painting when DisplayRole returns None uses empty string for name."""
        model = ThumbnailModel()
        model.set_paths([Path("/fake/test.png")])
        index = model.index(0)

        original_data = index.data

        def mock_data(role: int) -> Any:
            if role == Qt.ItemDataRole.DisplayRole:
                return None
            return original_data(role)

        with patch.object(index, "data", side_effect=mock_data):
            delegate.paint(mock_painter, style_option, index)

        # drawText should still be called with empty string
        draw_text_calls = mock_painter.drawText.call_args_list
        name_call = draw_text_calls[0]
        assert name_call.args[-1] == ""


class TestPaintLongFilenames:
    """Painting with very long filenames does not crash."""

    def test_paint_with_very_long_filename(self, delegate: Any, mock_painter: Any, style_option: Any) -> None:
        """Painting with an extremely long filename does not crash."""
        model = ThumbnailModel()
        long_name = "a" * 500 + ".png"
        model.set_paths([Path(f"/fake/{long_name}")])
        index = model.index(0)

        delegate.paint(mock_painter, style_option, index)
        # Should complete without exception
        assert mock_painter.save.called
        assert mock_painter.restore.called
        # drawText should be called with the long name
        draw_text_calls = mock_painter.drawText.call_args_list
        assert any(long_name in str(call) for call in draw_text_calls)


class TestPaintUnicodeFilenames:
    """Painting with Unicode filenames does not crash."""

    @pytest.mark.parametrize(
        "filename",
        [
            pytest.param("日本語ファイル.png", id="japanese"),
            pytest.param("émojis_🎨🖌️🖼️.png", id="emoji"),
            pytest.param("кириллица_exposure.tga", id="cyrillic"),
            pytest.param("中文文件名.psd", id="chinese_psd"),
            pytest.param("über_straße_final_v2.exr", id="german"),
        ],
    )
    def test_paint_with_unicode_filename(
        self,
        delegate: Any,
        mock_painter: Any,
        style_option: Any,
        filename: str,
    ) -> None:
        """Painting with various Unicode filenames does not crash."""
        model = ThumbnailModel()
        model.set_paths([Path(f"/fake/{filename}")])
        index = model.index(0)

        delegate.paint(mock_painter, style_option, index)
        assert mock_painter.save.called
        assert mock_painter.restore.called


class TestPaintLargeModel:
    """ThumbnailGrid and delegate handle large models without error."""

    def test_model_with_1000_items_does_not_crash(self, grid: Any) -> None:
        """ThumbnailGrid handles a model with 1000+ items without error."""
        model = ThumbnailModel()
        paths = [Path(f"/fake/batch_{i:04d}.png") for i in range(1000)]
        model.set_paths(paths)
        grid.set_model(model)

        assert model.rowCount() == 1000
        # Indexing first and last items should work
        first = model.index(0)
        last = model.index(999)
        assert first.isValid()
        assert last.isValid()
        assert first.data() == "batch_0000.png"
        assert last.data() == "batch_0999.png"

    def test_delegate_size_hint_consistent_across_indices(self, delegate: Any) -> None:
        """The sizeHint() method returns the same size regardless of index."""
        model = ThumbnailModel()
        model.set_paths([Path(f"/fake/img_{i}.png") for i in range(10)])

        hint_0 = delegate.sizeHint(None, model.index(0))
        hint_5 = delegate.sizeHint(None, model.index(5))
        hint_9 = delegate.sizeHint(None, model.index(9))

        assert hint_0 == hint_5 == hint_9


class TestMouseMoveEvent:
    """The mouseMoveEvent handler updates the hovered row and repaints the viewport."""

    def test_mouse_move_event_over_empty_area_resets_hover(self, grid_with_model: Any) -> None:
        """The mouseMoveEvent handler resets hover to -1 when over an area with no item."""
        grid, model = grid_with_model
        delegate = grid._delegate

        # First set hover to something
        delegate.set_hovered_row(1)
        assert delegate._hovered_row == 1

        # Simulate mouse move to a point that likely has no item (far off)
        event = QMouseEvent(
            QMouseEvent.Type.MouseMove,
            QPointF(-9999, -9999),
            Qt.MouseButton.NoButton,
            Qt.MouseButton.NoButton,
            Qt.KeyboardModifier.NoModifier,
        )
        grid.mouseMoveEvent(event)

        # Hover should be reset since indexAt returns invalid index
        assert delegate._hovered_row == -1

    def test_mouse_move_event_triggers_viewport_update(self, grid_with_model: Any) -> None:
        """The mouseMoveEvent handler triggers a viewport update when the hovered row changes."""
        grid, _ = grid_with_model
        delegate = grid._delegate

        # Pre-set hover to a known row so the change is detected
        delegate.set_hovered_row(99)

        with patch.object(grid.viewport(), "update") as mock_update:
            event = QMouseEvent(
                QMouseEvent.Type.MouseMove,
                QPointF(-9999, -9999),  # guaranteed invalid -> row = -1
                Qt.MouseButton.NoButton,
                Qt.MouseButton.NoButton,
                Qt.KeyboardModifier.NoModifier,
            )
            grid.mouseMoveEvent(event)
            mock_update.assert_called_once()

    def test_mouse_move_event_no_update_when_row_unchanged(self, grid_with_model: Any) -> None:
        """The mouseMoveEvent handler does NOT trigger a viewport update when the hovered row is unchanged."""
        grid, _ = grid_with_model
        delegate = grid._delegate

        # Both current hover and mouse position resolve to row -1
        delegate.set_hovered_row(-1)

        with patch.object(grid.viewport(), "update") as mock_update:
            event = QMouseEvent(
                QMouseEvent.Type.MouseMove,
                QPointF(-9999, -9999),  # invalid -> row = -1 (same as current)
                Qt.MouseButton.NoButton,
                Qt.MouseButton.NoButton,
                Qt.KeyboardModifier.NoModifier,
            )
            grid.mouseMoveEvent(event)
            mock_update.assert_not_called()


class TestLeaveEvent:
    """The leaveEvent handler resets hover state and repaints the viewport."""

    def test_leave_event_resets_hover_state(self, grid_with_model: Any) -> None:
        """The leaveEvent handler resets the hovered row to -1."""
        grid, _ = grid_with_model
        delegate = grid._delegate

        delegate.set_hovered_row(2)
        assert delegate._hovered_row == 2

        event = QMouseEvent(
            QMouseEvent.Type.Leave,
            QPointF(0, 0),
            Qt.MouseButton.NoButton,
            Qt.MouseButton.NoButton,
            Qt.KeyboardModifier.NoModifier,
        )
        grid.leaveEvent(event)

        assert delegate._hovered_row == -1

    def test_leave_event_triggers_viewport_update(self, grid_with_model: Any) -> None:
        """The leaveEvent handler triggers a viewport update for repaint."""
        grid, _ = grid_with_model

        with patch.object(grid.viewport(), "update") as mock_update:
            event = QMouseEvent(
                QMouseEvent.Type.Leave,
                QPointF(0, 0),
                Qt.MouseButton.NoButton,
                Qt.MouseButton.NoButton,
                Qt.KeyboardModifier.NoModifier,
            )
            grid.leaveEvent(event)
            mock_update.assert_called_once()


class TestRapidHoverChanges:
    """Rapid hover changes always reflect the latest row."""

    def test_rapid_hover_changes_via_mouse_events(self, grid_with_model: Any) -> None:
        """Rapid mouseMoveEvents correctly update hover state each time."""
        grid, _ = grid_with_model
        delegate = grid._delegate

        # Simulate a sequence of mouse moves to off-screen points (all invalid)
        for _ in range(20):
            event = QMouseEvent(
                QMouseEvent.Type.MouseMove,
                QPointF(-1, -1),
                Qt.MouseButton.NoButton,
                Qt.MouseButton.NoButton,
                Qt.KeyboardModifier.NoModifier,
            )
            grid.mouseMoveEvent(event)

        assert delegate._hovered_row == -1


class TestBadgePainting:
    """File extension badges are painted for supported and unknown types."""

    def test_paint_psd_file_draws_badge(self, delegate: Any, mock_painter: Any, style_option: Any) -> None:
        """Painting a .psd file draws the PSD extension badge overlay."""
        model = ThumbnailModel()
        model.set_paths([Path("/fake/layer_composite.psd")])
        index = model.index(0)

        delegate.paint(mock_painter, style_option, index)

        # Badge text "PSD" should appear in one of the drawText calls
        draw_text_calls = mock_painter.drawText.call_args_list
        assert any("PSD" in str(call) for call in draw_text_calls)

    def test_paint_psb_file_draws_badge(self, delegate: Any, mock_painter: Any, style_option: Any) -> None:
        """Painting a .psb file draws a PSB badge overlay (not PSD)."""
        model = ThumbnailModel()
        model.set_paths([Path("/fake/big_document.psb")])
        index = model.index(0)

        delegate.paint(mock_painter, style_option, index)

        draw_text_calls = mock_painter.drawText.call_args_list
        # Badge text should be "PSB", not "PSD"
        assert any("PSB" in str(call) for call in draw_text_calls)
        assert not any("PSD" in str(call) for call in draw_text_calls)

    def test_paint_psd_case_insensitive(self, delegate: Any, mock_painter: Any, style_option: Any) -> None:
        """Extension badge is drawn regardless of file extension case (.PSD, .Psd)."""
        model = ThumbnailModel()
        model.set_paths([Path("/fake/UPPER.PSD")])
        index = model.index(0)

        delegate.paint(mock_painter, style_option, index)

        draw_text_calls = mock_painter.drawText.call_args_list
        # Badge text is always uppercase
        assert any("PSD" in str(call) for call in draw_text_calls)

    @pytest.mark.parametrize(
        "filename,expected_badge",
        [
            pytest.param("photo.jpg", "JPG", id="jpg"),
            pytest.param("photo.jpeg", "JPEG", id="jpeg"),
            pytest.param("image.png", "PNG", id="png"),
            pytest.param("scan.tiff", "TIFF", id="tiff"),
            pytest.param("scan.tif", "TIF", id="tif"),
            pytest.param("anim.gif", "GIF", id="gif"),
            pytest.param("hero.webp", "WEBP", id="webp"),
            pytest.param("old.bmp", "BMP", id="bmp"),
        ],
    )
    def test_paint_various_extensions_draw_badge(
        self,
        delegate: Any,
        mock_painter: Any,
        style_option: Any,
        filename: str,
        expected_badge: str,
    ) -> None:
        """Painting files with various extensions draws the correct uppercase badge."""
        model = ThumbnailModel()
        model.set_paths([Path(f"/fake/{filename}")])
        index = model.index(0)

        delegate.paint(mock_painter, style_option, index)

        draw_text_calls = mock_painter.drawText.call_args_list
        assert any(expected_badge in str(call) for call in draw_text_calls)

    def test_paint_unknown_extension_draws_badge(self, delegate: Any, mock_painter: Any, style_option: Any) -> None:
        """Painting a file with an unknown extension still draws a badge with the extension text."""
        model = ThumbnailModel()
        model.set_paths([Path("/fake/raw_image.exr")])
        index = model.index(0)

        delegate.paint(mock_painter, style_option, index)

        draw_text_calls = mock_painter.drawText.call_args_list
        assert any("EXR" in str(call) for call in draw_text_calls)

    def test_paint_file_without_extension_no_badge(self, delegate: Any, mock_painter: Any, style_option: Any) -> None:
        """Painting a file without an extension does NOT draw an extension badge."""
        model = ThumbnailModel()
        model.set_paths([Path("/fake/no_extension_file")])
        index = model.index(0)

        delegate.paint(mock_painter, style_option, index)

        # Only the filename drawText call should exist, no badge text
        draw_text_calls = mock_painter.drawText.call_args_list
        # Should have exactly 1 drawText call (the filename), no badge
        assert len(draw_text_calls) == 1

    def test_paint_png_file_draws_png_not_psd(self, delegate: Any, mock_painter: Any, style_option: Any) -> None:
        """Painting a .png file draws a PNG badge, not a PSD badge."""
        model = ThumbnailModel()
        model.set_paths([Path("/fake/regular_image.png")])
        index = model.index(0)

        delegate.paint(mock_painter, style_option, index)

        draw_text_calls = mock_painter.drawText.call_args_list
        # "PNG" should appear in badge
        assert any("PNG" in str(call) for call in draw_text_calls)
        # "PSD" should NOT appear
        assert not any("PSD" in str(call) for call in draw_text_calls)


class TestSelectionStatePainting:
    """Selected and hovered items use the correct background colours."""

    def test_paint_selected_item_uses_secondary_bg(self, delegate: Any, mock_painter: Any, style_option: Any) -> None:
        """Painting a selected item fills background with BG_SECONDARY."""
        model = ThumbnailModel()
        model.set_paths([Path("/fake/selected.png")])
        index = model.index(0)

        # Set the style option to selected state
        style_option.state = QStyle.StateFlag.State_Selected

        delegate.paint(mock_painter, style_option, index)

        # fillRect should be called with BG_SECONDARY for selected state
        fill_calls = mock_painter.fillRect.call_args_list
        assert len(fill_calls) >= 1
        # First fillRect is the background
        first_fill_args = fill_calls[0].args
        assert first_fill_args[1] == BG_SECONDARY

    def test_paint_selected_item_draws_coral_border(self, delegate: Any, mock_painter: Any, style_option: Any) -> None:
        """Painting a selected item draws a coral selection border."""
        model = ThumbnailModel()
        model.set_paths([Path("/fake/selected.png")])
        index = model.index(0)

        style_option.state = QStyle.StateFlag.State_Selected

        delegate.paint(mock_painter, style_option, index)

        # setPen should be called with a QPen using CORAL_MUTED
        # The border pen is set after the text pen - check for drawRoundedRect (border)
        assert mock_painter.drawRoundedRect.called

    def test_paint_hovered_item_uses_lighter_bg(self, delegate: Any, mock_painter: Any, style_option: Any) -> None:
        """Painting a hovered item fills background with a lighter variant."""
        model = ThumbnailModel()
        model.set_paths([Path("/fake/hovered.png")])
        index = model.index(0)

        # Set this row as hovered
        delegate.set_hovered_row(0)
        style_option.state = QStyle.StateFlag.State_None

        delegate.paint(mock_painter, style_option, index)

        fill_calls = mock_painter.fillRect.call_args_list
        assert len(fill_calls) >= 1
        # Background should be BG_SECONDARY.lighter(110)
        expected_color = BG_SECONDARY.lighter(110)
        first_fill_args = fill_calls[0].args
        assert first_fill_args[1] == expected_color

    def test_paint_normal_item_uses_primary_bg(self, delegate: Any, mock_painter: Any, style_option: Any) -> None:
        """Painting a non-selected, non-hovered item uses BG_PRIMARY."""
        model = ThumbnailModel()
        model.set_paths([Path("/fake/normal.png")])
        index = model.index(0)

        delegate.set_hovered_row(-1)  # ensure not hovered
        style_option.state = QStyle.StateFlag.State_None

        delegate.paint(mock_painter, style_option, index)

        fill_calls = mock_painter.fillRect.call_args_list
        assert len(fill_calls) >= 1
        first_fill_args = fill_calls[0].args
        assert first_fill_args[1] == BG_PRIMARY


class TestGridConfiguration:
    """ThumbnailGrid configuration matches delegate cell dimensions."""

    def test_thumbnail_grid_grid_size_matches_delegate(self, grid: Any) -> None:
        """Grid size is large enough for delegate cell + extra spacing."""
        delegate_hint = grid._delegate.sizeHint(None, None)
        grid_size = grid.gridSize()

        # Grid size should be at least as large as the delegate hint
        assert grid_size.width() >= delegate_hint.width()
        assert grid_size.height() >= delegate_hint.height()

    def test_thumbnail_grid_mouse_tracking_enabled(self, grid: Any) -> None:
        """ThumbnailGrid has mouse tracking enabled for hover effects."""
        assert grid.hasMouseTracking()

    def test_thumbnail_grid_icon_size_matches_thumbnail_size(self, grid: Any) -> None:
        """Icon size matches THUMBNAIL_SIZE constant."""
        assert grid.iconSize() == QSize(THUMBNAIL_SIZE, THUMBNAIL_SIZE)

    def test_thumbnail_grid_wrapping_enabled(self, grid: Any) -> None:
        """ThumbnailGrid has wrapping enabled for multi-row layout."""
        assert grid.isWrapping()

    def test_thumbnail_grid_horizontal_scrollbar_always_off(self, grid: Any) -> None:
        """Horizontal scrollbar is always off."""
        assert grid.horizontalScrollBarPolicy() == Qt.ScrollBarPolicy.ScrollBarAlwaysOff

    def test_thumbnail_grid_uniform_item_sizes(self, grid: Any) -> None:
        """Uniform item sizes enabled for performance."""
        assert grid.uniformItemSizes()

    def test_thumbnail_grid_spacing_uses_grid_gap(self, grid: Any) -> None:
        """Grid spacing is GRID_GAP for proper visual separation between cells."""
        assert grid.spacing() == GRID_GAP

    def test_grid_size_accounts_for_hover_margin(self, grid: Any) -> None:
        """Grid size includes HOVER_MARGIN so hovered thumbnails don't overlap neighbors."""
        grid_size = grid.gridSize()
        expected_w = THUMBNAIL_SIZE + GRID_GAP * 2 + HOVER_MARGIN * 2
        expected_h = THUMBNAIL_SIZE + GRID_GAP * 2 + 24 + HOVER_MARGIN * 2
        assert grid_size == QSize(expected_w, expected_h)


class TestDelegateInitialState:
    """ThumbnailDelegate starts with no hovered row and accepts any row value."""

    def test_delegate_initial_hover_is_negative_one(self) -> None:
        """ThumbnailDelegate starts with _hovered_row = -1 (no hover)."""
        d = ThumbnailDelegate()
        assert d._hovered_row == -1

    def test_delegate_set_hovered_row_with_large_value(self) -> None:
        """The set_hovered_row() method accepts arbitrarily large row numbers."""
        d = ThumbnailDelegate()
        d.set_hovered_row(999_999)
        assert d._hovered_row == 999_999

    def test_delegate_set_hovered_row_with_negative_value(self) -> None:
        """The set_hovered_row() method accepts negative values (reset signal)."""
        d = ThumbnailDelegate()
        d.set_hovered_row(5)
        d.set_hovered_row(-1)
        assert d._hovered_row == -1


class TestPaintSaveRestore:
    """The paint() method pairs save() and restore() calls."""

    def test_paint_always_pairs_save_and_restore(self, delegate: Any, mock_painter: Any, style_option: Any) -> None:
        """The paint() method always calls save() and restore() exactly once each."""
        model = ThumbnailModel()
        model.set_paths([Path("/fake/test.png")])
        index = model.index(0)

        delegate.paint(mock_painter, style_option, index)

        assert mock_painter.save.call_count == 1
        assert mock_painter.restore.call_count == 1

    def test_paint_save_restore_with_invalid_index(self, delegate: Any, mock_painter: Any, style_option: Any) -> None:
        """The paint() method pairs save/restore even with an invalid index."""
        invalid = QModelIndex()
        delegate.paint(mock_painter, style_option, invalid)

        assert mock_painter.save.call_count == 1
        assert mock_painter.restore.call_count == 1


class TestPaintSequentialItems:
    """Painting multiple items sequentially does not leak state."""

    def test_paint_multiple_items_sequentially(self, delegate: Any, mock_painter: Any, style_option: Any) -> None:
        """Painting multiple items in sequence does not leak state between calls."""
        model = ThumbnailModel()
        model.set_paths(
            [
                Path("/fake/a.png"),
                Path("/fake/b.psd"),
                Path("/fake/c.jpg"),
            ]
        )

        for row in range(3):
            mock_painter.reset_mock()
            index = model.index(row)
            delegate.paint(mock_painter, style_option, index)
            assert mock_painter.save.call_count == 1
            assert mock_painter.restore.call_count == 1


class TestRegressionStateSelected:
    """Painting with State_Selected does not crash."""

    def test_paint_with_selected_state_does_not_crash(
        self, delegate: Any, mock_painter: Any, style_option: Any
    ) -> None:
        """Painting with State_Selected does not crash (regression for QStyle.StateFlag fix)."""
        model = ThumbnailModel()
        model.set_paths([Path("/fake/selected.png")])
        index = model.index(0)

        style_option.state = QStyle.StateFlag.State_Selected

        # Should not raise AttributeError from bad enum access
        delegate.paint(mock_painter, style_option, index)
        assert mock_painter.save.called
        assert mock_painter.restore.called


class TestTextElision:
    """Long filenames are elided with QFontMetrics.elidedText."""

    def test_paint_long_filename_uses_elided_text(self, delegate: Any, style_option: Any) -> None:
        """Painting with a long filename uses QFontMetrics.elidedText for truncation."""
        mock_painter = MagicMock()
        mock_painter.font.return_value = MagicMock()
        mock_painter.fontMetrics.return_value.elidedText.return_value = "aaaa..."

        model = ThumbnailModel()
        long_name = "a" * 500 + ".png"
        model.set_paths([Path(f"/fake/{long_name}")])
        index = model.index(0)

        delegate.paint(mock_painter, style_option, index)

        # Verify elidedText was called with the long name
        mock_painter.fontMetrics.return_value.elidedText.assert_called_once()
        call_args = mock_painter.fontMetrics.return_value.elidedText.call_args
        assert call_args.args[0] == long_name

        # Verify the elided text was used in drawText
        draw_text_calls = mock_painter.drawText.call_args_list
        assert any("aaaa..." in str(call) for call in draw_text_calls)


class TestTooltipSupport:
    """The helpEvent handler shows tooltips for ToolTip events only."""

    def test_help_event_returns_true_for_tooltip(self, delegate: Any) -> None:
        """The helpEvent handler returns True and shows a tooltip for ToolTip events."""
        model = ThumbnailModel()
        model.set_paths([Path("/fake/test_image.png")])
        index = model.index(0)

        event = MagicMock()
        event.type.return_value = QEvent.Type.ToolTip
        event.globalPos.return_value = QPoint(100, 100)

        view = MagicMock()
        option = QStyleOptionViewItem()

        with patch.object(QToolTip, "showText") as mock_show:
            result = delegate.helpEvent(event, view, option, index)

        assert result is True
        mock_show.assert_called_once()
        # Verify the full filename was passed to showText
        call_args = mock_show.call_args
        assert call_args.args[1] == "test_image.png"

    def test_help_event_delegates_non_tooltip(self, delegate: Any) -> None:
        """The helpEvent handler does NOT show a tooltip for non-ToolTip events."""
        model = ThumbnailModel()
        model.set_paths([Path("/fake/test_image.png")])
        index = model.index(0)

        event = MagicMock()
        event.type.return_value = QEvent.Type.MouseMove

        view = MagicMock()
        option = QStyleOptionViewItem()

        # For non-ToolTip events, showText should NOT be called
        with patch.object(QToolTip, "showText") as mock_show:
            # super().helpEvent requires real Qt types, so we patch it out
            with patch(
                "tarragon.widgets.thumbnail_delegate.QStyledItemDelegate.helpEvent",
                return_value=False,
            ):
                result = delegate.helpEvent(event, view, option, index)

        assert result is False
        mock_show.assert_not_called()

    def test_help_event_empty_name_returns_true(self, delegate: Any) -> None:
        """The helpEvent handler returns True for ToolTip events even when the name is empty (no tooltip shown)."""
        invalid_index = QModelIndex()

        event = MagicMock()
        event.type.return_value = QEvent.Type.ToolTip
        event.globalPos.return_value = QPoint(0, 0)

        view = MagicMock()
        option = QStyleOptionViewItem()

        with patch.object(QToolTip, "showText") as mock_show:
            result = delegate.helpEvent(event, view, option, invalid_index)

        assert result is True
        # showText should NOT be called since name is empty
        mock_show.assert_not_called()


class TestContextMenu:
    """The context menu regenerates thumbnails via a signal."""

    def test_context_menu_emits_regenerate_signal(self, grid_with_model: Any) -> None:
        """The contextMenuEvent handler creates a menu action that emits regenerate_requested on a valid item."""
        grid, model = grid_with_model
        index = model.index(0)

        received: list[str] = []
        grid.regenerate_requested.connect(lambda p: received.append(p))

        # Mock indexAt to return a valid index.
        # Patch QMenu at the module level to avoid blocking on exec().
        mock_menu_instance = MagicMock()
        with (
            patch.object(grid, "indexAt", return_value=index),
            patch("tarragon.widgets.thumbnail_grid.QMenu", return_value=mock_menu_instance),
        ):
            event = QContextMenuEvent(
                QContextMenuEvent.Reason.Mouse,
                QPoint(50, 50),
                QPoint(100, 100),
            )
            grid.contextMenuEvent(event)

        # Verify QMenu was created and exec was called
        mock_menu_instance.addAction.assert_called_once()
        action = mock_menu_instance.addAction.call_args[0][0]
        assert action.text() == "Regenerate Thumbnail"
        mock_menu_instance.exec.assert_called_once()

    def test_context_menu_on_empty_area_does_not_emit(self, grid_with_model: Any) -> None:
        """The contextMenuEvent handler on an empty area (invalid index) does NOT emit a signal."""
        grid, _ = grid_with_model

        received: list[str] = []
        grid.regenerate_requested.connect(lambda p: received.append(p))

        # Mock indexAt to return an invalid index
        with patch.object(grid, "indexAt", return_value=QModelIndex()):
            event = QContextMenuEvent(
                QContextMenuEvent.Reason.Mouse,
                QPoint(50, 50),
                QPoint(100, 100),
            )
            grid.contextMenuEvent(event)

        assert len(received) == 0

    def test_context_menu_with_no_path_does_not_emit(self, grid_with_model: Any) -> None:
        """The contextMenuEvent handler does NOT emit a signal when PathRole returns None."""
        grid, model = grid_with_model
        index = model.index(0)

        received: list[str] = []
        grid.regenerate_requested.connect(lambda p: received.append(p))

        # Mock indexAt to return valid index, but data returns None for PathRole
        with (
            patch.object(grid, "indexAt", return_value=index),
            patch.object(model, "data", return_value=None),
        ):
            event = QContextMenuEvent(
                QContextMenuEvent.Reason.Mouse,
                QPoint(50, 50),
                QPoint(100, 100),
            )
            grid.contextMenuEvent(event)

        assert len(received) == 0

    def test_regenerate_requested_signal_exists(self, grid: Any) -> None:
        """ThumbnailGrid has a regenerate_requested signal."""
        assert hasattr(grid, "regenerate_requested")


class TestContainerMargin:
    """Background and border use an inset rect to keep a visual gap."""

    def test_paint_background_uses_inset_rect(self, delegate: Any, mock_painter: Any, style_option: Any) -> None:
        """Background fillRect uses a rect inset by HOVER_MARGIN, not the full option.rect."""
        model = ThumbnailModel()
        model.set_paths([Path("/fake/test.png")])
        index = model.index(0)

        delegate.set_hovered_row(-1)
        style_option.state = QStyle.StateFlag.State_None

        delegate.paint(mock_painter, style_option, index)

        fill_calls = mock_painter.fillRect.call_args_list
        assert len(fill_calls) >= 1
        bg_rect = fill_calls[0].args[0]

        # Background rect should be inset by HOVER_MARGIN on all sides
        expected_rect = style_option.rect.adjusted(HOVER_MARGIN, HOVER_MARGIN, -HOVER_MARGIN, -HOVER_MARGIN)
        assert bg_rect == expected_rect

    def test_paint_background_rect_is_smaller_than_cell(
        self, delegate: Any, mock_painter: Any, style_option: Any
    ) -> None:
        """Background container rect is strictly smaller than the full cell rect."""
        model = ThumbnailModel()
        model.set_paths([Path("/fake/test.png")])
        index = model.index(0)

        delegate.paint(mock_painter, style_option, index)

        fill_calls = mock_painter.fillRect.call_args_list
        bg_rect = fill_calls[0].args[0]

        assert bg_rect.width() < style_option.rect.width()
        assert bg_rect.height() < style_option.rect.height()

    def test_paint_selection_border_uses_inset_rect(self, delegate: Any, mock_painter: Any, style_option: Any) -> None:
        """Selection border drawRoundedRect uses the inset container rect, not the full cell."""
        model = ThumbnailModel()
        model.set_paths([Path("/fake/selected.png")])
        index = model.index(0)

        style_option.state = QStyle.StateFlag.State_Selected

        delegate.paint(mock_painter, style_option, index)

        # The drawRoundedRect call for the selection border should use the inset rect.
        # The badge also calls drawRoundedRect, so the border is the LAST call.
        container_rect = style_option.rect.adjusted(HOVER_MARGIN, HOVER_MARGIN, -HOVER_MARGIN, -HOVER_MARGIN)
        expected_border_rect = container_rect.adjusted(1, 1, -1, -1)

        draw_rounded_calls = mock_painter.drawRoundedRect.call_args_list
        assert len(draw_rounded_calls) >= 2  # badge + border
        border_rect = draw_rounded_calls[-1].args[0]
        assert border_rect == expected_border_rect


class TestSelectionChangedSignalSingle:
    """Single selection emits selection_changed with one path."""

    def test_single_selection_emits_one_path(self, grid_with_model: tuple[ThumbnailGrid, ThumbnailModel]) -> None:
        """Selecting one item emits selection_changed with a list of 1 path."""
        grid, model = grid_with_model

        emitted = []
        grid.selection_changed.connect(lambda paths: emitted.append(paths))

        index = model.index(0)
        selection = QItemSelection(index, index)
        grid.selectionModel().select(selection, grid.selectionModel().SelectionFlag.ClearAndSelect)

        assert len(emitted) >= 1
        last_emission = emitted[-1]
        assert len(last_emission) == 1
        assert last_emission[0] == str(Path("/fake/images/photo_001.png"))


class TestSelectionChangedSignalMulti:
    """Multi selection emits selection_changed with multiple paths."""

    def test_multi_selection_emits_multiple_paths(self, grid_with_model: tuple[ThumbnailGrid, ThumbnailModel]) -> None:
        """Selecting multiple items emits selection_changed with all selected paths."""
        grid, model = grid_with_model

        emitted = []
        grid.selection_changed.connect(lambda paths: emitted.append(paths))

        sel_model = grid.selectionModel()
        for row in range(3):
            index = model.index(row)
            selection = QItemSelection(index, index)
            sel_model.select(selection, sel_model.SelectionFlag.Select)

        assert len(emitted) >= 1
        last_emission = emitted[-1]
        assert len(last_emission) == 3
        assert str(Path("/fake/images/photo_001.png")) in last_emission
        assert str(Path("/fake/images/photo_002.jpg")) in last_emission
        assert str(Path("/fake/images/photo_003.png")) in last_emission


class TestSelectionChangedSignalEmpty:
    """Clearing selection emits selection_changed with an empty list."""

    def test_no_selection_emits_empty_list(self, grid_with_model: tuple[ThumbnailGrid, ThumbnailModel]) -> None:
        """Clearing selection emits selection_changed with an empty list."""
        grid, model = grid_with_model

        index = model.index(0)
        selection = QItemSelection(index, index)
        grid.selectionModel().select(selection, grid.selectionModel().SelectionFlag.ClearAndSelect)

        emitted = []
        grid.selection_changed.connect(lambda paths: emitted.append(paths))

        grid.selectionModel().clear()

        assert len(emitted) >= 1
        last_emission = emitted[-1]
        assert last_emission == []
