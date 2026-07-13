#!/usr/bin/env python3
"""Benchmark script for Tarragon's rendering pipeline.

Measures performance of:
- Plain image rendering (Pillow open → convert → resize → cache)
- PSD compositing (direct and tiled paths via ProcessPoolExecutor)
- Cache hit vs cache miss scenarios

Usage:
    python scripts/benchmark_rendering.py
    python scripts/benchmark_rendering.py --iterations 20
    python scripts/benchmark_rendering.py --iterations 10 --skip-psd
"""

from __future__ import annotations

import argparse
import io
import statistics
import sys
import tempfile
import time
from pathlib import Path
from typing import Any

from PIL import Image, ImageDraw


# ---------------------------------------------------------------------------
# Test image generators
# ---------------------------------------------------------------------------


def create_test_image(width: int, height: int, color: tuple[int, ...] = (128, 64, 200)) -> Image.Image:
    """Create a test RGB image with a gradient pattern."""
    img = Image.new("RGB", (width, height), color)
    draw = ImageDraw.Draw(img)
    for y in range(0, height, 20):
        for x in range(0, width, 20):
            r = (x * 3) % 256
            g = (y * 5) % 256
            b = ((x + y) * 2) % 256
            draw.rectangle([x, y, x + 18, y + 18], fill=(r, g, b))
    return img


def create_test_rgba_image(width: int, height: int) -> Image.Image:
    """Create a test RGBA image with transparency."""
    img = Image.new("RGBA", (width, height), (100, 150, 200, 128))
    draw = ImageDraw.Draw(img)
    for i in range(0, min(width, height), 30):
        draw.ellipse([i, i, i + 28, i + 28], fill=(255, 100, 50, 200))
    return img


def create_minimal_psd(width: int, height: int) -> bytes:
    """Create a minimal valid PSD file in memory.

    Produces a bare-bones PSD with a single layer containing solid color.
    This is sufficient for psd-tools to open and composite.
    """
    from psd_tools import PSDImage

    img = Image.new("RGBA", (width, height), (80, 120, 200, 255))
    draw = ImageDraw.Draw(img)
    for y in range(0, height, 40):
        for x in range(0, width, 40):
            draw.rectangle([x, y, x + 38, y + 38], fill=(200, 80, 60, 255))

    psd = PSDImage.frompil(img)
    buf = io.BytesIO()
    psd.save(buf)
    return buf.getvalue()


# ---------------------------------------------------------------------------
# Timing helpers
# ---------------------------------------------------------------------------


class TimingResult:
    """Collects timing samples for a benchmark."""

    def __init__(self, name: str) -> None:
        self.name = name
        self.samples: list[float] = []

    def add(self, elapsed: float) -> None:
        self.samples.append(elapsed)

    @property
    def count(self) -> int:
        return len(self.samples)

    def report(self) -> dict[str, float]:
        if not self.samples:
            return {"mean": 0, "min": 0, "max": 0, "p95": 0, "count": 0}
        sorted_samples = sorted(self.samples)
        p95_index = max(0, int(len(sorted_samples) * 0.95) - 1)
        return {
            "mean": statistics.mean(sorted_samples),
            "min": sorted_samples[0],
            "max": sorted_samples[-1],
            "p95": sorted_samples[p95_index],
            "count": len(sorted_samples),
        }


def time_function(func: Any, *args: Any, **kwargs: Any) -> tuple[object, float]:
    """Call func and return (result, elapsed_seconds)."""
    start = time.perf_counter()
    result = func(*args, **kwargs)
    elapsed = time.perf_counter() - start
    return result, elapsed


# ---------------------------------------------------------------------------
# Benchmark: plain image rendering
# ---------------------------------------------------------------------------


def benchmark_plain_render(iterations: int, tmp_dir: Path) -> list[TimingResult]:
    """Benchmark the plain image rendering pipeline.

    Tests render_plain_image() and save_to_cache() with various image sizes
    and formats.
    """
    from tarragon.thumbnail import render_plain_image, save_to_cache

    results: list[TimingResult] = []

    test_configs = [
        ("small_jpeg", 640, 480, "JPEG", "small JPEG (640x480)"),
        ("medium_png", 1920, 1080, "PNG", "medium PNG (1920x1080)"),
        ("large_jpeg", 4000, 3000, "JPEG", "large JPEG (4000x3000)"),
        ("rgba_png", 2048, 2048, "PNG", "RGBA PNG (2048x2048)"),
    ]

    for config_name, w, h, fmt, label in test_configs:
        render_result = TimingResult(f"render_plain [{label}]")
        cache_result = TimingResult(f"save_cache [{label}]")

        for i in range(iterations):
            img = create_test_image(w, h) if fmt == "JPEG" else create_test_rgba_image(w, h)
            src_path = tmp_dir / f"{config_name}_{i}.{fmt.lower()}"
            img.save(src_path, fmt)

            _, elapsed_render = time_function(render_plain_image, src_path)
            render_result.add(elapsed_render)

            if i == 0 and render_result.samples:
                rendered = render_plain_image(src_path)
                if rendered is not None:
                    cache_path = tmp_dir / "cache" / f"{config_name}.png"
                    _, elapsed_cache = time_function(save_to_cache, rendered, cache_path, "png")
                    cache_result.add(elapsed_cache)

        results.append(render_result)
        results.append(cache_result)

    return results


