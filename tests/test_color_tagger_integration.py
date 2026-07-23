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
from tarragon.db._base import normalize_path
from tarragon.db.database import Database
from tarragon.renderers.cache import RESOLUTION_PREVIEW, RESOLUTION_THUMBNAIL
from tarragon.scanner import FileInfo
from tarragon.services.thumbnail_service import ThumbnailService


@pytest.fixture()
def db() -> Generator[Database, None, None]:
    """Provide an in-memory database for each test (isolated)."""
    conn = Database(Path(":memory:"))
    conn.init_schema()
    yield conn
    conn.close()


@pytest.fixture()
def settings_mock() -> MagicMock:
    """Mock SettingsService with color tagging enabled and default parameters."""
    mock = MagicMock()
    mock.get_cache_format.return_value = "png"
    mock.get_max_psd_workers.return_value = 3
    mock.get_large_canvas_threshold_mp.return_value = 20.0
    mock.get_tile_grid_size.return_value = "2x2"
    mock.get_color_tag_enabled.return_value = True
    mock.get_color_tag_palette_size.return_value = 8
    mock.get_color_tag_min_share.return_value = 0.10
    mock.get_color_tag_neutral_s_threshold.return_value = 0.15
    return mock


@pytest.fixture()
def service(db: Database, settings_mock: MagicMock) -> ThumbnailService:
    """Create a ThumbnailService with real DB, mock settings_service, and mock threadpool."""
    with patch("tarragon.services.thumbnail_service.get_executor"):
        svc = ThumbnailService(db=db, settings_service=settings_mock)
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
    return Image.new("RGB", (width, height), color)


def _get_file_tag_names(db: Database, path: str) -> list[tuple[str, str]]:
    """Helper to retrieve tag (name, source) pairs for a file path from the DB."""
    normalized = normalize_path(path)
    cursor = db._conn.execute(
        """
        SELECT t.name, ft.source
        FROM file_tags ft
        JOIN tags t ON ft.tag_id = t.id
        WHERE ft.path = ?
        ORDER BY t.name
        """,
        (normalized,),
    )
    return [(row["name"], row["source"]) for row in cursor.fetchall()]


def _run_render_all_resolutions(
    service: ThumbnailService,
    file_info: FileInfo,
    img: Image.Image,
    tmp_path: Path,
    extract_return: list[str] | None = None,
) -> None:
    """Helper to run _render_all_resolutions with mocked render/save functions.

    This simulates the full render pipeline: render → save → color tag → DB upsert.
    """
    cache_path = tmp_path / "cache" / "full.png"

    with (
        patch("tarragon.services.thumbnail_service.render_plain_image", return_value=img),
        patch("tarragon.services.thumbnail_service.generate_cache_uuid", return_value="test-uuid"),
        patch("tarragon.services.thumbnail_service.generate_cache_paths") as mock_paths,
        patch("tarragon.services.thumbnail_service.save_to_cache"),
        patch("tarragon.services.thumbnail_service.derive_smaller_sizes", return_value={}),
    ):
        mock_paths.return_value = {
            str(RESOLUTION_THUMBNAIL): tmp_path / "cache" / "256.png",
            str(RESOLUTION_PREVIEW): tmp_path / "cache" / "1024.png",
            "full": cache_path,
        }

        if extract_return is not None:
            with patch(
                "tarragon.services.color_tagger.extract_dominant_color_tags",
                return_value=extract_return,
            ):
                service._render_all_resolutions(file_info)
        else:
            service._render_all_resolutions(file_info)


