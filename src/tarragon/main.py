"""Tarragon application entry point."""

import sys

from PySide6.QtWidgets import QApplication, QMainWindow


class MainWindow(QMainWindow):
    """Minimal main window stub — docks and features added in later milestones."""

    def __init__(self) -> None:
        super().__init__()
        self.setWindowTitle("Tarragon")
        self.resize(1200, 800)


def main() -> None:
    """Application entry point."""
    app = QApplication(sys.argv)
    app.setOrganizationName("tarragon")
    app.setApplicationName("tarragon")

    window = MainWindow()
    window.show()

    sys.exit(app.exec())


if __name__ == "__main__":
    main()