# ---------------------------------------------------------------------------
# Benchmark: cache hit vs cache miss
# ---------------------------------------------------------------------------


def benchmark_cache_scenarios(iterations: int, tmp_dir: Path) -> list[TimingResult]:
    """Benchmark cache hit vs cache miss scenarios.

    Cache miss: render from original file (full pipeline).
    Cache hit: load from cached PNG file directly.
    """
    from tarragon.thumbnail import render_plain_image, save_to_cache

    results: list[TimingResult] = []

    cache_miss_result = TimingResult("cache_miss [1920x1080 PNG]")
    cache_hit_result = TimingResult("cache_hit [1920x1080 PNG]")

    src_path = tmp_dir / "cache_test_src.png"
    cache_path = tmp_dir / "cache" / "cache_test_master.png"

    img = create_test_image(1920, 1080)
    img.save(src_path, "PNG")

    rendered = render_plain_image(src_path)
    if rendered is not None:
        save_to_cache(rendered, cache_path, "png")

    for _ in range(iterations):
        _, elapsed_miss = time_function(render_plain_image, src_path)
        cache_miss_result.add(elapsed_miss)

        _, elapsed_hit = time_function(Image.open, cache_path)
        cache_hit_result.add(elapsed_hit)

    results.append(cache_miss_result)
    results.append(cache_hit_result)

    return results


# ---------------------------------------------------------------------------
# Benchmark: PSD compositing
# ---------------------------------------------------------------------------


def benchmark_psd_rendering(iterations: int, tmp_dir: Path) -> list[TimingResult]:
    """Benchmark PSD compositing via ProcessPoolExecutor.

    Tests both the direct composite path (small canvas) and the tiled
    composite path (large canvas above threshold).
    """
    from tarragon.thumbnail import render_psd_image

    results: list[TimingResult] = []

    psd_configs = [
        ("small_psd", 1024, 768, 20.0, 2, 2, "small PSD (1024x768, direct)"),
        ("medium_psd", 3000, 2000, 20.0, 2, 2, "medium PSD (3000x2000, direct)"),
        ("large_psd", 6000, 4000, 20.0, 2, 2, "large PSD (6000x4000, tiled 2x2)"),
    ]

    for config_name, w, h, threshold, grid_x, grid_y, label in psd_configs:
        render_result = TimingResult(f"render_psd [{label}]")

        psd_path = tmp_dir / f"{config_name}.psd"
        try:
            psd_bytes = create_minimal_psd(w, h)
            psd_path.write_bytes(psd_bytes)
        except Exception as exc:
            print(f"  [SKIP] Could not create test PSD for {label}: {exc}")
            results.append(render_result)
            continue

        for _ in range(iterations):
            _, elapsed = time_function(
                render_psd_image,
                psd_path,
                threshold,
                grid_x,
                grid_y,
            )
            render_result.add(elapsed)

        results.append(render_result)

    return results


# ---------------------------------------------------------------------------
# Benchmark: color tagging
# ---------------------------------------------------------------------------


def benchmark_color_tagging(iterations: int) -> list[TimingResult]:
    """Benchmark the color tagging algorithm on various image types."""
    from tarragon.services.color_tagger import extract_dominant_color_tags

    results: list[TimingResult] = []

    tag_configs = [
        ("solid_color", Image.new("RGB", (2048, 2048), (200, 50, 50)), "solid red"),
        ("gradient", create_test_image(2048, 2048), "patterned gradient"),
        ("rgba_complex", create_test_rgba_image(2048, 2048), "RGBA with transparency"),
    ]

    for config_name, img, label in tag_configs:
        tag_result = TimingResult(f"color_tag [{label}]")

        for _ in range(iterations):
            _, elapsed = time_function(
                extract_dominant_color_tags,
                img,
                8,
                0.10,
                0.15,
            )
            tag_result.add(elapsed)

        results.append(tag_result)

    return results


# ---------------------------------------------------------------------------
# Report formatting
# ---------------------------------------------------------------------------


