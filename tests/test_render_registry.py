"""Tests for the render format registry (dispatch tables, adapters, service wiring)."""

from __future__ import annotations

import threading
from collections.abc import Generator
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest
from PIL import Image

from tarragon.renderers import registry
from tarragon.renderers.cache import RESOLUTION_FULL, RESOLUTION_PREVIEW, RESOLUTION_THUMBNAIL
from tarragon.renderers.registry import DEFAULT_RENDERER, FORMAT_DISPATCH, SUPPORTED_EXTENSIONS
from tarragon.scanner import SUPPORTED_EXTENSIONS as SCANNER_SUPPORTED_EXTENSIONS
from tarragon.scanner import FileInfo
from tarragon.services.thumbnail_service import ThumbnailService


class _CannedValue:
    """Settings leaf returning a fixed value from get()."""

    def __init__(self, value: float | str) -> None:
        self._value = value

    def get(self) -> float | str:
        return self._value


class _FakeLargeCanvasSettings:
    """Fake settings exposing canned large-canvas threshold and grid values."""

    def __init__(self, threshold: float, grid: str) -> None:
        self.large_canvas_threshold_mp = _CannedValue(threshold)
        self.tile_grid_size = _CannedValue(grid)


@pytest.fixture
def service() -> Generator[ThumbnailService, None, None]:
    """Create a ThumbnailService with mocked dependencies for registry dispatch tests."""
    db_mock = MagicMock()
    db_mock.get_or_create_folder_uuid.return_value = "test-uuid"
    settings = MagicMock()
    settings.cache_format.get.return_value = "PNG"
    settings.max_psd_workers.get.return_value = 3
    settings.large_canvas_threshold_mp.get.return_value = 20.0
    settings.tile_grid_size.get.return_value = "2x2"
    settings.color_tag_enabled.get.return_value = False
    tag_service = MagicMock()
    with patch("tarragon.services.thumbnail_service.get_executor"):
        svc = ThumbnailService(db=db_mock, settings_service=settings, tag_service=tag_service)
    yield svc


@pytest.fixture(autouse=True)
def clear_settings_provider(monkeypatch: pytest.MonkeyPatch) -> None:
    """Clear the registry settings provider before each test."""
    monkeypatch.setattr(registry, "_settings_provider", None)


class TestRegistryShape:
    """The registry exposes exact dispatch maps and extension sets."""

    def test_supported_extensions_is_exact_ten_entry_set(self) -> None:
        """SUPPORTED_EXTENSIONS contains exactly the ten known image formats."""
        assert SUPPORTED_EXTENSIONS == {
            ".jpg",
            ".jpeg",
            ".png",
            ".webp",
            ".tiff",
            ".tif",
            ".psd",
            ".psb",
            ".clip",
            ".kra",
        }

    def test_format_dispatch_covers_exactly_four_specialized_formats(self) -> None:
        """FORMAT_DISPATCH keys are the four specialized formats, lowercase with leading dots."""
        assert set(FORMAT_DISPATCH) == {".psd", ".psb", ".clip", ".kra"}
        assert all(key == key.lower() and key.startswith(".") for key in FORMAT_DISPATCH)

    def test_format_dispatch_keys_are_subset_of_supported_extensions(self) -> None:
        """Every FORMAT_DISPATCH key also appears in SUPPORTED_EXTENSIONS."""
        assert set(FORMAT_DISPATCH).issubset(SUPPORTED_EXTENSIONS)

    def test_scanner_and_registry_share_extension_set(self) -> None:
        """scanner.SUPPORTED_EXTENSIONS is the registry singleton, not a copy."""
        assert SCANNER_SUPPORTED_EXTENSIONS is SUPPORTED_EXTENSIONS

    def test_unknown_extension_falls_back_to_default_renderer(self) -> None:
        """A key missing from FORMAT_DISPATCH resolves to the DEFAULT_RENDERER identity."""
        assert FORMAT_DISPATCH.get(".unknown", DEFAULT_RENDERER) is DEFAULT_RENDERER

    def test_psd_and_psb_map_to_same_adapter(self) -> None:
        """The .psd and .psb keys dispatch to the same PSD adapter object."""
        assert FORMAT_DISPATCH[".psd"] is FORMAT_DISPATCH[".psb"]

    def test_upper_case_lookup_resolves_to_lowercase_adapter(self) -> None:
        """Lowercasing an upper-case extension before lookup finds the PSD adapter."""
        assert FORMAT_DISPATCH.get(".PSD".lower(), DEFAULT_RENDERER) is FORMAT_DISPATCH[".psd"]


