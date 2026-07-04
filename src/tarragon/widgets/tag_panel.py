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
from PySide6.QtGui import QContextMenuEvent
from PySide6.QtWidgets import (
    QCheckBox,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QMenu,
    QMessageBox,
    QPushButton,
    QScrollArea,
    QVBoxLayout,
    QWidget,
)

from tarragon.services.tag_service import TagService
from tarragon.theme.color_buckets import BUCKET_COLORS, BUCKET_HEX_COLORS
from tarragon.theme.spacing import SM

# Re-export under the legacy name used by the sort-key helper.
COLOR_ORDER: list[str] = list(BUCKET_COLORS)


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
        self._cached_file_tags: dict[str, set[int]] = {}
        self._tag_checkboxes: dict[int, QCheckBox] = {}
        self._setting_checkboxes = False
        self._global_scope: bool = False  # False = local (default), True = global
        self._folder_path: str = ""  # current folder for local-scoped counts

        # ── Layout ──────────────────────────────────────────────────────
        layout = QVBoxLayout(self)
        layout.setContentsMargins(SM, SM, SM, SM)

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

        Tag IDs are batch-fetched once via
        :meth:`TagService.get_file_tag_ids_batch` to avoid redundant
        database queries when resolving tri-state for every tag.
        """
        self._selected_paths = paths
        self._cached_file_tags = self._tag_service.get_file_tag_ids_batch(paths)
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
            # Re-fetch cached tags since the DB may have changed
            self._cached_file_tags = self._tag_service.get_file_tag_ids_batch(self._selected_paths)
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
            # TODO: #888 has no matching color token — generic swatch border grey.
            swatch.setStyleSheet(f"background-color: {hex_color}; border: 1px solid #888;")
            row_layout.addWidget(swatch)

            # TODO: #555 has no matching color token — generic auto-color row border grey.
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
        return BUCKET_HEX_COLORS.get(color_name, "#888888")

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
            try:
                checkbox = self._tag_checkboxes.get(tag_id)
                if checkbox is not None:
                    checkbox.setCheckState(Qt.CheckState.Checked)
            finally:
                self._setting_checkboxes = False
            self.toggle_tag(tag_id, True)
        elif check_state == Qt.CheckState.Checked:
            self.toggle_tag(tag_id, True)
        elif check_state == Qt.CheckState.Unchecked:
            self.toggle_tag(tag_id, False)

    def _update_checkbox_states(self) -> None:
        """Sync every checkbox's state with the current file selection.

        Uses ``tag_service.resolve_tri_state`` per tag, passing the
        pre-fetched ``_cached_file_tags`` so that only a single batch
        query is needed instead of one query per tag.
        While updating, ``_setting_checkboxes`` is ``True`` so that the
        ``stateChanged`` handler does not treat these as user actions.
        """
        self._setting_checkboxes = True
        try:
            for tag_id, checkbox in self._tag_checkboxes.items():
                state = self._tag_service.resolve_tri_state(
                    self._selected_paths,
                    tag_id,
                    self._cached_file_tags,
                )
                checkbox.setCheckState(state)
        finally:
            self._setting_checkboxes = False

    def _on_add_tag_clicked(self) -> None:
        """Create a new tag from the input text, then clear the input."""
        name = self._tag_input.text().strip()
        if name:
            self.create_new_tag(name)
            self._tag_input.clear()

    # ── Context menu (tag deletion) ──────────────────────────────

    def contextMenuEvent(self, event: QContextMenuEvent) -> None:  # noqa: N802
        """Show a context menu with 'Delete' for custom (non-color) tags."""
        # Map event position to scroll area viewport coordinates
        viewport = self._scroll_area.viewport()
        viewport_pos = viewport.mapFrom(self, event.pos())
        widget = viewport.childAt(viewport_pos)
        tag_id = self._get_tag_id_from_widget(widget)

        if tag_id is None:
            return

        if self._is_color_tag(tag_id):
            return

        menu = QMenu(self)
        delete_action = menu.addAction("Delete")

        action = menu.exec(event.globalPos())
        if action == delete_action:
            self._confirm_and_delete_tag(tag_id)

    def _get_tag_id_from_widget(self, widget: QWidget | None) -> int | None:
        """Walk up the parent chain from *widget* to find a tag_id property.

        The tag_id is stored as a dynamic property on the ``QCheckBox``
        inside each tag row.  This method traverses upward from the
        clicked widget (which may be the checkbox, label, or swatch)
        until it finds a sibling/child checkbox with a ``tag_id``.
        """
        if widget is None:
            return None

        # Walk up the parent chain looking for a QCheckBox with tag_id
        current: QWidget | None = widget
        while current is not None and current is not self:
            if isinstance(current, QCheckBox):
                tag_id = current.property("tag_id")
                if tag_id is not None:
                    return int(tag_id)
            # Check children of the current widget for a checkbox with tag_id
            for child in current.findChildren(QCheckBox):
                tag_id = child.property("tag_id")
                if tag_id is not None:
                    return int(tag_id)
            current = current.parentWidget()
        return None

    def _is_color_tag(self, tag_id: int) -> bool:
        """Return True if the tag is a color tag (name starts with 'color:')."""
        tag_name = self._tag_service.get_tag_name(tag_id)
        if tag_name is None:
            return False
        return tag_name.startswith("color:")

    def _confirm_and_delete_tag(self, tag_id: int) -> None:
        """Show a confirmation dialog and delete the tag if confirmed."""
        tag_name = self._tag_service.get_tag_name(tag_id)
        if tag_name is None:
            return

        reply = QMessageBox.question(
            self,
            "Delete Tag",
            f"Delete tag '{tag_name}'?\n\nThis will remove the tag from all images.",
            QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No,
            QMessageBox.StandardButton.No,
        )
        if reply == QMessageBox.StandardButton.Yes:
            self._tag_service.delete_tag(tag_id)