def format_report(all_results: list[TimingResult]) -> str:
    """Format benchmark results into a readable report."""
    lines: list[str] = []
    lines.append("")
    lines.append("=" * 80)
    lines.append("TARRAGON RENDERING BENCHMARK RESULTS")
    lines.append("=" * 80)
    lines.append("")

    header = f"{'Benchmark':<50} {'Mean':>8} {'Min':>8} {'Max':>8} {'P95':>8} {'N':>4}"
    lines.append(header)
    lines.append("-" * 80)

    for result in all_results:
        report = result.report()
        if report["count"] == 0:
            lines.append(f"{result.name:<50} {'N/A':>8}")
            continue
        mean_ms = report["mean"] * 1000
        min_ms = report["min"] * 1000
        max_ms = report["max"] * 1000
        p95_ms = report["p95"] * 1000
        count = report["count"]
        lines.append(
            f"{result.name:<50} {mean_ms:>7.1f}ms {min_ms:>7.1f}ms {max_ms:>7.1f}ms {p95_ms:>7.1f}ms {count:>4}"
        )

    lines.append("-" * 80)
    lines.append("")
    lines.append("All times in milliseconds. P95 = 95th percentile.")
    lines.append("")
    return "\n".join(lines)


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Benchmark Tarragon's rendering pipeline.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=(
            "Examples:\n"
            "  python scripts/benchmark_rendering.py\n"
            "  python scripts/benchmark_rendering.py --iterations 20\n"
            "  python scripts/benchmark_rendering.py --iterations 5 --skip-psd\n"
        ),
    )
    parser.add_argument(
        "-n",
        "--iterations",
        type=int,
        default=5,
        help="Number of iterations per benchmark (default: 5)",
    )
    parser.add_argument(
        "--skip-psd",
        action="store_true",
        help="Skip PSD compositing benchmarks (requires psd-tools)",
    )
    parser.add_argument(
        "--skip-color",
        action="store_true",
        help="Skip color tagging benchmarks",
    )
    parser.add_argument(
        "--output-dir",
        type=str,
        default=None,
        help="Directory for temporary test files (default: auto-created temp dir)",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    iterations = max(1, args.iterations)

    print(f"Tarragon Rendering Benchmark")
    print(f"Iterations per test: {iterations}")
    print()

    tmp_dir_obj: tempfile.TemporaryDirectory[str] | None = None
    if args.output_dir:
        tmp_dir = Path(args.output_dir)
        tmp_dir.mkdir(parents=True, exist_ok=True)
        cleanup_tmp = False
    else:
        tmp_dir_obj = tempfile.TemporaryDirectory(prefix="tarragon_bench_")
        tmp_dir = Path(tmp_dir_obj.name)
        cleanup_tmp = True

    (tmp_dir / "cache").mkdir(exist_ok=True)

    all_results: list[TimingResult] = []

    try:
        print("[1/4] Benchmarking plain image rendering...")
        plain_results = benchmark_plain_render(iterations, tmp_dir)
        all_results.extend(plain_results)
        for r in plain_results:
            report = r.report()
            if report["count"] > 0:
                print(f"  {r.name}: {report['mean'] * 1000:.1f}ms mean")

        print()
        print("[2/4] Benchmarking cache hit vs miss...")
        cache_results = benchmark_cache_scenarios(iterations, tmp_dir)
        all_results.extend(cache_results)
        for r in cache_results:
            report = r.report()
            if report["count"] > 0:
                print(f"  {r.name}: {report['mean'] * 1000:.1f}ms mean")

        if not args.skip_psd:
            print()
            print("[3/4] Benchmarking PSD compositing...")
            try:
                import psd_tools  # noqa: F401

                psd_results = benchmark_psd_rendering(iterations, tmp_dir)
                all_results.extend(psd_results)
                for r in psd_results:
                    report = r.report()
                    if report["count"] > 0:
                        print(f"  {r.name}: {report['mean'] * 1000:.1f}ms mean")
            except ImportError:
                print("  [SKIP] psd-tools not installed — skipping PSD benchmarks")
        else:
            print()
            print("[3/4] PSD benchmarks skipped (--skip-psd)")

        if not args.skip_color:
            print()
            print("[4/4] Benchmarking color tagging...")
            color_results = benchmark_color_tagging(iterations)
            all_results.extend(color_results)
            for r in color_results:
                report = r.report()
                if report["count"] > 0:
                    print(f"  {r.name}: {report['mean'] * 1000:.1f}ms mean")
        else:
            print()
            print("[4/4] Color tagging benchmarks skipped (--skip-color)")

        print(format_report(all_results))

    finally:
        if cleanup_tmp and tmp_dir_obj is not None:
            tmp_dir_obj.cleanup()


if __name__ == "__main__":
    main()
