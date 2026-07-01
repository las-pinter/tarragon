"""TagFilterBar — dropdown tag filter widget for the gallery top bar.

Provides a compact toggle button that opens a popup with tag checkboxes.
Selected tags are combined into a filter set emitted via ``tag_filter_changed``.

Design patterns:
    - Observer pattern  — ``tag_filter_changed`` signal for filter changes
    - Service Layer     — uses ``TagService``, never touches DB directly
"""

from __future__ import annotations

from typing import Any

from PySide6.QtCore import Qt, Signal
from PySide6.QtWidgets import (
    QCheckBox,
    QFrame,
    QHBoxLayout,
    QLabel,
    QPushButton,
    QScrollArea,
    QVBoxLayout,
    QWidget,
)

from tarragon.services.tag_service import TagService


class TagFilterBar(QWidget):
    """A compact tag filter widget with a dropdown popup of tag checkboxes.

    Emits ``tag_filter_changed(set)`` whenever the set of checked tag IDs
    changes.  The payload is a ``set[int]`` of active filter tag IDs.
    """

    tag_filter_changed = Signal(set)  # set of int — active filter tag IDs

    def __init__(self, tag_service: TagService, parent: QWidget | None = None) -> None:
        """Build the filter bar with a toggle button and hidden popup panel."""
        super().__init__(parent)
        self._tag_service = tag_service
        self._active_tag_ids: set[int] = set()
        self._tag_checkboxes: dict[int, QCheckBox] = {}
        self._popup_visible = False

        # ── Layout ──────────────────────────────────────────────────────
        outer_layout = QHBoxLayout(self)
        outer_layout.setContentsMargins(0, 0, 0, 0)

        # Toggle button — shows/hides the popup
        self._toggle_btn = QPushButton("Tags")
        self._toggle_btn.setCheckable(True)
        self._toggle_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        self._toggle_btn.toggled.connect(self._on_toggle_popup)
        outer_layout.addWidget(self._toggle_btn)

        # Spacer so the bar takes minimal space when no tags are selected
        outer_layout.addStretch()

        # ── Popup panel (hidden by default) ─────────────────────────────
        self._popup = QFrame()
        self._popup.setFrameShape(QFrame.Shape.StyledPanel)
        self._popup.setFrameShadow(QFrame.Shadow.Raised)
        self._popup.setVisible(False)

        popup_layout = QVBoxLayout(self._popup)
        popup_layout.setContentsMargins(4, 4, 4, 4)

        popup_header = QLabel("Filter by tag:")
        popup_layout.addWidget(popup_header)

        # Scroll area for tag checkboxes
        self._scroll_area = QScrollArea()
        self._scroll_area.setWidgetResizable(True)
        self._scroll_area.setMaximumHeight(200)
        scroll_widget = QWidget()
        self._scroll_layout = QVBoxLayout(scroll_widget)
        self._scroll_layout.setContentsMargins(0, 0, 0, 0)
        self._scroll_area.setWidget(scroll_widget)
        popup_layout.addWidget(self._scroll_area)

        # Place popup below the toggle button via the parent layout
        # We add it to the outer layout so it's positioned relative to this widget
        outer_layout.addWidget(self._popup)

        # React to external tag changes (new tags created, tags deleted)
        self._tag_service.tagsChanged.connect(self._refresh_tags)

        # Initial tag load
        self._refresh_tags()

    # ── Public API ──────────────────────────────────────────────────────

    def get_active_tag_ids(self) -> set[int]:
        """Return the set of currently active (checked) tag IDs."""
        return set(self._active_tag_ids)

    def has_active_filters(self) -> bool:
        """Return True if any tag filter checkbox is checked."""
        return len(self._active_tag_ids) > 0

    def clear_filters(self) -> None:
        """Uncheck all tag filter checkboxes and emit an empty set."""
        self._active_tag_ids.clear()
        for checkbox in self._tag_checkboxes.values():
            checkbox.blockSignals(True)
            checkbox.setCheckState(Qt.CheckState.Unchecked)
            checkbox.blockSignals(False)
        self._update_button_label()
        self.tag_filter_changed.emit(set())

    # ── Internal helpers ───────────────────────────────────────────────

    def _on_toggle_popup(self, checked: bool) -> None:  # noqa: FBT001
        """Show or hide the tag checkbox popup."""
        self._popup_visible = checked
        self._popup.setVisible(checked)

    def _refresh_tags(self) -> None:
        """Rebuild the popup tag list from the service.

        Preserves the current active filter selections across rebuilds.
        """
        self._tag_checkboxes.clear()
        while self._scroll_layout.count():
            item = self._scroll_layout.takeAt(0)
            if item is not None:
                w = item.widget()
                if w is not None:
                    w.deleteLater()

        tags = self._tag_service.get_all_tags()
        for tag in tags:
            row = self._build_tag_row(tag)
            self._scroll_layout.addWidget(row)

        self._scroll_layout.addStretch()

    def _build_tag_row(self, tag: dict[str, Any]) -> QWidget:
        """Build a single row with a checkbox for one tag in the popup."""
        row = QWidget()
        row_layout = QHBoxLayout(row)
        row_layout.setContentsMargins(0, 0, 0, 0)

        checkbox = QCheckBox(f"{tag['name']} ({tag['usage_count']})")
        checkbox.setProperty("tag_id", tag["id"])

        # Restore checked state if this tag was previously active
        if tag["id"] in self._active_tag_ids:
            checkbox.setCheckState(Qt.CheckState.Checked)

        checkbox.stateChanged.connect(
            lambda state, tid=tag["id"]: self._on_checkbox_changed(tid, state),
        )

        row_layout.addWidget(checkbox)
        self._tag_checkboxes[tag["id"]] = checkbox
        return row

    def _on_checkbox_changed(self, tag_id: int, state: int) -> None:
        """Handle a checkbox state change in the popup."""
        check_state = Qt.CheckState(state)
        if check_state == Qt.CheckState.Checked:
            self._active_tag_ids.add(tag_id)
        else:
            self._active_tag_ids.discard(tag_id)

        self._update_button_label()
        self.tag_filter_changed.emit(set(self._active_tag_ids))

    def _update_button_label(self) -> None:
        """Update the toggle button text to reflect active filter count."""
        count = len(self._active_tag_ids)
        if count == 0:
            self._toggle_btn.setText("Tags")
        else:
            self._toggle_btn.setText(f"Tags ({count})")
