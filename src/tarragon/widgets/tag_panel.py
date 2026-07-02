"""TagPanel — Manual tag management widget with tri-state checkboxes.

Provides a scrollable list of all known tags, each shown as a tri-state
checkbox.  The panel also exposes a text field to create new tags on the
fly.

Design patterns (from python-design-patterns skill):
  - Service Layer     — ``TagPanel`` uses ``TagService``, never touches DB
  - Strategy pattern  — Tri-state resolution delegated to ``TagService``
"""

from __future__ import annotations

from typing import Any

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
    QCheckBox,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QPushButton,
    QScrollArea,
    QVBoxLayout,
    QWidget,
)

from tarragon.services.tag_service import TagService

# Canonical color-wheel order matching ColorFilterBar.BUCKET_HUES.
COLOR_ORDER: list[str] = [
    "red",
    "orange",
    "yellow",
    "green",
    "teal",
    "cyan",
    "blue",
    "purple",
    "magenta",
    "neutral",
]


class TagPanel(QWidget):
    """A widget showing all known tags with tri-state checkboxes.

    The user can:
      * See every tag in the system, each with a tri-state checkbox.
      * Click a checkbox to add/remove the tag from the currently
        selected files.
      * Type a new tag name and press *Add Tag* to create it.

    Emits no signals; scope is controlled externally via ``set_global_scope()``.
    """

    def __init__(self, tag_service: TagService, parent: QWidget | None = None) -> None:
        """Build the tag panel with header, scrollable list, and new-tag input."""
        super().__init__(parent)
        self._tag_service = tag_service
        self._selected_paths: list[str] = []
        self._tag_checkboxes: dict[int, QCheckBox] = {}
        self._setting_checkboxes = False
        self._global_scope: bool = False  # False = local (default), True = global
        self._folder_path: str = ""  # current folder for local-scoped counts

        # ── Layout ──────────────────────────────────────────────────────
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)

        # Header
        header = QLabel("Tags")
        layout.addWidget(header)

        # Scroll area for tag checkboxes
        self._scroll_area = QScrollArea()
        scroll_widget = QWidget()
        self._scroll_layout = QVBoxLayout(scroll_widget)
        self._scroll_layout.setContentsMargins(0, 0, 0, 0)
        self._scroll_area.setWidget(scroll_widget)
        self._scroll_area.setWidgetResizable(True)
        layout.addWidget(self._scroll_area, stretch=1)

        # New tag input row
        input_layout = QHBoxLayout()
        self._tag_input = QLineEdit()
        self._tag_input.setPlaceholderText("New tag name...")
        self._add_button = QPushButton("Add Tag")
        self._add_button.clicked.connect(self._on_add_tag_clicked)
        input_layout.addWidget(self._tag_input)
        input_layout.addWidget(self._add_button)
        layout.addLayout(input_layout)

        # React to external tag changes
        self._tag_service.tagsChanged.connect(self._refresh_tags)

        # Initial tag load
        self._refresh_tags()

    # ── Public API ──────────────────────────────────────────────────────

    def set_selection(self, paths: list[str]) -> None:
        """Update the tri-state display based on which files are selected.

        For each tag, ``tag_service.resolve_tri_state(paths, tag_id)`` is
        used to decide whether the checkbox should be ``Checked``,
        ``PartiallyChecked``, or ``Unchecked``.  The *paths* are stored so
        that subsequent ``toggle_tag`` calls know which files to act on.
        """
        self._selected_paths = paths
        self._update_checkbox_states()

    def set_global_scope(self, is_global: bool) -> None:
        """Set scope from gallery tabs. Refreshes tag counts.

        Args:
            is_global: True for global scope, False for folder-scoped.
        """
        self._global_scope = is_global
        self._refresh_tags()

    def set_folder_path(self, folder_path: str) -> None:
        """Set the current folder path for local-scoped tag counts.

        Also refreshes the tag list to update usage counts.
        """
        self._folder_path = folder_path
        self._refresh_tags()

    def create_new_tag(self, name: str) -> int:
        """Create a new tag via *tag_service* and refresh the UI.

        Returns the new tag id.
        """
        tag_id = self._tag_service.get_or_create_tag(name)
        self._refresh_tags()
        return tag_id

    def toggle_tag(self, tag_id: int, checked: bool) -> None:
        """Apply or remove a tag to/from all currently selected files.

        *checked* == ``True``  → ``add_tags_to_files(...)``
        *checked* == ``False`` → ``remove_tags_from_files(...)``

        The panel automatically refreshes via the ``tagsChanged`` signal
        emitted by the service.
        """
        paths = self._selected_paths
        if not paths:
            return

        # Look up the tag name from the service
        tag_name = None
        for tag in self._tag_service.get_all_tags():
            if tag["id"] == tag_id:
                tag_name = tag["name"]
                break

        if tag_name is None:
            return

        if checked:
            self._tag_service.add_tags_to_files(paths, [tag_name])
        else:
            self._tag_service.remove_tags_from_files(paths, {tag_id})

    # ── Internal helpers ───────────────────────────────────────────────

    @staticmethod
    def _tag_sort_key(tag: dict[str, Any]) -> tuple[int, int | str]:
        """Sort key: color tags first (in color-wheel order), then manual tags alphabetically.

        Color tags (``"color:<name>"``) are ordered according to
        ``COLOR_ORDER``.  Unknown color names sort after all known colors.
        Manual (non-color) tags sort alphabetically after all color tags.
        """
        tag_name: str = tag["name"]
        if tag_name.startswith("color:"):
            color = tag_name[6:]  # strip "color:" prefix
            try:
                return (0, COLOR_ORDER.index(color))
            except ValueError:
                return (0, len(COLOR_ORDER))  # unknown colors go last among colors
        return (1, tag_name)  # manual tags sorted alphabetically

    def _refresh_tags(self) -> None:
        """Rebuild the entire tag list from the service.

        Destroys any existing tag rows and creates fresh ones from
        ``tag_service.get_all_tags()``.  In local mode, usage counts are
        scoped to the current folder; in global mode, counts span the DB.
        """
        self._tag_checkboxes.clear()
        while self._scroll_layout.count():
            item = self._scroll_layout.takeAt(0)
            if item is not None:
                w = item.widget()
                if w is not None:
                    w.deleteLater()

        folder_scope = None if self._global_scope else (self._folder_path or None)
        tags = self._tag_service.get_all_tags(folder_path=folder_scope)
        sorted_tags = sorted(tags, key=self._tag_sort_key)
        for tag in sorted_tags:
            row = self._build_tag_row(tag)
            self._scroll_layout.addWidget(row)

        self._scroll_layout.addStretch()

        if self._selected_paths:
            self._update_checkbox_states()

    def _build_tag_row(self, tag: dict[str, Any]) -> QWidget:
        """Build a single row widget for one tag.

        The row contains a tri-state ``QCheckBox`` and a ``QLabel`` with
        the tag name and usage count.

        Auto-color tags (names starting with ``"color:"``) receive a small
        color swatch and a dashed border to visually distinguish them from
        manual tags.

        The tag id is stored as a Qt dynamic property (``"tag_id"``) on
        the checkbox so that it can be retrieved later.
        """
        row = QWidget()
        row_layout = QHBoxLayout(row)
        row_layout.setContentsMargins(0, 0, 0, 0)

        checkbox = QCheckBox()
        checkbox.setTristate(True)
        checkbox.setProperty("tag_id", tag["id"])

        tag_name = tag["name"]
        is_auto_color = tag_name.startswith("color:")

        if is_auto_color:
            # Extract color name (e.g., "color:red" -> "red")
            color_name = tag_name.split(":", 1)[1]

            # Create color swatch
            swatch = QLabel()
            swatch.setFixedSize(16, 16)
            swatch.setObjectName("colorSwatch")
            hex_color = self._get_color_hex(color_name)
            swatch.setStyleSheet(f"background-color: {hex_color}; border: 1px solid #888;")
            row_layout.addWidget(swatch)

            # Outlined/dashed row style for auto-color tags
            row.setStyleSheet("border: 1px dashed #555; border-radius: 2px; padding: 2px;")
            row.setToolTip("Auto color tag")

        label = QLabel(f"{tag_name} ({tag['usage_count']})")
        label.setProperty(
            "tagRole",
            "primary" if tag["usage_count"] > 0 else "secondary",
        )

        checkbox.stateChanged.connect(
            lambda state, tid=tag["id"]: self._on_checkbox_state_changed(tid, state),
        )

        row_layout.addWidget(checkbox)
        row_layout.addWidget(label, stretch=1)

        self._tag_checkboxes[tag["id"]] = checkbox
        return row

    @staticmethod
    def _get_color_hex(color_name: str) -> str:
        """Map color bucket name to hex color for swatch display.

        Returns a default grey (``#888888``) for unrecognised names.
        """
        color_map: dict[str, str] = {
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
        return color_map.get(color_name, "#888888")

    def _on_checkbox_state_changed(self, tag_id: int, state: int) -> None:
        """Handle a change in a tag checkbox's state.

        When the change is **user-initiated** (i.e. not a programmatic
        update) the method:

        * Overrides ``PartiallyChecked`` → ``Checked`` so that clicking
          a partial checkbox always *adds* the tag.
        * Calls ``toggle_tag`` to apply/remove the tag.

        Note: The ``stateChanged`` signal passes an ``int`` (not a
        ``Qt.CheckState`` enum), so we convert via ``Qt.CheckState()``
        before comparing.
        """
        if self._setting_checkboxes:
            return

        check_state = Qt.CheckState(state)

        if check_state == Qt.CheckState.PartiallyChecked:
            # User clicked on a PartiallyChecked checkbox → treat as Checked
            self._setting_checkboxes = True
            checkbox = self._tag_checkboxes.get(tag_id)
            if checkbox is not None:
                checkbox.setCheckState(Qt.CheckState.Checked)
            self._setting_checkboxes = False
            self.toggle_tag(tag_id, True)
        elif check_state == Qt.CheckState.Checked:
            self.toggle_tag(tag_id, True)
        elif check_state == Qt.CheckState.Unchecked:
            self.toggle_tag(tag_id, False)

    def _update_checkbox_states(self) -> None:
        """Sync every checkbox's state with the current file selection.

        Uses ``tag_service.resolve_tri_state`` per tag.
        While updating, ``_setting_checkboxes`` is ``True`` so that the
        ``stateChanged`` handler does not treat these as user actions.
        """
        self._setting_checkboxes = True
        for tag_id, checkbox in self._tag_checkboxes.items():
            state = self._tag_service.resolve_tri_state(self._selected_paths, tag_id)
            checkbox.setCheckState(state)
        self._setting_checkboxes = False

    def _on_add_tag_clicked(self) -> None:
        """Create a new tag from the input text, then clear the input."""
        name = self._tag_input.text().strip()
        if name:
            self.create_new_tag(name)
            self._tag_input.clear()