class TestAdapterContracts:
    """Adapters forward path and target_size to the underlying renderers."""

    def test_clip_adapter_forwards_path_and_target_size(self, tmp_path: Path) -> None:
        """_render_clip calls render_clip_image with the source path and target_size."""
        cancel_event = threading.Event()
        with patch("tarragon.renderers.registry.render_clip_image", return_value=None) as mock_clip:
            registry._render_clip(tmp_path / "test.clip", RESOLUTION_FULL, cancel_event)

        mock_clip.assert_called_once_with(tmp_path / "test.clip", target_size=RESOLUTION_FULL)

    def test_kra_adapter_forwards_path_and_target_size(self, tmp_path: Path) -> None:
        """_render_kra calls render_kra_image with the source path and target_size."""
        cancel_event = threading.Event()
        with patch("tarragon.renderers.registry.render_kra_image", return_value=None) as mock_kra:
            registry._render_kra(tmp_path / "test.kra", RESOLUTION_FULL, cancel_event)

        mock_kra.assert_called_once_with(tmp_path / "test.kra", target_size=RESOLUTION_FULL)

    def test_default_adapter_forwards_path_and_target_size(self, tmp_path: Path) -> None:
        """_render_default calls render_plain_image with the source path and target_size."""
        cancel_event = threading.Event()
        with patch("tarragon.renderers.registry.render_plain_image", return_value=None) as mock_plain:
            registry._render_default(tmp_path / "test.png", RESOLUTION_FULL, cancel_event)

        mock_plain.assert_called_once_with(tmp_path / "test.png", target_size=RESOLUTION_FULL)


class TestPsdAdapter:
    """The PSD adapter consults the configured settings service before rendering."""

    def test_psd_adapter_raises_without_configured_provider(self, tmp_path: Path) -> None:
        """_render_psd raises RuntimeError when configure_settings() was never called."""
        with pytest.raises(RuntimeError, match="configure_settings"):
            registry._render_psd(tmp_path / "test.psd", RESOLUTION_FULL, None)

    def test_psd_adapter_parses_grid_and_forwards_cancel_event(self, tmp_path: Path) -> None:
        """_render_psd reads threshold, parses the x grid string, and forwards cancel_event."""
        settings = _FakeLargeCanvasSettings(20.0, "3x3")
        registry.configure_settings(lambda: settings)
        cancel_event = threading.Event()

        with patch("tarragon.renderers.registry.render_psd_image", return_value=None) as mock_psd:
            registry._render_psd(tmp_path / "test.psd", RESOLUTION_FULL, cancel_event)

        mock_psd.assert_called_once_with(
            tmp_path / "test.psd",
            20.0,
            3,
            3,
            target_size=RESOLUTION_FULL,
            cancel_event=cancel_event,
        )

    def test_psd_adapter_reconsults_provider_for_each_call(self, tmp_path: Path) -> None:
        """_render_psd reads fresh settings on every call rather than caching the first values."""
        first_settings = _FakeLargeCanvasSettings(20.0, "3x3")
        registry.configure_settings(lambda: first_settings)
        cancel_event = threading.Event()

        with patch("tarragon.renderers.registry.render_psd_image", return_value=None) as mock_psd:
            registry._render_psd(tmp_path / "a.psd", RESOLUTION_FULL, cancel_event)
            second_settings = _FakeLargeCanvasSettings(55.5, "4x7")
            registry.configure_settings(lambda: second_settings)
            registry._render_psd(tmp_path / "b.psd", RESOLUTION_FULL, cancel_event)

        first_call, second_call = mock_psd.call_args_list
        assert first_call.args[:4] == (tmp_path / "a.psd", 20.0, 3, 3)
        assert second_call.args[:4] == (tmp_path / "b.psd", 55.5, 4, 7)
        assert first_call.kwargs["target_size"] == RESOLUTION_FULL
        assert second_call.kwargs["target_size"] == RESOLUTION_FULL
        assert first_call.kwargs["cancel_event"] is cancel_event
        assert second_call.kwargs["cancel_event"] is cancel_event

    def test_psd_adapter_rejects_grid_without_x_separator(self, tmp_path: Path) -> None:
        """_render_psd raises ValueError when the grid setting lacks an x separator."""
        settings = _FakeLargeCanvasSettings(20.0, "3")
        registry.configure_settings(lambda: settings)

        with (
            pytest.raises(ValueError),
            patch("tarragon.renderers.registry.render_psd_image"),
        ):
            registry._render_psd(tmp_path / "test.psd", RESOLUTION_FULL, None)


