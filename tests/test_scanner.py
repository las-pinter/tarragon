"""Tests for Scanner"""

from __future__ import annotations

from pathlib import Path

from tarragon.scanner import SUPPORTED_EXTENSIONS, FileInfo, scan_folder


class TestExtensionFiltering:
    """Only supported extensions are returned, case-insensitively."""

    def test_only_supported_extensions_returned(self, tmp_path: Path) -> None:
        """Only files with extensions in SUPPORTED_EXTENSIONS appear in results."""
        for ext in {".jpg", ".jpeg", ".png", ".webp", ".tiff", ".tif", ".psd", ".psb", ".clip"}:
            (tmp_path / f"image{ext}").write_text("fake-image-data")

        (tmp_path / "notes.txt").write_text("hello")
        (tmp_path / "readme.md").write_text("# Hi")
        (tmp_path / "script.py").write_text("print('hello')")

        results = scan_folder(tmp_path)

        assert len(results) == 9
        for info in results:
            assert info.extension in SUPPORTED_EXTENSIONS
            assert info.path.suffix.lower() == info.extension

    def test_unsupported_files_excluded(self, tmp_path: Path) -> None:
        """Files with unsupported extensions (.txt, .md, .pdf, .py, .zip) are excluded."""
        (tmp_path / "photo.jpg").write_text("data")
        (tmp_path / "notes.txt").write_text("data")
        (tmp_path / "doc.pdf").write_text("data")
        (tmp_path / "archive.zip").write_text("data")

        results = scan_folder(tmp_path)

        assert len(results) == 1
        assert results[0].path.name == "photo.jpg"
        for info in results:
            assert info.extension in SUPPORTED_EXTENSIONS

    def test_case_insensitive_extension_matching(self, tmp_path: Path) -> None:
        """Extensions like .JPG, .PNG, .TIFF are matched case-insensitively."""
        (tmp_path / "photo.JPG").write_text("data")
        (tmp_path / "image.PNG").write_text("data")
        (tmp_path / "asset.TIFF").write_text("data")
        (tmp_path / "doc.TxT").write_text("data")  # unsupported regardless of case

        results = scan_folder(tmp_path)

        assert len(results) == 3
        names = {r.path.name for r in results}
        assert names == {"photo.JPG", "image.PNG", "asset.TIFF"}
        for info in results:
            assert info.extension == info.extension.lower()
            assert info.extension in {".jpg", ".png", ".tiff"}

    def test_clip_files_are_discovered(self, tmp_path: Path) -> None:
        """Clip files are included in scan results as a supported extension."""
        assert ".clip" in SUPPORTED_EXTENSIONS

        (tmp_path / "illustration.clip").write_text("fake-clip-data")
        (tmp_path / "photo.jpg").write_text("data")
        (tmp_path / "notes.txt").write_text("data")

        results = scan_folder(tmp_path)

        assert len(results) == 2
        clip_results = [r for r in results if r.extension == ".clip"]
        assert len(clip_results) == 1
        assert clip_results[0].path.name == "illustration.clip"


class TestRecursiveScanning:
    """Recursive scanning includes nested folders."""

    def test_recursive_scanning(self, tmp_path: Path) -> None:
        """When recursive=True, files in nested subdirectories are included."""
        (tmp_path / "root.jpg").write_text("data")
        sub = tmp_path / "subdir"
        sub.mkdir()
        (sub / "nested.png").write_text("data")
        deep = sub / "deeper"
        deep.mkdir()
        (deep / "deep.webp").write_text("data")

        results = scan_folder(tmp_path, recursive=True)

        assert len(results) == 3
        paths = {r.path for r in results}
        assert paths == {
            tmp_path / "root.jpg",
            sub / "nested.png",
            deep / "deep.webp",
        }

    def test_non_recursive_default_excludes_nested(self, tmp_path: Path) -> None:
        """With recursive=False (default), only direct-child files are returned."""
        (tmp_path / "root.jpg").write_text("data")
        sub = tmp_path / "subdir"
        sub.mkdir()
        (sub / "nested.png").write_text("data")

        results = scan_folder(tmp_path)

        assert len(results) == 1
        assert results[0].path.name == "root.jpg"


class TestMissingOrEmptyFolders:
    """Missing or empty folders return an empty list."""

    def test_non_existent_folder_returns_empty_list(self, tmp_path: Path) -> None:
        """Scanning a non-existent folder returns an empty list (no crash)."""
        missing = tmp_path / "i_do_not_exist"
        results = scan_folder(missing)

        assert results == []

    def test_non_existent_folder_recursive_returns_empty_list(self, tmp_path: Path) -> None:
        """Scanning a non-existent folder in recursive mode also returns an empty list."""
        missing = tmp_path / "nowhere"
        results = scan_folder(missing, recursive=True)

        assert results == []

    def test_empty_folder_returns_empty_list(self, tmp_path: Path) -> None:
        """Scanning an existing but empty folder returns an empty list."""
        results = scan_folder(tmp_path)

        assert results == []


class TestFileInfo:
    """FileInfo records the source file's attributes."""

    def test_fileinfo_has_correct_attributes(self, tmp_path: Path) -> None:
        """Each FileInfo has path, size, mtime, and extension matching the source file."""
        content = b"some-image-bytes"
        (tmp_path / "artwork.png").write_bytes(content)

        results = scan_folder(tmp_path)
        assert len(results) == 1

        info = results[0]
        assert isinstance(info, FileInfo)
        assert info.path == tmp_path / "artwork.png"
        assert info.size == len(content)
        assert isinstance(info.mtime, float)
        assert info.mtime > 0
        assert info.extension == ".png"


class TestSortOrder:
    """Results are sorted deterministically by path."""

    def test_sort_order_is_deterministic(self, tmp_path: Path) -> None:
        """Results are sorted by path for consistent ordering."""
        names = ["zeta.jpg", "alpha.png", "delta.webp", "beta.tiff"]
        for name in names:
            (tmp_path / name).write_text("data")

        results = scan_folder(tmp_path)

        assert len(results) == 4
        assert results[0].path.name == "alpha.png"
        assert results[1].path.name == "beta.tiff"
        assert results[2].path.name == "delta.webp"
        assert results[3].path.name == "zeta.jpg"

        results2 = scan_folder(tmp_path)
        assert [r.path for r in results] == [r.path for r in results2]

    def test_recursive_sort_order(self, tmp_path: Path) -> None:
        """Recursive results are sorted by path deterministically."""
        sub = tmp_path / "b_sub"
        sub.mkdir()
        (sub / "nested.png").write_text("data")
        (tmp_path / "alpha.jpg").write_text("data")

        results = scan_folder(tmp_path, recursive=True)

        assert len(results) == 2
        assert results[0].path.name == "alpha.jpg"
        assert results[1].path.name == "nested.png"


class TestHiddenFiles:
    """Hidden files are included in results."""

    def test_hidden_files_are_included(self, tmp_path: Path) -> None:
        """Files starting with a dot are not excluded from results."""
        (tmp_path / ".hidden.png").write_text("data")
        (tmp_path / "visible.jpg").write_text("data")

        results = scan_folder(tmp_path)

        assert len(results) == 2
        names = {r.path.name for r in results}
        assert names == {".hidden.png", "visible.jpg"}
