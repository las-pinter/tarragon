"""Tests for the plain image render pipeline"""

from __future__ import annotations

from pathlib import Path

from PIL import Image


class TestRenderPlainImage:
    """Testing plain image rendering"""

    def test_render_plain_image_one_by_one_pixel(self, tmp_path: Path) -> None:
        """render_plain_image handles 1x1 pixel images without error."""
        from tarragon.renderers.plain import render_plain_image

        img_path = tmp_path / "one_by_one.png"
        img = Image.new("RGB", (1, 1), color="red")
        img.save(img_path)

        result = render_plain_image(img_path)

        assert result is not None
        # 1x1 should remain 1x1 since it's already <= MASTER_LONG_EDGE
        assert result.size == (1, 1)
        assert result.mode == "RGB"

    def test_render_plain_image_smaller_than_master_long_edge_is_not_upscaled(self, tmp_path: Path) -> None:
        """render_plain_image does NOT upscale images smaller than MASTER_LONG_EDGE."""
        from tarragon.renderers.plain import render_plain_image

        img_path = tmp_path / "small.png"
        small_size = (100, 200)
        img = Image.new("RGB", small_size, color="blue")
        img.save(img_path)

        result = render_plain_image(img_path)

        assert result is not None
        # Thumbnail only shrinks, never enlarges
        assert result.size == small_size

    def test_render_plain_image_very_large_image_still_resizes(self, tmp_path: Path) -> None:
        """render_plain_image resizes a very large image (e.g. 6000x4000) when target_size is given."""
        from tarragon.renderers.cache import MASTER_LONG_EDGE
        from tarragon.renderers.plain import render_plain_image

        img_path = tmp_path / "very_large.jpg"
        # 6000x4000 = 24 MP — large enough to stress the resize path
        img = Image.new("RGB", (6000, 4000), color="green")
        img.save(img_path, format="JPEG", quality=85)

        result = render_plain_image(img_path, target_size=MASTER_LONG_EDGE)

        assert result is not None
        assert max(result.size) <= MASTER_LONG_EDGE
        # Aspect ratio should be preserved
        assert abs(result.size[0] / result.size[1] - 6000 / 4000) < 0.01

    def test_render_plain_image_animated_gif_uses_first_frame(self, tmp_path: Path) -> None:
        """render_plain_image opens only the first frame of an animated GIF (mode P → RGBA)."""
        from tarragon.renderers.plain import render_plain_image

        frames = []
        for color in ("red", "blue"):
            frame = Image.new("P", (100, 100), color)
            # Set a palette entry to the desired color
            frame.info["transparency"] = 0
            frames.append(frame)

        img_path = tmp_path / "animated.gif"
        frames[0].save(
            img_path,
            save_all=True,
            append_images=frames[1:],
            duration=100,
            loop=0,
        )

        result = render_plain_image(img_path)

        assert result is not None
        assert result.mode == "RGBA"  # image converted to RGBA
        assert result.size == (100, 100)

    def test_render_plain_image_converts_cmyk_to_rgba(self, tmp_path: Path) -> None:
        """render_plain_image converts CMYK images to RGBA."""
        from tarragon.renderers.plain import render_plain_image

        img_path = tmp_path / "cmyk.tiff"
        img = Image.new("CMYK", (100, 100), (0, 0, 0, 0))
        img.save(img_path, format="TIFF")

        result = render_plain_image(img_path)

        assert result is not None
        assert result.mode == "RGBA", f"Expected RGBA, got {result.mode}"

    def test_render_plain_image_with_icc_profile(self, tmp_path: Path) -> None:
        """render_plain_image handles images with embedded ICC profiles."""
        from PIL import ImageCms
        from tarragon.renderers.plain import render_plain_image

        img_path = tmp_path / "icc_profile.jpg"
        img = Image.new("RGB", (200, 150), color="red")

        # Create and attach an sRGB ICC profile
        srgb_profile = ImageCms.createProfile("sRGB")
        img.save(img_path, format="JPEG", icc_profile=ImageCms.ImageCmsProfile(srgb_profile).tobytes())

        result = render_plain_image(img_path)

        assert result is not None
        assert result.mode == "RGB"
        assert max(result.size) <= 2048

    def test_render_plain_image_with_exif_orientation_is_auto_rotated(self, tmp_path: Path) -> None:
        """render_plain_image auto-rotates based on EXIF orientation tag."""
        from tarragon.renderers.plain import render_plain_image

        img_path = tmp_path / "exif_portrait.jpg"
        # Create a portrait-orientation image (tall, not wide)
        img = Image.new("RGB", (50, 100), color="red")

        # Set EXIF orientation: 6 = rotate 90 CW (meaning the camera was rotated)
        exif = img.getexif()
        exif[0x0112] = 6
        img.save(img_path, format="JPEG", exif=exif)

        result = render_plain_image(img_path)

        assert result is not None
        # render_plain_image NOW auto-rotates via ImageOps.exif_transpose,
        # so dimensions swap from (50, 100) to (100, 50)
        assert result.size == (100, 50), f"Expected (100, 50) after auto-rotation, got {result.size}"

    def test_render_plain_image_corrupt_jpeg_valid_header_returns_none(self, tmp_path: Path) -> None:
        """render_plain_image returns None for a JPEG with valid magic bytes but garbage data."""
        from tarragon.renderers.plain import render_plain_image

        img_path = tmp_path / "corrupt_header.jpg"
        # JPEG starts with FF D8 FF — valid header, then garbage
        corrupt_data = b"\xff\xd8\xff\xe0" + b"\x00" * 100 + b"GARBAGE" * 50
        img_path.write_bytes(corrupt_data)

        result = render_plain_image(img_path)

        assert result is None

    def test_render_plain_image_follows_symlinks(self, tmp_path: Path) -> None:
        """render_plain_image follows symlinks to valid image files."""
        from tarragon.renderers.plain import render_plain_image

        real_img = tmp_path / "real_image.png"
        img = Image.new("RGB", (100, 100), color="green")
        img.save(real_img)

        link_path = tmp_path / "link_image.png"
        link_path.symlink_to(real_img)

        result = render_plain_image(link_path)

        assert result is not None
        assert result.mode == "RGB"
        assert max(result.size) <= 2048

    def test_render_plain_image_converts_one_bit_to_rgba(self, tmp_path: Path) -> None:
        """render_plain_image converts mode '1' (1-bit) images to RGBA."""
        from tarragon.renderers.plain import render_plain_image

        img_path = tmp_path / "one_bit.png"
        from PIL import Image

        img = Image.new("1", (50, 50), 1)  # 1-bit: 1 = white
        img.save(img_path, format="PNG")

        result = render_plain_image(img_path)

        assert result is not None
        assert result.mode == "RGBA", f"Expected RGBA, got {result.mode}"

    def test_render_plain_image_handles_ycbcr_jpeg(self, tmp_path: Path) -> None:
        """render_plain_image handles YCbCr JPEGs — Pillow auto-converts to RGB on open."""
        from tarragon.renderers.plain import render_plain_image

        img_path = tmp_path / "ycbcr.jpg"
        img = Image.new("YCbCr", (100, 100), (128, 128, 128))
        img.save(img_path, format="JPEG")

        result = render_plain_image(img_path)

        assert result is not None
        # Pillow's JPEG decoder converts YCbCr to RGB when opening, so mode is "RGB"
        assert result.mode == "RGB", f"Expected RGB (Pillow auto-converts YCbCr JPEG), got {result.mode}"

    def test_render_plain_image_empty_file_returns_none(self, tmp_path: Path) -> None:
        """render_plain_image returns None for an empty file."""
        from tarragon.renderers.plain import render_plain_image

        empty_path = tmp_path / "empty.png"
        empty_path.write_text("")

        result = render_plain_image(empty_path)

        assert result is None

    def test_render_plain_image_rgba_source(self, tmp_path: Path) -> None:
        """render_plain_image keeps RGBA source images in RGBA mode."""
        from tarragon.renderers.plain import render_plain_image

        img_path = tmp_path / "rgba_source.png"
        # Create RGBA image and save as PNG (which supports alpha)
        img = Image.new("RGBA", (100, 100), (255, 0, 0, 128))
        img.save(img_path, format="PNG")

        result = render_plain_image(img_path)

        assert result is not None
        assert result.mode == "RGBA"
        assert result.size == (100, 100)