class TestServiceDispatch:
    """_render_all_resolutions dispatches through the registry for specialized formats."""

    def test_kra_file_routes_to_kra_renderer(self, tmp_path: Path, service: ThumbnailService) -> None:
        """A .kra file renders through the registry kra adapter, not the plain default."""
        file_info = FileInfo(path=tmp_path / "art.kra", mtime=1000.0, size=500, extension=".kra")
        mock_img = MagicMock(spec=Image.Image)
        mock_img.width = 100
        mock_img.height = 80

        with (
            patch("tarragon.renderers.registry.render_kra_image", return_value=mock_img) as mock_kra,
            patch("tarragon.renderers.registry.render_plain_image") as mock_plain,
            patch("tarragon.services.thumbnail_service.generate_cache_uuid", return_value="test-uuid"),
            patch("tarragon.services.thumbnail_service.generate_cache_paths") as mock_paths,
            patch("tarragon.services.thumbnail_service.save_to_cache"),
            patch("tarragon.services.thumbnail_service.derive_smaller_sizes", return_value={}),
        ):
            mock_paths.return_value = {
                str(RESOLUTION_THUMBNAIL): tmp_path / "cache" / "256.png",
                str(RESOLUTION_PREVIEW): tmp_path / "cache" / "1024.png",
                "full": tmp_path / "cache" / "full.png",
            }
            service._render_all_resolutions(file_info)

        mock_kra.assert_called_once_with(file_info.path, target_size=RESOLUTION_FULL)
        mock_plain.assert_not_called()

    def test_unknown_extension_routes_to_plain_renderer(self, tmp_path: Path, service: ThumbnailService) -> None:
        """An extension outside FORMAT_DISPATCH renders through DEFAULT_RENDERER (plain)."""
        file_info = FileInfo(path=tmp_path / "photo.png", mtime=1000.0, size=500, extension=".png")
        mock_img = MagicMock(spec=Image.Image)
        mock_img.width = 100
        mock_img.height = 80

        with (
            patch("tarragon.renderers.registry.render_plain_image", return_value=mock_img) as mock_plain,
            patch("tarragon.renderers.registry.render_kra_image") as mock_kra,
            patch("tarragon.services.thumbnail_service.generate_cache_uuid", return_value="test-uuid"),
            patch("tarragon.services.thumbnail_service.generate_cache_paths") as mock_paths,
            patch("tarragon.services.thumbnail_service.save_to_cache"),
            patch("tarragon.services.thumbnail_service.derive_smaller_sizes", return_value={}),
        ):
            mock_paths.return_value = {
                str(RESOLUTION_THUMBNAIL): tmp_path / "cache" / "256.png",
                str(RESOLUTION_PREVIEW): tmp_path / "cache" / "1024.png",
                "full": tmp_path / "cache" / "full.png",
            }
            service._render_all_resolutions(file_info)

        mock_plain.assert_called_once_with(file_info.path, target_size=RESOLUTION_FULL)
        mock_kra.assert_not_called()

    def test_upper_case_extension_is_lowercased_before_dispatch(
        self, tmp_path: Path, service: ThumbnailService
    ) -> None:
        """An upper-case .KRA extension is lowercased and still reaches the kra adapter."""
        file_info = FileInfo(path=tmp_path / "art.KRA", mtime=1000.0, size=500, extension=".KRA")
        mock_img = MagicMock(spec=Image.Image)
        mock_img.width = 100
        mock_img.height = 80

        with (
            patch("tarragon.renderers.registry.render_kra_image", return_value=mock_img) as mock_kra,
            patch("tarragon.services.thumbnail_service.generate_cache_uuid", return_value="test-uuid"),
            patch("tarragon.services.thumbnail_service.generate_cache_paths") as mock_paths,
            patch("tarragon.services.thumbnail_service.save_to_cache"),
            patch("tarragon.services.thumbnail_service.derive_smaller_sizes", return_value={}),
        ):
            mock_paths.return_value = {
                str(RESOLUTION_THUMBNAIL): tmp_path / "cache" / "256.png",
                str(RESOLUTION_PREVIEW): tmp_path / "cache" / "1024.png",
                "full": tmp_path / "cache" / "full.png",
            }
            service._render_all_resolutions(file_info)

        mock_kra.assert_called_once_with(file_info.path, target_size=RESOLUTION_FULL)
