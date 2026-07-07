"""Tests for the folder-scanner module."""

from __future__ import annotations

from pathlib import Path


def test_only_supported_extensions_returned(tmp_path: Path) -> None:
    """Only files with extensions in SUPPORTED_EXTENSIONS appear in results."""
    from tarragon.scanner import SUPPORTED_EXTENSIONS, scan_folder

    # Create supported files — all 9 extensions
    for ext in {".jpg", ".jpeg", ".png", ".webp", ".tiff", ".tif", ".psd", ".psb", ".clip"}:
        (tmp_path / f"image{ext}").write_text("fake-image-data")

    # Create unsupported files
    (tmp_path / "notes.txt").write_text("hello")
    (tmp_path / "readme.md").write_text("# Hi")
    (tmp_path / "script.py").write_text("print('hello')")

    results = scan_folder(tmp_path)

    assert len(results) == 9
    for info in results:
        assert info.extension in SUPPORTED_EXTENSIONS
        assert info.path.suffix.lower() == info.extension


def test_unsupported_files_excluded(tmp_path: Path) -> None:
    """Files with unsupported extensions (.txt, .md, .pdf, .py, .zip) are excluded."""
    from tarragon.scanner import SUPPORTED_EXTENSIONS, scan_folder

    (tmp_path / "photo.jpg").write_text("data")
    (tmp_path / "notes.txt").write_text("data")
    (tmp_path / "doc.pdf").write_text("data")
    (tmp_path / "archive.zip").write_text("data")

    results = scan_folder(tmp_path)

    assert len(results) == 1
    assert results[0].path.name == "photo.jpg"

    # All returned extensions are in the supported set
    for info in results:
        assert info.extension in SUPPORTED_EXTENSIONS


def test_case_insensitive_extension_matching(tmp_path: Path) -> None:
    """Extensions like .JPG, .PNG, .TIFF are matched case-insensitively."""
    from tarragon.scanner import scan_folder

    (tmp_path / "photo.JPG").write_text("data")
    (tmp_path / "image.PNG").write_text("data")
    (tmp_path / "asset.TIFF").write_text("data")
    (tmp_path / "doc.TxT").write_text("data")  # unsupported regardless of case

    results = scan_folder(tmp_path)

    assert len(results) == 3
    names = {r.path.name for r in results}
    assert names == {"photo.JPG", "image.PNG", "asset.TIFF"}
    # Extension is stored lowercase
    for info in results:
        assert info.extension == info.extension.lower()
        assert info.extension in {".jpg", ".png", ".tiff"}


def test_recursive_scanning(tmp_path: Path) -> None:
    """When recursive=True, files in nested subdirectories are included."""
    from tarragon.scanner import scan_folder

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


def test_non_recursive_default_excludes_nested(tmp_path: Path) -> None:
    """With recursive=False (default), only direct-child files are returned."""
    from tarragon.scanner import scan_folder

    (tmp_path / "root.jpg").write_text("data")
    sub = tmp_path / "subdir"
    sub.mkdir()
    (sub / "nested.png").write_text("data")

    results = scan_folder(tmp_path)

    assert len(results) == 1
    assert results[0].path.name == "root.jpg"


def test_non_existent_folder_returns_empty_list(tmp_path: Path) -> None:
    """Scanning a non-existent folder returns an empty list (no crash)."""
    from tarragon.scanner import scan_folder

    missing = tmp_path / "i_do_not_exist"
    results = scan_folder(missing)

    assert results == []


def test_non_existent_folder_recursive_returns_empty_list(tmp_path: Path) -> None:
    """Scanning a non-existent folder in recursive mode also returns empty list."""
    from tarragon.scanner import scan_folder

    missing = tmp_path / "nowhere"
    results = scan_folder(missing, recursive=True)

    assert results == []


def test_empty_folder_returns_empty_list(tmp_path: Path) -> None:
    """Scanning an existing but empty folder returns an empty list."""
    from tarragon.scanner import scan_folder

    results = scan_folder(tmp_path)

    assert results == []


def test_fileinfo_has_correct_attributes(tmp_path: Path) -> None:
    """Each FileInfo has path, mtime, size, and extension matching the source file."""
    from tarragon.scanner import FileInfo, scan_folder

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


def test_sort_order_is_deterministic(tmp_path: Path) -> None:
    """Results are sorted by path for consistent ordering."""
    from tarragon.scanner import scan_folder

    # Create files in non-alphabetical order
    names = ["zeta.jpg", "alpha.png", "delta.webp", "beta.tiff"]
    for name in names:
        (tmp_path / name).write_text("data")

    results = scan_folder(tmp_path)

    assert len(results) == 4
    # Expect sorted by path name
    assert results[0].path.name == "alpha.png"
    assert results[1].path.name == "beta.tiff"
    assert results[2].path.name == "delta.webp"
    assert results[3].path.name == "zeta.jpg"

    # Running again gives the same order
    results2 = scan_folder(tmp_path)
    assert [r.path for r in results] == [r.path for r in results2]


def test_recursive_sort_order(tmp_path: Path) -> None:
    """Recursive results are sorted by path deterministically."""
    from tarragon.scanner import scan_folder

    sub = tmp_path / "b_sub"
    sub.mkdir()
    (sub / "nested.png").write_text("data")
    (tmp_path / "alpha.jpg").write_text("data")

    results = scan_folder(tmp_path, recursive=True)

    assert len(results) == 2
    assert results[0].path.name == "alpha.jpg"
    assert results[1].path.name == "nested.png"


def test_hidden_files_are_included(tmp_path: Path) -> None:
    """Files starting with a dot are not excluded from results."""
    from tarragon.scanner import scan_folder

    (tmp_path / ".hidden.png").write_text("data")
    (tmp_path / "visible.jpg").write_text("data")

    results = scan_folder(tmp_path)

    assert len(results) == 2
    names = {r.path.name for r in results}
    assert names == {".hidden.png", "visible.jpg"}


def test_clip_files_are_discovered(tmp_path: Path) -> None:
    """.clip files are included in scan results as a supported extension."""
    from tarragon.scanner import SUPPORTED_EXTENSIONS, scan_folder

    assert ".clip" in SUPPORTED_EXTENSIONS

    (tmp_path / "illustration.clip").write_text("fake-clip-data")
    (tmp_path / "photo.jpg").write_text("data")
    (tmp_path / "notes.txt").write_text("data")

    results = scan_folder(tmp_path)

    assert len(results) == 2
    clip_results = [r for r in results if r.extension == ".clip"]
    assert len(clip_results) == 1
    assert clip_results[0].path.name == "illustration.clip"
