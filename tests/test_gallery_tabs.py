"""Tests for GalleryTabs widget."""

from __future__ import annotations

from typing import Any

from tarragon.widgets.gallery_tabs import GalleryTabs


class TestGalleryTabsCreation:
    """GalleryTabs construction and basic structure."""

    def test_creation(self, qapp: Any) -> None:  # noqa: ARG002
        """GalleryTabs is created without error."""
        tabs = GalleryTabs()
        assert tabs is not None

    def test_has_two_tabs(self, qapp: Any) -> None:  # noqa: ARG002
        """GalleryTabs has exactly two tabs."""
        tabs = GalleryTabs()
        assert tabs.count() == 2

    def test_tab_labels(self, qapp: Any) -> None:  # noqa: ARG002
        """Tab labels are 'Folder' and 'All Images'."""
        tabs = GalleryTabs()
        assert tabs.tabText(0) == "Folder"
        assert tabs.tabText(1) == "All Images"

    def test_default_is_folder(self, qapp: Any) -> None:  # noqa: ARG002
        """Default active tab is Folder (index 0), not global."""
        tabs = GalleryTabs()
        assert tabs.currentIndex() == 0
        assert tabs.is_global_scope() is False


class TestGalleryTabsScope:
    """GalleryTabs scope switching and signal emission."""

    def test_is_global_scope_folder(self, qapp: Any) -> None:  # noqa: ARG002
        """Folder tab active means is_global_scope() returns False."""
        tabs = GalleryTabs()
        tabs.setCurrentIndex(0)
        assert tabs.is_global_scope() is False

    def test_is_global_scope_all_images(self, qapp: Any) -> None:  # noqa: ARG002
        """All Images tab active means is_global_scope() returns True."""
        tabs = GalleryTabs()
        tabs.setCurrentIndex(1)
        assert tabs.is_global_scope() is True

    def test_scope_changed_signal(self, qapp: Any) -> None:  # noqa: ARG002
        """Switching to All Images emits scope_changed(True)."""
        tabs = GalleryTabs()
        signals: list[bool] = []
        tabs.scope_changed.connect(lambda v: signals.append(v))
        tabs.setCurrentIndex(1)
        assert signals == [True]

    def test_scope_changed_on_switch_back(self, qapp: Any) -> None:  # noqa: ARG002
        """Switching back to Folder emits scope_changed(False)."""
        tabs = GalleryTabs()
        signals: list[bool] = []
        tabs.scope_changed.connect(lambda v: signals.append(v))
        tabs.setCurrentIndex(1)
        tabs.setCurrentIndex(0)
        assert signals == [True, False]
