"""Integration tests for color tagger in the thumbnail pipeline.

WAAAGH! Wrenchbasha's integration tests — verifies dat color tags flow
from render → color extraction → DB persistence without breakin' anything!
"""

from __future__ import annotations

from collections.abc import Generator
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest
from PIL import Image
from tarragon.db import Database
from tarragon.scanner import FileInfo
from tarragon.services.thumbnail_service import ThumbnailService

# =========================================================================
# Fixtures
# =========================================================================


@pytest.fixture(autouse=True)
def qapp() -> Generator[object, None, None]:
    """Provide a shared QApplication instance for Qt-based tests."""
    from PySide6.QtWidgets import QApplication

    app = QApplication.instance()
    if app is None:
        app = QApplication(["test"])
    yield app


@pytest.fixture()
def db() -> Generator[Database, None, None]:
    """Provide an in-memory database for each test (isolated)."""
    conn = Database(Path(":memory:"))
    conn.init_schema()
    yield conn
    conn.close()


@pytest.fixture()
def settings_mock() -> MagicMock:
    """Mock Settings with color tagging enabled and default parameters."""
    mock = MagicMock()

    def _get_side_effect(key: str):
        defaults = {
            "cache_format": "png",
            "color_tag_enabled": True,
            "color_tag_palette_size": 8,
            "color_tag_min_share": 0.10,
            "color_tag_neutral_s_threshold": 0.15,
        }
        return defaults.get(key)

    mock.get.side_effect = _get_side_effect
    return mock


@pytest.fixture()
def service(db: Database, settings_mock: MagicMock) -> ThumbnailService:
    """Create a ThumbnailService with real DB, mock settings, and mock threadpool."""
    svc = ThumbnailService(db=db, settings=settings_mock)
    svc._threadpool = MagicMock()
    return svc


def _make_file_info(tmp_path: Path, name: str = "test.png") -> FileInfo:
    """Helper to create a FileInfo for testing."""
    return FileInfo(
        path=tmp_path / name,
        mtime=1000.0,
        size=500,
        extension=".png",
    )


def _make_image(width: int = 128, height: int = 64, color: tuple[int, int, int] = (255, 0, 0)) -> Image.Image:
    """Helper to create a test PIL image."""
    return Image.new("RGB", (width, height), color)  # type: ignore[arg-type]


def _get_file_tag_names(db: Database, path: str) -> list[tuple[str, str]]:
    """Helper to retrieve tag (name, source) pairs for a file path from the DB."""
    cursor = db._conn.execute(
        """
        SELECT t.name, ft.source
        FROM file_tags ft
        JOIN tags t ON ft.tag_id = t.id
        WHERE ft.path = ?
        ORDER BY t.name
        """,
        (path,),
    )
    return [(row["name"], row["source"]) for row in cursor.fetchall()]


# =========================================================================
# Test 1: Color tags extracted on render
# =========================================================================


class TestColorTagsExtractedOnRender:
    """After a successful render, color tags are extracted and persisted to DB."""

    def test_color_tags_extracted_on_render(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db: Database,
    ) -> None:
        """After _on_done, color tags from extract_dominant_color_tags are in the DB."""
        # Arrange
        file_info = _make_file_info(tmp_path)
        img = _make_image()
        cache_path = tmp_path / "cache" / "hash.png"
        expected_tags = ["color:blue", "color:red"]

        emitted: list[tuple[str, object]] = []
        service.thumbnailReady.connect(lambda p, i: emitted.append((p, i)))

        with patch(
            "tarragon.color_tagger.extract_dominant_color_tags",
            return_value=expected_tags,
        ) as mock_extract:
            # Act
            service._on_done(file_info, img, cache_path)

        # Assert — tags persisted in DB
        tag_entries = _get_file_tag_names(db, str(file_info.path))
        tag_names = [name for name, _ in tag_entries]
        assert "color:red" in tag_names
        assert "color:blue" in tag_names

        # Assert — all tags have source='auto_color'
        for _, source in tag_entries:
            assert source == "auto_color"

        # Assert — extract was called with the image
        mock_extract.assert_called_once()
        assert mock_extract.call_args[0][0] is img

        # Assert — thumbnailReady still emitted
        assert len(emitted) == 1
        assert emitted[0][0] == str(file_info.path)
        assert emitted[0][1] is img


# =========================================================================
# Test 2: Color tags replaced on re-render
# =========================================================================


