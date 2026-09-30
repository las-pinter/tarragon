"""Indeterminate busy status strip shown while background operations run."""

from __future__ import annotations

from PySide6.QtWidgets import QHBoxLayout, QLabel, QProgressBar, QWidget

from tarragon.theme.constants import SPACING_XS


class BusyIndicator(QWidget):
    """A thin non-modal strip (label + indeterminate bar) hidden when idle.

    Uses an operation ref-count so overlapping operations (e.g. a folder
    scan racing a multi-select decode) keep the indicator visible until
    the last one finishes. The strip never steals focus and occupies no
    space while hidden.
    """

    def __init__(self, parent: QWidget | None = None) -> None:
        """Initialize the hidden strip with a label and an indeterminate bar."""
        super().__init__(parent)
        self._active_ops = 0

        layout = QHBoxLayout(self)
        layout.setContentsMargins(SPACING_XS, 0, SPACING_XS, 0)
        layout.setSpacing(SPACING_XS)

        self._label = QLabel("Working...")
        self._label.setObjectName("busyIndicatorLabel")
        layout.addWidget(self._label)

        self._bar = QProgressBar()
        self._bar.setObjectName("busyIndicatorBar")
        self._bar.setRange(0, 0)  # indeterminate animation
        self._bar.setFixedHeight(6)
        self._bar.setTextVisible(False)
        layout.addWidget(self._bar, stretch=1)

        self.hide()

    @property
    def is_busy(self) -> bool:
        """True while at least one operation is active."""
        return self._active_ops > 0

    def begin_op(self) -> None:
        """Mark one operation as started; show the strip when the first starts."""
        self._active_ops += 1
        if self._active_ops == 1:
            self.show()

    def end_op(self) -> None:
        """Mark one operation as finished; hide the strip when the last ends."""
        if self._active_ops > 0:
            self._active_ops -= 1
        if self._active_ops == 0:
            self.hide()
