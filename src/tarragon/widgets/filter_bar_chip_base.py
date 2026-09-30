"""Shared base class for chip-style filter bars (tags and folders).

Both ``FilterBarTag`` and ``FilterBarFolder`` previously duplicated the
add-button + chips-container scaffold, the checkable menu loop, and the
toggle/remove/emit machinery.  This base class captures that shared
behaviour while leaving the data source and active-item type to the
subclasses.
"""

from __future__ import annotations

import logging
from typing import Generic, TypeVar

from PySide6.QtCore import Qt
from PySide6.QtWidgets import QHBoxLayout, QMenu, QPushButton, QWidget

from tarragon.theme.constants import SPACING_S
from tarragon.widgets.filter_bar_filter import FilterBarFilter

logger = logging.getLogger(__name__)

T = TypeVar("T")


# Generic[T] (not PEP 695 type parameters) is required here: sip's metaclass
# for QWidget subclasses conflicts with PEP 695 generic classes at runtime.
# ABC is likewise omitted -- its metaclass conflicts with sip's QWidget
# metaclass -- so subclass contracts are enforced by the NotImplementedError
# stubs below.
class _ChipFilterBarBase(FilterBarFilter, Generic[T]):  # noqa: UP046
    """Shared scaffold for chip-based filter bars.

    Owns the Add-Button + chips-container layout, the checkable menu loop,
    the toggle/remove selection core, incremental chip syncing, and the
    standard ``has_active_filters``/``clear_filters``/``active_items`` API.

    Subclasses supply:
      - ``_load_items()``: the menu's available items (or ``None`` when the
        data source is unavailable, in which case no menu is shown).
      - ``_item_label()``/``_item_data()``: menu action text/data mapping.
      - ``_create_chip()``: the removable chip widget for an item.
    """

    def __init__(
        self,
        button_text: str,
        *,
        button_tooltip: str | None = None,
        initially_hidden: bool = False,
        parent: QWidget | None = None,
    ) -> None:
        """Build the Add-Button + chips-container layout scaffold.

        Args:
            button_text: Label for the menu-opening button.
            button_tooltip: Optional tooltip for the button.
            initially_hidden: Hide the button on construction (folder bars
                only appear in global scope).
            parent: Optional parent widget.
        """
        super().__init__(parent)
        self._active: set[T] = set()
        self._chips: dict[T, QWidget] = {}

        layout = QHBoxLayout(self)
        layout.setContentsMargins(SPACING_S, 0, SPACING_S, 0)
        layout.setSpacing(SPACING_S)

        # Add button
        self._add_button = QPushButton(button_text)
        self._add_button.setCursor(Qt.CursorShape.PointingHandCursor)
        if button_tooltip is not None:
            self._add_button.setToolTip(button_tooltip)
        self._add_button.clicked.connect(self._show_menu)
        layout.addWidget(self._add_button)

        # Container for active chips
        self._chips_container = QWidget()
        self._chips_layout = QHBoxLayout(self._chips_container)
        self._chips_layout.setContentsMargins(0, 0, 0, 0)
        self._chips_layout.setSpacing(SPACING_S)
        layout.addWidget(self._chips_container)

        layout.addStretch()

        # Checkable menu
        self._menu = QMenu(self)

        if initially_hidden:
            self._add_button.hide()

        # Match the chips container to the (empty) active set.
        self._update_chips()

    # --- Subclass hooks -------------------------------------------------

    def _load_items(self) -> list[T] | None:
        """Return the items for the menu, or ``None`` when unavailable.

        Subclasses must override this.  Raising ``NotImplementedError``
        satisfies the interface but makes every menu interaction fail, so
        real subclasses always provide an implementation.
        """
        raise NotImplementedError

    def _item_label(self, item: T) -> str:
        """Return the display label for a menu item (subclass contract)."""
        raise NotImplementedError

    def _item_data(self, item: T) -> object:
        """Return the value stored on the menu action (defaults to item)."""
        return item

    def _create_chip(self, item: T) -> QWidget:
        """Create the removable chip widget for *item* (subclass contract)."""
        raise NotImplementedError

    # --- Menu -------------------------------------------------------------

    def _show_menu(self) -> None:
        """Show the checkable menu of available items below the add button."""
        self._menu.clear()

        items = self._load_items()
        if items is None:
            return

        for item in items:
            action = self._menu.addAction(self._item_label(item))
            action.setCheckable(True)
            checked = item in self._active
            action.setChecked(checked)
            action.setData(self._item_data(item))
            action.triggered.connect(lambda checked=checked, item=item: self._toggle_item(item))

        # Show menu below the button
        pos = self._add_button.mapToGlobal(self._add_button.rect().bottomLeft())
        self._menu.popup(pos)

    # --- Selection core ---------------------------------------------------

    def _toggle_item(self, item: T) -> None:
        """Toggle *item* in the active set, syncing chips and emitting."""
        if item in self._active:
            self._remove_item(item)
        else:
            self._active.add(item)
            chip = self._create_chip(item)
            self._chips[item] = chip
            self._chips_layout.addWidget(chip)
            self._update_chips()
            self._emit_signal(set(self._active))

    def _remove_item(self, item: T) -> None:
        """Remove *item* from the active set, drop its chip, and emit."""
        self._active.discard(item)
        chip = self._chips.pop(item, None)
        if chip is not None:
            self._chips_layout.removeWidget(chip)
            chip.deleteLater()
        self._update_chips()
        self._emit_signal(set(self._active))

    def _discard_item_silently(self, item: T) -> None:
        """Drop *item* and its chip without emitting (data-driven pruning)."""
        self._active.discard(item)
        chip = self._chips.pop(item, None)
        if chip is not None:
            self._chips_layout.removeWidget(chip)
            chip.deleteLater()

    # --- Chips -------------------------------------------------------------

    def _update_chips(self) -> None:
        """Show the chips container while filters are active; hide when empty.

        Chips themselves are maintained incrementally by ``_toggle_item``/
        ``_remove_item`` rather than being rebuilt on every change.
        """
        self._chips_container.setVisible(bool(self._active))

    # --- Standard API ------------------------------------------------------

    def has_active_filters(self) -> bool:
        """Return True while any filter is active."""
        return len(self._active) > 0

    def clear_filters(self) -> None:
        """Remove all active filters and their chips, then emit an empty set."""
        for item in list(self._active):
            chip = self._chips.pop(item, None)
            if chip is not None:
                self._chips_layout.removeWidget(chip)
                chip.deleteLater()
        self._active.clear()
        self._update_chips()
        self._emit_signal(set())

    def active_items(self) -> set[T]:
        """Return a copy of the currently active items."""
        return set(self._active)