class TestColorTagsReplacedOnRerender:
    """Old auto_color tags are replaced when a file is re-rendered."""

    def test_color_tags_replaced_on_rerender(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db: Database,
    ) -> None:
        """Re-rendering replaces old auto_color tags with new ones."""
        # Arrange
        file_info = _make_file_info(tmp_path)
        img = _make_image()
        cache_path = tmp_path / "cache" / "hash.png"

        # First render — produces red and green tags
        with patch(
            "tarragon.color_tagger.extract_dominant_color_tags",
            return_value=["color:green", "color:red"],
        ):
            service._on_done(file_info, img, cache_path)

        # Verify first render tags
        first_tags = _get_file_tag_names(db, str(file_info.path))
        first_names = [name for name, _ in first_tags]
        assert "color:red" in first_names
        assert "color:green" in first_names

        # Act — second render produces different tags (blue and yellow)
        with patch(
            "tarragon.color_tagger.extract_dominant_color_tags",
            return_value=["color:blue", "color:yellow"],
        ):
            service._on_done(file_info, img, cache_path)

        # Assert — old tags gone, new tags present
        final_tags = _get_file_tag_names(db, str(file_info.path))
        final_names = [name for name, _ in final_tags]
        assert "color:red" not in final_names, "Old auto_color tag should be removed"
        assert "color:green" not in final_names, "Old auto_color tag should be removed"
        assert "color:blue" in final_names, "New auto_color tag should be present"
        assert "color:yellow" in final_names, "New auto_color tag should be present"


# =========================================================================
# Test 3: Manual tags preserved
# =========================================================================


class TestManualTagsPreserved:
    """Manual (user) tags are never affected by auto-color replacement."""

    def test_manual_tags_preserved(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db: Database,
    ) -> None:
        """Manual tags survive auto-color tag replacement."""
        # Arrange — manually add a user tag to the file
        file_info = _make_file_info(tmp_path)
        img = _make_image()
        cache_path = tmp_path / "cache" / "hash.png"
        path_str = str(file_info.path)

        # Insert a manual (user) tag
        tag_id = db.ensure_tag("favorite")
        db.add_file_tags([path_str], tag_id, source="user")

        # Verify manual tag is there
        pre_tags = _get_file_tag_names(db, path_str)
        assert ("favorite", "user") in pre_tags

        # Act — render with auto color tags
        with patch(
            "tarragon.color_tagger.extract_dominant_color_tags",
            return_value=["color:red"],
        ):
            service._on_done(file_info, img, cache_path)

        # Assert — manual tag still present alongside auto_color tags
        post_tags = _get_file_tag_names(db, path_str)
        post_names_with_source = [(name, src) for name, src in post_tags]
        assert ("favorite", "user") in post_names_with_source, "Manual tag must be preserved"
        assert ("color:red", "auto_color") in post_names_with_source

        # Act again — re-render with different auto_color tags
        with patch(
            "tarragon.color_tagger.extract_dominant_color_tags",
            return_value=["color:blue"],
        ):
            service._on_done(file_info, img, cache_path)

        # Assert — manual tag STILL present, old auto_color replaced
        final_tags = _get_file_tag_names(db, path_str)
        final_with_source = [(name, src) for name, src in final_tags]
        assert ("favorite", "user") in final_with_source, "Manual tag must survive re-render"
        assert ("color:red", "auto_color") not in final_with_source, "Old auto_color should be gone"
        assert ("color:blue", "auto_color") in final_with_source, "New auto_color should be present"


# =========================================================================
# Test 4: Color tagging disabled
# =========================================================================


class TestColorTaggingDisabled:
    """When color_tag_enabled=False, no color tags are generated."""

    def test_color_tagging_disabled(
        self,
        tmp_path: Path,
        db: Database,
    ) -> None:
        """When color_tag_enabled is False, no color tags are extracted or persisted."""
        # Arrange — settings with color_tag_enabled=False
        settings_mock = MagicMock()

        def _get_side_effect(key: str):
            defaults = {
                "cache_format": "png",
                "color_tag_enabled": False,
            }
            return defaults.get(key)

        settings_mock.get.side_effect = _get_side_effect
        svc = ThumbnailService(db=db, settings=settings_mock)
        svc._threadpool = MagicMock()

        file_info = _make_file_info(tmp_path)
        img = _make_image()
        cache_path = tmp_path / "cache" / "hash.png"

        emitted: list[tuple[str, object]] = []
        svc.thumbnailReady.connect(lambda p, i: emitted.append((p, i)))

        # Act
        with patch("tarragon.color_tagger.extract_dominant_color_tags") as mock_extract:
            svc._on_done(file_info, img, cache_path)

        # Assert — extract was never called
        mock_extract.assert_not_called()

        # Assert — no tags in DB for this file
        tag_entries = _get_file_tag_names(db, str(file_info.path))
        assert tag_entries == [], "No color tags should be persisted when disabled"

        # Assert — thumbnailReady still emitted (pipeline not broken)
        assert len(emitted) == 1
        assert emitted[0][1] is img


# =========================================================================
# Test 5: Color tagging failure isolated
# =========================================================================