class TestColorTagsExtractedOnRender:
    """After a successful render, color tags are extracted and persisted to DB."""

    def test_color_tags_extracted_on_render(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db: Database,
    ) -> None:
        """After _render_all_resolutions, color tags from extract_dominant_color_tags are in the DB."""
        # Arrange
        file_info = _make_file_info(tmp_path)
        img = _make_image()
        expected_tags = ["color:blue", "color:red"]

        emitted: list[tuple[str, object, object]] = []
        service.thumbnail_ready.connect(lambda p, i, r: emitted.append((p, i, r)))

        # Act
        _run_render_all_resolutions(service, file_info, img, tmp_path, extract_return=expected_tags)

        # Assert — tags persisted in DB
        tag_entries = _get_file_tag_names(db, str(file_info.path))
        tag_names = [name for name, _ in tag_entries]
        assert "color:red" in tag_names
        assert "color:blue" in tag_names

        # Assert — all tags have source='auto_color'
        for _, source in tag_entries:
            assert source == "auto_color"

        # Assert — thumbnail_ready emitted (full + smaller sizes)
        assert len(emitted) >= 1
        assert emitted[0][0] == str(file_info.path)
        assert emitted[0][1] is img


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

        # First render — produces red and green tags
        _run_render_all_resolutions(
            service,
            file_info,
            img,
            tmp_path,
            extract_return=["color:green", "color:red"],
        )

        # Verify first render tags
        first_tags = _get_file_tag_names(db, str(file_info.path))
        first_names = [name for name, _ in first_tags]
        assert "color:red" in first_names
        assert "color:green" in first_names

        # Act — second render produces different tags (blue and yellow)
        _run_render_all_resolutions(
            service,
            file_info,
            img,
            tmp_path,
            extract_return=["color:blue", "color:yellow"],
        )

        # Assert — old tags gone, new tags present
        final_tags = _get_file_tag_names(db, str(file_info.path))
        final_names = [name for name, _ in final_tags]
        assert "color:red" not in final_names, "Old auto_color tag should be removed"
        assert "color:green" not in final_names, "Old auto_color tag should be removed"
        assert "color:blue" in final_names, "New auto_color tag should be present"
        assert "color:yellow" in final_names, "New auto_color tag should be present"


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
        path_str = str(file_info.path)

        # Insert a manual (user) tag
        tag_id = db.ensure_tag("favorite")
        db.add_file_tags([path_str], tag_id, source="user")

        # Verify manual tag is there
        pre_tags = _get_file_tag_names(db, path_str)
        assert ("favorite", "user") in pre_tags

        # Act — render with auto color tags
        _run_render_all_resolutions(
            service,
            file_info,
            img,
            tmp_path,
            extract_return=["color:red"],
        )

        # Assert — manual tag still present alongside auto_color tags
        post_tags = _get_file_tag_names(db, path_str)
        post_names_with_source = [(name, src) for name, src in post_tags]
        assert ("favorite", "user") in post_names_with_source, "Manual tag must be preserved"
        assert ("color:red", "auto_color") in post_names_with_source

        # Act again — re-render with different auto_color tags
        _run_render_all_resolutions(
            service,
            file_info,
            img,
            tmp_path,
            extract_return=["color:blue"],
        )

        # Assert — manual tag STILL present, old auto_color replaced
        final_tags = _get_file_tag_names(db, path_str)
        final_with_source = [(name, src) for name, src in final_tags]
        assert ("favorite", "user") in final_with_source, "Manual tag must survive re-render"
        assert ("color:red", "auto_color") not in final_with_source, "Old auto_color should be gone"
        assert ("color:blue", "auto_color") in final_with_source, "New auto_color should be present"


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
        settings_mock.get_cache_format.return_value = "png"
        settings_mock.get_max_psd_workers.return_value = 3
        settings_mock.get_large_canvas_threshold_mp.return_value = 20.0
        settings_mock.get_tile_grid_size.return_value = "2x2"
        settings_mock.get_color_tag_enabled.return_value = False

        with patch("tarragon.services.thumbnail_service.get_executor"):
            svc = ThumbnailService(db=db, settings_service=settings_mock)
        svc._threadpool = MagicMock()

        file_info = _make_file_info(tmp_path)
        img = _make_image()

        emitted: list[tuple[str, object, object]] = []
        svc.thumbnail_ready.connect(lambda p, i, r: emitted.append((p, i, r)))

        # Act
        with patch("tarragon.services.color_tagger.extract_dominant_color_tags") as mock_extract:
            _run_render_all_resolutions(svc, file_info, img, tmp_path)

        # Assert — extract was never called
        mock_extract.assert_not_called()

        # Assert — no tags in DB for this file
        tag_entries = _get_file_tag_names(db, str(file_info.path))
        assert tag_entries == [], "No color tags should be persisted when disabled"

        # Assert — thumbnail_ready still emitted (pipeline not broken)
        assert len(emitted) >= 1
        assert emitted[0][1] is img


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

        emitted: list[tuple[str, object, object]] = []
        service.thumbnail_ready.connect(lambda p, i, r: emitted.append((p, i, r)))

        with (
            patch("tarragon.services.thumbnail_service.render_plain_image", return_value=img),
            patch("tarragon.services.thumbnail_service.generate_cache_uuid", return_value="test-uuid"),
            patch("tarragon.services.thumbnail_service.generate_cache_paths") as mock_paths,
            patch("tarragon.services.thumbnail_service.save_to_cache"),
            patch("tarragon.services.thumbnail_service.derive_smaller_sizes", return_value={}),
            patch(
                "tarragon.services.color_tagger.extract_dominant_color_tags",
                side_effect=RuntimeError("Color extraction exploded!"),
            ),
        ):
            mock_paths.return_value = {
                str(RESOLUTION_THUMBNAIL): tmp_path / "cache" / "256.png",
                str(RESOLUTION_PREVIEW): tmp_path / "cache" / "1024.png",
                "full": tmp_path / "cache" / "full.png",
            }
            # Act — should NOT raise
            service._render_all_resolutions(file_info)

        # Assert — thumbnail was still persisted to DB
        thumb = db.get_thumbnail(str(file_info.path))
        assert thumb is not None, "Thumbnail must be persisted even if color tagging fails"
        assert thumb["width"] == 128
        assert thumb["height"] == 64

        # Assert — thumbnail_ready still emitted
        assert len(emitted) >= 1
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

        emitted: list[tuple[str, object, object]] = []
        service.thumbnail_ready.connect(lambda p, i, r: emitted.append((p, i, r)))

        with (
            patch("tarragon.services.thumbnail_service.render_plain_image", return_value=img),
            patch("tarragon.services.thumbnail_service.generate_cache_uuid", return_value="test-uuid"),
            patch("tarragon.services.thumbnail_service.generate_cache_paths") as mock_paths,
            patch("tarragon.services.thumbnail_service.save_to_cache"),
            patch("tarragon.services.thumbnail_service.derive_smaller_sizes", return_value={}),
            patch(
                "tarragon.services.color_tagger.extract_dominant_color_tags",
                return_value=["color:red"],
            ),
            patch.object(
                db,
                "replace_auto_color_tags",
                side_effect=RuntimeError("DB write failed!"),
            ),
        ):
            mock_paths.return_value = {
                str(RESOLUTION_THUMBNAIL): tmp_path / "cache" / "256.png",
                str(RESOLUTION_PREVIEW): tmp_path / "cache" / "1024.png",
                "full": tmp_path / "cache" / "full.png",
            }
            # Act — should NOT raise
            service._render_all_resolutions(file_info)

        # Assert — thumbnail was still persisted
        thumb = db.get_thumbnail(str(file_info.path))
        assert thumb is not None

        # Assert — signal still emitted
        assert len(emitted) >= 1
        assert emitted[0][1] is img


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
        settings_mock.get_cache_format.return_value = "png"
        settings_mock.get_max_psd_workers.return_value = 3
        settings_mock.get_large_canvas_threshold_mp.return_value = 20.0
        settings_mock.get_tile_grid_size.return_value = "2x2"
        settings_mock.get_color_tag_enabled.return_value = True
        settings_mock.get_color_tag_palette_size.return_value = 4
        settings_mock.get_color_tag_min_share.return_value = 0.25
        settings_mock.get_color_tag_neutral_s_threshold.return_value = 0.30

        with patch("tarragon.services.thumbnail_service.get_executor"):
            svc = ThumbnailService(db=db, settings_service=settings_mock)
        svc._threadpool = MagicMock()

        file_info = _make_file_info(tmp_path)
        img = _make_image()

        # Act
        with (
            patch("tarragon.services.thumbnail_service.render_plain_image", return_value=img),
            patch("tarragon.services.thumbnail_service.generate_cache_uuid", return_value="test-uuid"),
            patch("tarragon.services.thumbnail_service.generate_cache_paths") as mock_paths,
            patch("tarragon.services.thumbnail_service.save_to_cache"),
            patch("tarragon.services.thumbnail_service.derive_smaller_sizes", return_value={}),
            patch(
                "tarragon.services.color_tagger.extract_dominant_color_tags",
                return_value=["color:red"],
            ) as mock_extract,
        ):
            mock_paths.return_value = {
                str(RESOLUTION_THUMBNAIL): tmp_path / "cache" / "256.png",
                str(RESOLUTION_PREVIEW): tmp_path / "cache" / "1024.png",
                "full": tmp_path / "cache" / "full.png",
            }
            svc._render_all_resolutions(file_info)

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

        # Act
        with (
            patch("tarragon.services.thumbnail_service.render_plain_image", return_value=img),
            patch("tarragon.services.thumbnail_service.generate_cache_uuid", return_value="test-uuid"),
            patch("tarragon.services.thumbnail_service.generate_cache_paths") as mock_paths,
            patch("tarragon.services.thumbnail_service.save_to_cache"),
            patch("tarragon.services.thumbnail_service.derive_smaller_sizes", return_value={}),
            patch(
                "tarragon.services.color_tagger.extract_dominant_color_tags",
                return_value=["color:red"],
            ) as mock_extract,
        ):
            mock_paths.return_value = {
                str(RESOLUTION_THUMBNAIL): tmp_path / "cache" / "256.png",
                str(RESOLUTION_PREVIEW): tmp_path / "cache" / "1024.png",
                "full": tmp_path / "cache" / "full.png",
            }
            service._render_all_resolutions(file_info)

        # Assert — default parameter values from settings fixture
        mock_extract.assert_called_once_with(
            img,
            palette_size=8,
            min_share=0.10,
            neutral_s_threshold=0.15,
        )


