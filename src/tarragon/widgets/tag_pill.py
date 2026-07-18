"""Tag pill widget — a clickable tag chip with hover-revealed remove button."""

from __future__ import annotations

from collections.abc import Callable

from PySide6.QtCore import QEvent, Qt, Signal
from PySide6.QtGui import QEnterEvent, QMouseEvent
from PySide6.QtWidgets import QHBoxLayout, QLabel, QPushButton, QWidget


class _ClickableLabel(QLabel):
    """A QLabel subclass dat emits a ``clicked`` signal on mouse press.

    Replaces da monkey-patched ``mousePressEvent`` approach wiv a proper
    Qt event-chain override, avoiding fragile instance-level patching.
    """

    clicked = Signal()

    def mousePressEvent(self, event: QMouseEvent) -> None:  # noqa: N802
        """Emit ``clicked`` an' propagate da event up da Qt chain."""
        self.clicked.emit()
        super().mousePressEvent(event)


class _TagPillWidget(QWidget):
    """A tag pill widget with a hover-revealed remove (×) button.

    Contains a QLabel for the tag name and a small QPushButton ("×") that
    is hidden by default and shown when the mouse enters the widget.
    Clicking the × button removes the tag; clicking the pill body toggles it.
    """

    def __init__(
        self,
        tag_name: str,
        on_remove: Callable[[], None],
        on_toggle: Callable[[], None],
        parent: QWidget | None = None,
    ) -> None:
        super().__init__(parent)
        self._tag_name = tag_name
        self._on_remove = on_remove
        self._on_toggle = on_toggle

        layout = QHBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(2)

        self._label = _ClickableLabel(tag_name)
        self._label.setCursor(Qt.CursorShape.PointingHandCursor)
        self._label.clicked.connect(self._on_toggle)
        layout.addWidget(self._label)

        self._remove_btn = QPushButton("×")
        self._remove_btn.setFixedSize(16, 16)
        self._remove_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        self._remove_btn.setObjectName("tagPillRemoveBtn")
        self._remove_btn.hide()
        self._remove_btn.clicked.connect(lambda _checked=False: self._on_remove())
        layout.addWidget(self._remove_btn)

        self.setMouseTracking(True)
        self.setCursor(Qt.CursorShape.PointingHandCursor)
        self._label.setMouseTracking(True)
        self._remove_btn.setMouseTracking(True)

    def text(self) -> str:
        """Return the tag name displayed by this pill."""
        return self._tag_name

    def enterEvent(self, event: QEnterEvent) -> None:  # noqa: N802
        """Show the remove button on hover."""
        self._remove_btn.show()
        super().enterEvent(event)

    def leaveEvent(self, event: QEvent) -> None:  # noqa: N802
        """Hide the remove button when mouse leaves."""
        self._remove_btn.hide()
        super().leaveEvent(event)