class TestColorTaggingFailureIsolated:
    """If color_tagger raises, the pipeline still works."""

    def test_color_tagging_failure_isolated(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db: Database,
    ) -> None:
        """If extract_dominant_color_tags raises, thumbnail is still persisted and signal emitted."""
        # Arrange
        file_info = _make_file_info(tmp_path)
        img = _make_image()
        cache_path = tmp_path / "cache" / "hash.png"

        emitted: list[tuple[str, object]] = []
        service.thumbnailReady.connect(lambda p, i: emitted.append((p, i)))

        with patch(
            "tarragon.color_tagger.extract_dominant_color_tags",
            side_effect=RuntimeError("Color extraction exploded!"),
        ):
            # Act — should NOT raise
            service._on_done(file_info, img, cache_path)

        # Assert — thumbnail was still persisted to DB
        thumb = db.get_thumbnail(str(file_info.path))
        assert thumb is not None, "Thumbnail must be persisted even if color tagging fails"
        assert thumb["width"] == 128
        assert thumb["height"] == 64

        # Assert — thumbnailReady still emitted
        assert len(emitted) == 1
        assert emitted[0][0] == str(file_info.path)
        assert emitted[0][1] is img

        # Assert — no color tags in DB (extraction failed)
        tag_entries = _get_file_tag_names(db, str(file_info.path))
        assert tag_entries == []

    def test_db_replace_failure_isolated(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db: Database,
    ) -> None:
        """If replace_auto_color_tags raises, thumbnail is still persisted and signal emitted."""
        # Arrange
        file_info = _make_file_info(tmp_path)
        img = _make_image()
        cache_path = tmp_path / "cache" / "hash.png"

        emitted: list[tuple[str, object]] = []
        service.thumbnailReady.connect(lambda p, i: emitted.append((p, i)))

        with (
            patch(
                "tarragon.color_tagger.extract_dominant_color_tags",
                return_value=["color:red"],
            ),
            patch.object(
                db,
                "replace_auto_color_tags",
                side_effect=RuntimeError("DB write failed!"),
            ),
        ):
            # Act — should NOT raise
            service._on_done(file_info, img, cache_path)

        # Assert — thumbnail was still persisted
        thumb = db.get_thumbnail(str(file_info.path))
        assert thumb is not None

        # Assert — signal still emitted
        assert len(emitted) == 1
        assert emitted[0][1] is img


# =========================================================================
# Test 6: Settings parameters used
# =========================================================================


class TestSettingsParametersUsed:
    """Verify palette_size, min_share, neutral_s_threshold are passed from settings."""

    def test_settings_parameters_used(
        self,
        tmp_path: Path,
        db: Database,
    ) -> None:
        """extract_dominant_color_tags receives the correct parameters from settings."""
        # Arrange — custom settings values
        settings_mock = MagicMock()

        def _get_side_effect(key: str):
            defaults = {
                "cache_format": "png",
                "color_tag_enabled": True,
                "color_tag_palette_size": 4,
                "color_tag_min_share": 0.25,
                "color_tag_neutral_s_threshold": 0.30,
            }
            return defaults.get(key)

        settings_mock.get.side_effect = _get_side_effect
        svc = ThumbnailService(db=db, settings=settings_mock)
        svc._threadpool = MagicMock()

        file_info = _make_file_info(tmp_path)
        img = _make_image()
        cache_path = tmp_path / "cache" / "hash.png"

        # Act
        with patch(
            "tarragon.color_tagger.extract_dominant_color_tags",
            return_value=["color:red"],
        ) as mock_extract:
            svc._on_done(file_info, img, cache_path)

        # Assert — extract called with correct parameters from settings
        mock_extract.assert_called_once_with(
            img,
            palette_size=4,
            min_share=0.25,
            neutral_s_threshold=0.30,
        )

    def test_default_settings_parameters(
        self,
        tmp_path: Path,
        service: ThumbnailService,
    ) -> None:
        """Default settings values are passed correctly to extract_dominant_color_tags."""
        # Arrange
        file_info = _make_file_info(tmp_path)
        img = _make_image()
        cache_path = tmp_path / "cache" / "hash.png"

        # Act
        with patch(
            "tarragon.color_tagger.extract_dominant_color_tags",
            return_value=["color:red"],
        ) as mock_extract:
            service._on_done(file_info, img, cache_path)

        # Assert — default parameter values from settings fixture
        mock_extract.assert_called_once_with(
            img,
            palette_size=8,
            min_share=0.10,
            neutral_s_threshold=0.15,
        )


# =========================================================================
# Edge case: None image or None cache_path skips color tagging
# =========================================================================


class TestColorTaggingSkippedForNullInputs:
    """Color tagging is skipped when image or cache_path is None."""

    def test_none_image_skips_color_tagging(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db: Database,
    ) -> None:
        """When img is None, color tagging is not attempted."""
        # Arrange
        file_info = _make_file_info(tmp_path)

        with patch("tarragon.color_tagger.extract_dominant_color_tags") as mock_extract:
            # Act
            service._on_done(file_info, None, None)

        # Assert
        mock_extract.assert_not_called()
        tag_entries = _get_file_tag_names(db, str(file_info.path))
        assert tag_entries == []

    def test_none_cache_path_skips_color_tagging(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db: Database,
    ) -> None:
        """When cache_path is None but img is valid, color tagging is not attempted."""
        # Arrange
        file_info = _make_file_info(tmp_path)
        img = _make_image()

        with patch("tarragon.color_tagger.extract_dominant_color_tags") as mock_extract:
            # Act
            service._on_done(file_info, img, None)

        # Assert
        mock_extract.assert_not_called()
        tag_entries = _get_file_tag_names(db, str(file_info.path))
        assert tag_entries == []