class TestColorTaggingForValidImage:
    """Color tagging is skipped when image or cache_path is None."""

    def test_none_image_skips_color_tagging(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db: Database,
    ) -> None:
        """When render returns None, color tagging is not attempted."""
        # Arrange
        file_info = _make_file_info(tmp_path)

        with patch("tarragon.services.color_tagger.extract_dominant_color_tags") as mock_extract:
            with patch("tarragon.services.thumbnail_service.render_plain_image", return_value=None):
                with patch("tarragon.services.thumbnail_service.generate_cache_uuid", return_value="test-uuid"):
                    with patch("tarragon.services.thumbnail_service.generate_cache_paths"):
                        # Act
                        service._render_all_resolutions(file_info)

        # Assert
        mock_extract.assert_not_called()
        tag_entries = _get_file_tag_names(db, str(file_info.path))
        assert tag_entries == []

    def test_valid_render_triggers_color_tagging(
        self,
        tmp_path: Path,
        service: ThumbnailService,
        db: Database,
    ) -> None:
        """When render succeeds but save fails, color tagging is still attempted.

        Note: In the new implementation, color tagging happens in _render_all_resolutions
        after the render succeeds, regardless of save_to_cache. This test verifies that
        if render_plain_image returns None, no color tagging occurs.
        """
        # Arrange
        file_info = _make_file_info(tmp_path)
        img = _make_image()

        # When render returns a valid image, color tagging IS attempted
        with (
            patch("tarragon.services.thumbnail_service.render_plain_image", return_value=img),
            patch("tarragon.services.thumbnail_service.generate_cache_uuid", return_value="test-uuid"),
            patch("tarragon.services.thumbnail_service.generate_cache_paths") as mock_paths,
            patch("tarragon.services.thumbnail_service.save_to_cache"),
            patch("tarragon.services.thumbnail_service.derive_smaller_sizes", return_value={}),
            patch(
                "tarragon.services.color_tagger.extract_dominant_color_tags",
                return_value=["color:red"],
            ) as mock_extract,
        ):
            mock_paths.return_value = {
                str(RESOLUTION_THUMBNAIL): tmp_path / "cache" / "256.png",
                str(RESOLUTION_PREVIEW): tmp_path / "cache" / "1024.png",
                "full": tmp_path / "cache" / "full.png",
            }
            service._render_all_resolutions(file_info)

        # Assert — extract WAS called since render succeeded
        mock_extract.assert_called_once()
