#!/usr/bin/env python3
"""Capture and compare the Rust editor's bounded native display probe.

The probe owns one synthetic application process and one verified Hyprland
window. It never changes desktop configuration or captures another window.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import subprocess
import sys
import time

try:
    from PIL import Image, ImageChops, ImageFilter
except ImportError as error:  # pragma: no cover - host dependency diagnostic
    raise SystemExit(f"Pillow is required for native display comparison: {error}")


CASE_TIMEOUT = 10.0
OVERALL_TIMEOUT = 120.0
BASE_CASES = [
    ("tiled", 0.37, False),
    ("reference", 0.37, False),
    ("tiled", 1.0, False),
    ("reference", 1.0, False),
    ("tiled", 1.25, False),
    ("reference", 1.25, False),
    ("tiled", 2.0, False),
    ("reference", 2.0, False),
    ("tiled", 1.25, True),
    ("reference", 1.25, True),
]
EXPECTED = [
    (*case, matte)
    for matte in ("checkerboard", "black", "white")
    for case in BASE_CASES
]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executable", type=Path)
    parser.add_argument("evidence", type=Path)
    parser.add_argument("--desktop-env", type=Path)
    return parser.parse_args()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def source_digest(repo_root: Path) -> tuple[str, int]:
    paths = [repo_root / "rust" / "Cargo.toml", repo_root / "rust" / "Cargo.lock"]
    paths.extend(sorted((repo_root / "rust" / "src").rglob("*.rs")))
    paths.extend(sorted((repo_root / "rust" / "tests").rglob("*.rs")))
    paths.extend(sorted((repo_root / "rust" / "examples").rglob("*.rs")))
    digest = hashlib.sha256()
    count = 0
    for path in sorted({path.resolve() for path in paths if path.is_file()}):
        relative = path.relative_to(repo_root.resolve()).as_posix().encode()
        digest.update(len(relative).to_bytes(8, "big"))
        digest.update(relative)
        data = path.read_bytes()
        digest.update(len(data).to_bytes(8, "big"))
        digest.update(data)
        count += 1
    return digest.hexdigest(), count


def write_json(path: Path, value: object) -> None:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def image_pixels(image: Image.Image):
    """Use Pillow's current pixel iterator while retaining older compatibility."""
    flattened = getattr(image, "get_flattened_data", None)
    return flattened() if flattened is not None else image.getdata()


def nonzero_pixel_count(mask: Image.Image) -> int:
    histogram = mask.histogram()
    return sum(histogram[1:])


def any_channel_nonzero(image: Image.Image) -> Image.Image:
    channels = image.split()
    combined = channels[0]
    for channel in channels[1:]:
        combined = ImageChops.lighter(combined, channel)
    return combined


def outside_envelope_mask(
    observed: Image.Image,
    minimum: Image.Image,
    maximum: Image.Image,
    tolerance: int,
) -> Image.Image:
    lower = minimum.point([max(0, value - tolerance) for value in range(256)] * 3)
    upper = maximum.point([min(255, value + tolerance) for value in range(256)] * 3)
    below = any_channel_nonzero(ImageChops.subtract(lower, observed))
    above = any_channel_nonzero(ImageChops.subtract(observed, upper))
    return ImageChops.lighter(below, above)


def wait_for(
    path: Path,
    process: subprocess.Popen,
    overall_deadline: float,
    native_error: Path,
) -> None:
    deadline = min(time.monotonic() + CASE_TIMEOUT, overall_deadline)
    while not path.exists():
        if native_error.exists():
            raise RuntimeError(native_error.read_text(encoding="utf-8"))
        if process.poll() is not None:
            raise RuntimeError(f"Native display probe exited {process.returncode} while waiting for {path.name}")
        if time.monotonic() >= deadline:
            raise TimeoutError(f"Timed out waiting for {path.name}")
        time.sleep(0.05)


def validate_case(case: dict, index: int) -> None:
    mode, zoom, edited, matte = EXPECTED[index]
    if case.get("index") != index or case.get("mode") != mode or case.get("edited") is not edited:
        raise RuntimeError(f"Unexpected case-{index} metadata: {case!r}")
    actual_zoom = case.get("zoom")
    if not isinstance(actual_zoom, (int, float)) or abs(float(actual_zoom) - zoom) > 1e-6:
        raise RuntimeError(f"Unexpected case-{index} zoom: {actual_zoom!r}")
    scale = case.get("scale_factor")
    if not isinstance(scale, (int, float)) or not (0.25 <= float(scale) <= 8.0):
        raise RuntimeError(f"Invalid case-{index} scale factor: {scale!r}")
    viewport = case.get("viewport")
    if not isinstance(viewport, dict) or any(
        not isinstance(viewport.get(key), (int, float))
        for key in ("x", "y", "width", "height")
    ):
        raise RuntimeError(f"Invalid case-{index} viewport: {viewport!r}")
    if viewport["width"] <= 4 or viewport["height"] <= 4:
        raise RuntimeError(f"Unusable case-{index} viewport: {viewport!r}")
    if case.get("comparison_reference") != "monolithic_with_clamped_outer_halo":
        raise RuntimeError(
            f"Unexpected case-{index} comparison reference: "
            f"{case.get('comparison_reference')!r}"
        )
    if case.get("matte") != matte:
        raise RuntimeError(f"Unexpected case-{index} matte: {case.get('matte')!r}")


def owned_window(hypr, pid: int) -> dict:
    matches = [
        client
        for client in json.loads(hypr("-j", "clients"))
        if client.get("pid") == pid
        and client.get("class") == "omuse"
        and client.get("mapped")
    ]
    if len(matches) != 1:
        raise RuntimeError("Expected exactly one mapped omuse window owned by the probe PID")
    return matches[0]


def wait_for_owned_window(
    hypr,
    pid: int,
    process: subprocess.Popen,
    overall_deadline: float,
) -> dict:
    deadline = min(time.monotonic() + 3.0, overall_deadline)
    while True:
        matches = [
            client
            for client in json.loads(hypr("-j", "clients"))
            if client.get("pid") == pid
            and client.get("class") == "omuse"
            and client.get("mapped")
        ]
        if len(matches) > 1:
            raise RuntimeError("Probe PID owns more than one mapped omuse window")
        if len(matches) == 1:
            return matches[0]
        if process.poll() is not None:
            raise RuntimeError(
                f"Native display probe exited {process.returncode} before its window mapped"
            )
        if time.monotonic() >= deadline:
            raise TimeoutError("Native display-probe window did not map after window-ready")
        time.sleep(0.05)


def focus_and_verify(hypr, pid: int, address: str, maximize: bool = False) -> dict:
    hypr("dispatch", f'hl.dsp.focus({{ window = "address:{address}" }})')
    time.sleep(0.15)
    active = json.loads(hypr("-j", "activewindow"))
    if active.get("pid") != pid or active.get("address") != address:
        raise RuntimeError("Owned display-probe window did not become active")
    if maximize and not active.get("fullscreen"):
        hypr("dispatch", 'hl.dsp.window.fullscreen({ mode = "maximized" })')
        time.sleep(0.25)
        active = json.loads(hypr("-j", "activewindow"))
        if active.get("pid") != pid or active.get("address") != address:
            raise RuntimeError("Owned display-probe window lost focus while maximizing")
    width, height = active.get("size", [0, 0])
    if width < 800 or height < 600:
        raise RuntimeError(f"Display-probe window is not usable: {width}x{height}")
    return active


def viewport_crop(image: Image.Image, case: dict) -> tuple[Image.Image, dict]:
    viewport = case["viewport"]
    scale = float(case["scale_factor"])
    left = round(float(viewport["x"]) * scale) + 2
    top = round(float(viewport["y"]) * scale) + 2
    right = round((float(viewport["x"]) + float(viewport["width"])) * scale) - 2
    bottom = round((float(viewport["y"]) + float(viewport["height"])) * scale) - 2
    if left < 0 or top < 0 or right > image.width or bottom > image.height or right <= left or bottom <= top:
        raise RuntimeError(
            f"Viewport crop {(left, top, right, bottom)} is outside screenshot {image.size}"
        )
    return image.crop((left, top, right, bottom)).convert("RGB"), {
        "left": left,
        "top": top,
        "right": right,
        "bottom": bottom,
    }


def seam_samples(case: dict, crop: Image.Image) -> list[dict]:
    coordinates = case.get("seam_coordinates", case.get("seams", []))
    if not isinstance(coordinates, list):
        return []
    scale = float(case["scale_factor"])
    samples = []
    for coordinate in coordinates:
        if not isinstance(coordinate, dict) or not all(
            isinstance(coordinate.get(key), (int, float)) for key in ("x", "y")
        ):
            continue
        x = round(float(coordinate["x"]) * scale) - 2
        y = round(float(coordinate["y"]) * scale) - 2
        if 0 <= x < crop.width and 0 <= y < crop.height:
            box = (max(0, x - 1), max(0, y - 1), min(crop.width, x + 2), min(crop.height, y + 2))
            colors = sorted({tuple(pixel) for pixel in image_pixels(crop.crop(box))})
            samples.append(
                {
                    "coordinate": coordinate,
                    "devicePixel": [x, y],
                    "sampleBox": list(box),
                    "rgb": colors,
                    "constantColor": len(colors) == 1,
                }
            )
    return samples


def constant_band_seam_check(
    tiled: dict,
    crop_box: dict,
    tiled_crop: Image.Image,
    outside_mask: Image.Image,
) -> dict:
    """Strictly compare flat fixture regions spanning vertical tile seams."""
    canvas = tiled.get("canvas")
    dimensions = tiled.get("source_dimensions")
    if not isinstance(canvas, dict) or any(
        not isinstance(canvas.get(key), (int, float))
        for key in ("x", "y", "width", "height")
    ):
        raise RuntimeError(f"Invalid case-{tiled['index']} canvas: {canvas!r}")
    if (
        not isinstance(dimensions, list)
        or len(dimensions) != 2
        or not all(isinstance(value, int) and value > 0 for value in dimensions)
    ):
        raise RuntimeError(
            f"Invalid case-{tiled['index']} source dimensions: {dimensions!r}"
        )

    scale = float(tiled["scale_factor"])
    zoom = float(tiled["zoom"])
    samples = []
    total_pixels = 0
    total_outside = 0
    for source_x in (256, 512, 768):
        if source_x >= dimensions[0]:
            continue
        device_x = (float(canvas["x"]) + source_x * zoom) * scale - crop_box["left"]
        for band in range(3):
            source_top = band * 256 + 108
            source_bottom = min(band * 256 + 132, dimensions[1])
            if source_top >= source_bottom:
                continue
            device_top = (
                (float(canvas["y"]) + source_top * zoom) * scale - crop_box["top"]
            )
            device_bottom = (
                (float(canvas["y"]) + source_bottom * zoom) * scale - crop_box["top"]
            )
            box = (
                max(0, math.floor(device_x - 2)),
                max(0, math.floor(device_top)),
                min(tiled_crop.width, math.ceil(device_x + 2)),
                min(tiled_crop.height, math.ceil(device_bottom)),
            )
            if box[2] <= box[0] or box[3] <= box[1]:
                continue
            sample_mask = outside_mask.crop(box)
            outside = nonzero_pixel_count(sample_mask)
            pixels = sample_mask.width * sample_mask.height
            total_pixels += pixels
            total_outside += outside
            samples.append(
                {
                    "sourceSeamX": source_x,
                    "sourceBandY": [source_top, source_bottom],
                    "deviceSeamX": device_x,
                    "sampleBox": list(box),
                    "samplePixels": pixels,
                    "outsideReference3x3EnvelopePixels": outside,
                    "passed": outside == 0,
                }
            )
    return {
        "description": "Strict fixture constant-band samples within two device pixels of internal vertical tile seams",
        "samplePixels": total_pixels,
        "outsideReference3x3EnvelopePixels": total_outside,
        "outsideTolerance": 2,
        "passed": total_pixels > 0 and total_outside == 0,
        "samples": samples,
    }


def compare_pair(evidence: Path, tiled: dict, reference: dict) -> dict:
    tiled_image = Image.open(evidence / f"case-{tiled['index']}.png")
    reference_image = Image.open(evidence / f"case-{reference['index']}.png")
    tiled_crop, tiled_box = viewport_crop(tiled_image, tiled)
    reference_crop, reference_box = viewport_crop(reference_image, reference)
    width = min(tiled_crop.width, reference_crop.width)
    height = min(tiled_crop.height, reference_crop.height)
    if abs(tiled_crop.width - reference_crop.width) > 1 or abs(tiled_crop.height - reference_crop.height) > 1:
        raise RuntimeError(
            f"Viewport interiors differ by more than one device pixel: {tiled_crop.size} vs {reference_crop.size}"
        )
    tiled_crop = tiled_crop.crop((0, 0, width, height))
    reference_crop = reference_crop.crop((0, 0, width, height))
    difference = ImageChops.difference(tiled_crop, reference_crop)
    difference_path = evidence / f"case-{tiled['index']}-{reference['index']}-diff.png"
    difference.save(difference_path)

    histogram = difference.histogram()
    channel_pixels = width * height * 3
    total_delta = sum((index % 256) * count for index, count in enumerate(histogram))
    extrema = difference.getextrema()
    max_delta = max(high for _, high in extrema)
    exact_different = nonzero_pixel_count(any_channel_nonzero(difference))

    minimum = reference_crop.filter(ImageFilter.MinFilter(3))
    maximum = reference_crop.filter(ImageFilter.MaxFilter(3))
    outside_mask = outside_envelope_mask(tiled_crop, minimum, maximum, 2)
    outside = nonzero_pixel_count(outside_mask)
    pixels = width * height
    outside_fraction = outside / pixels
    constant_bands = constant_band_seam_check(
        tiled, tiled_box, tiled_crop, outside_mask
    )
    matte = tiled["matte"]
    uniform_matte = matte in ("black", "white")
    return {
        "tiledIndex": tiled["index"],
        "referenceIndex": reference["index"],
        "zoom": tiled["zoom"],
        "edited": tiled["edited"],
        "matte": matte,
        "comparedSize": [width, height],
        "tiledViewportCrop": tiled_box,
        "referenceViewportCrop": reference_box,
        "exactDifferentPixels": exact_different,
        "exactDifferentFraction": exact_different / pixels,
        "meanAbsoluteChannelDifference": total_delta / channel_pixels,
        "maximumChannelDifference": max_delta,
        "outsideReference3x3EnvelopePixels": outside,
        "outsideReference3x3EnvelopeFraction": outside_fraction,
        "outsideTolerance": 2,
        "allowedOutsideFraction": 0.0001,
        "envelopeGateApplied": uniform_matte,
        "envelopeDiagnosticPassed": outside_fraction <= 0.0001,
        "envelopeDiagnosticReason": None
        if uniform_matte
        else "Composited checker phase coupling makes a neighborhood RGB envelope unsuitable as a correctness invariant; retained as a diagnostic.",
        "passed": constant_bands["passed"]
        and (not uniform_matte or outside_fraction <= 0.0001),
        "diffImage": difference_path.name,
        "tiledSeamSamples": seam_samples(tiled, tiled_crop),
        "referenceSeamSamples": seam_samples(reference, reference_crop),
        "constantBandSeamCheck": constant_bands,
    }


def channel_diagnostics(image: Image.Image) -> list[dict]:
    diagnostics = []
    pixels = image.width * image.height
    for name, channel in zip(("red", "green", "blue"), image.split()):
        histogram = channel.histogram()
        diagnostics.append(
            {
                "channel": name,
                "minimum": channel.getextrema()[0],
                "maximum": channel.getextrema()[1],
                "mean": sum(value * count for value, count in enumerate(histogram)) / pixels,
            }
        )
    return diagnostics


def derived_alpha(black: Image.Image, white: Image.Image) -> tuple[Image.Image, int]:
    if black.size != white.size:
        raise RuntimeError(f"Black/white matte crops differ: {black.size} vs {white.size}")
    underflow = nonzero_pixel_count(
        any_channel_nonzero(ImageChops.subtract(black, white))
    )
    return ImageChops.invert(ImageChops.subtract(white, black)), underflow


def alpha_band_observations(alpha: Image.Image, case: dict, crop_box: dict) -> dict:
    canvas = case["canvas"]
    scale = float(case["scale_factor"])
    zoom = float(case["zoom"])
    samples = []
    outside = 0
    pixels = 0
    for source_x in (256, 512, 768):
        device_x = (float(canvas["x"]) + source_x * zoom) * scale - crop_box["left"]
        for band in range(3):
            source_top, source_bottom = band * 256 + 108, band * 256 + 132
            device_top = (float(canvas["y"]) + source_top * zoom) * scale - crop_box["top"]
            device_bottom = (float(canvas["y"]) + source_bottom * zoom) * scale - crop_box["top"]
            box = (
                max(0, math.floor(device_x - 2)),
                max(0, math.floor(device_top)),
                min(alpha.width, math.ceil(device_x + 2)),
                min(alpha.height, math.ceil(device_bottom)),
            )
            if box[2] <= box[0] or box[3] <= box[1]:
                continue
            sample = alpha.crop(box)
            sample_outside = 0
            extrema = []
            for channel in sample.split():
                histogram = channel.histogram()
                sample_outside += sum(histogram[:173]) + sum(histogram[178:])
                extrema.append(list(channel.getextrema()))
            count = sample.width * sample.height * 3
            outside += sample_outside
            pixels += count
            samples.append(
                {
                    "sourceSeamX": source_x,
                    "sourceBandY": [source_top, source_bottom],
                    "sampleBox": list(box),
                    "channelSamples": count,
                    "outsideExpected175Tolerance2": sample_outside,
                    "channelExtrema": extrema,
                }
            )
    return {
        "expectedAlpha": 175,
        "tolerance": 2,
        "channelSamples": pixels,
        "outsideExpectedTolerance": outside,
        "samples": samples,
    }


def compare_alpha_pair(
    evidence: Path,
    black_tiled: dict,
    black_reference: dict,
    white_tiled: dict,
    white_reference: dict,
) -> dict:
    crops = []
    boxes = []
    for case in (black_tiled, black_reference, white_tiled, white_reference):
        crop, box = viewport_crop(Image.open(evidence / f"case-{case['index']}.png"), case)
        crops.append(crop)
        boxes.append(box)
    width = min(crop.width for crop in crops)
    height = min(crop.height for crop in crops)
    crops = [crop.crop((0, 0, width, height)) for crop in crops]
    tiled_alpha, tiled_underflow = derived_alpha(crops[0], crops[2])
    reference_alpha, reference_underflow = derived_alpha(crops[1], crops[3])
    minimum = reference_alpha.filter(ImageFilter.MinFilter(3))
    maximum = reference_alpha.filter(ImageFilter.MaxFilter(3))
    outside = nonzero_pixel_count(
        outside_envelope_mask(tiled_alpha, minimum, maximum, 2)
    )
    pixels = width * height
    tiled_bands = alpha_band_observations(tiled_alpha, black_tiled, boxes[0])
    reference_bands = alpha_band_observations(reference_alpha, black_reference, boxes[1])
    source_alpha_supported = (
        reference_bands["channelSamples"] > 0
        and reference_bands["outsideExpectedTolerance"] == 0
    )
    source_alpha_passed = (
        tiled_bands["outsideExpectedTolerance"] == 0
        if source_alpha_supported
        else None
    )
    return {
        "tiledBlackIndex": black_tiled["index"],
        "referenceBlackIndex": black_reference["index"],
        "tiledWhiteIndex": white_tiled["index"],
        "referenceWhiteIndex": white_reference["index"],
        "zoom": black_tiled["zoom"],
        "edited": black_tiled["edited"],
        "comparedSize": [width, height],
        "outsideReference3x3EnvelopePixels": outside,
        "outsideReference3x3EnvelopeFraction": outside / pixels,
        "outsideTolerance": 2,
        "allowedOutsideFraction": 0.0001,
        "tiledWhiteBelowBlackPixels": tiled_underflow,
        "referenceWhiteBelowBlackPixels": reference_underflow,
        "tiledDerivedAlphaChannels": channel_diagnostics(tiled_alpha),
        "referenceDerivedAlphaChannels": channel_diagnostics(reference_alpha),
        "constantBandSourceAlpha175": {
            "supportedByReferenceOutput": source_alpha_supported,
            "unsupportedReason": None
            if source_alpha_supported
            else "Reference matte captures do not reproduce source alpha 175 within tolerance 2; observations retained without asserting that invariant.",
            "tiled": tiled_bands,
            "reference": reference_bands,
            "passed": source_alpha_passed,
        },
        "passed": outside / pixels <= 0.0001
        and tiled_underflow == 0
        and reference_underflow == 0
        and (source_alpha_passed is not False),
    }


def main() -> int:
    args = parse_args()
    executable = args.executable.resolve()
    evidence = args.evidence.resolve()
    if not executable.is_file() or not os.access(executable, os.X_OK):
        raise SystemExit(f"Executable is missing or not executable: {executable}")
    if evidence.exists() and any(evidence.iterdir()):
        raise SystemExit("Use a fresh evidence directory so stale results cannot pass.")
    evidence.mkdir(parents=True, exist_ok=True)

    repo_root = Path(__file__).resolve().parent.parent
    harness_sha256 = sha256_file(Path(__file__).resolve())
    source_sha256, source_file_count = source_digest(repo_root)
    binary_sha256 = sha256_file(executable)
    environment = os.environ.copy()
    if args.desktop_env:
        source = json.loads(args.desktop_env.read_text(encoding="utf-8"))
        for key in (
            "WAYLAND_DISPLAY",
            "XDG_RUNTIME_DIR",
            "DBUS_SESSION_BUS_ADDRESS",
            "HYPRLAND_INSTANCE_SIGNATURE",
        ):
            if key in source:
                environment[key] = source[key]
    for key, folder in (
        ("XDG_DATA_HOME", "data"),
        ("XDG_CONFIG_HOME", "config"),
        ("XDG_CACHE_HOME", "cache"),
        ("XDG_STATE_HOME", "state"),
    ):
        environment[key] = str(evidence / folder)
        Path(environment[key]).mkdir()
    environment["OMUSE_NATIVE_DISPLAY_PROBE"] = "1"
    environment["OMUSE_NATIVE_WAIT"] = "1"

    app_result_path = evidence / "display-results.json"
    native_error = evidence / "native-error.txt"
    overall_deadline = time.monotonic() + OVERALL_TIMEOUT
    started = time.time()
    process = None
    report: dict = {}
    log_path = evidence / "native-display.log"
    try:
        with log_path.open("w", encoding="utf-8") as log:
            process = subprocess.Popen(
                [str(executable), "--ui-smoke", str(evidence)],
                env=environment,
                stdout=log,
                stderr=log,
            )

            def hypr(*command: str) -> str:
                return subprocess.check_output(
                    ["hyprctl", *command], env=environment, text=True, timeout=2
                )

            wait_for(evidence / "window-ready", process, overall_deadline, native_error)
            window = wait_for_owned_window(hypr, process.pid, process, overall_deadline)
            address = window["address"]
            active = focus_and_verify(hypr, process.pid, address, maximize=True)
            launch_window = {
                key: active.get(key)
                for key in ("pid", "class", "address", "size", "at", "fullscreen")
            }
            write_json(evidence / "launch-window.json", launch_window)
            (evidence / "start").write_text("start\n", encoding="utf-8")

            cases = []
            for index in range(len(EXPECTED)):
                case_deadline = min(time.monotonic() + CASE_TIMEOUT, overall_deadline)
                ready = evidence / f"case-{index}-ready.json"
                wait_for(ready, process, case_deadline, native_error)
                case = json.loads(ready.read_text(encoding="utf-8"))
                validate_case(case, index)
                owned = owned_window(hypr, process.pid)
                if owned.get("address") != address:
                    raise RuntimeError("Display-probe window address changed during capture")
                active = focus_and_verify(hypr, process.pid, address)
                x, y = active["at"]
                width, height = active["size"]
                screenshot = evidence / f"case-{index}.png"
                remaining = case_deadline - time.monotonic()
                if remaining <= 0:
                    raise TimeoutError(f"Case {index} exceeded its {CASE_TIMEOUT:g}-second budget")
                subprocess.run(
                    ["grim", "-g", f"{x},{y} {width}x{height}", str(screenshot)],
                    env=environment,
                    check=True,
                    timeout=remaining,
                )
                with Image.open(screenshot) as captured:
                    if captured.width <= 0 or captured.height <= 0:
                        raise RuntimeError(f"Empty screenshot for case {index}")
                    capture_size = [captured.width, captured.height]
                expected_width = round(width * float(case["scale_factor"]))
                expected_height = round(height * float(case["scale_factor"]))
                if abs(capture_size[0] - expected_width) > 2 or abs(capture_size[1] - expected_height) > 2:
                    raise RuntimeError(
                        f"Case {index} capture size {capture_size} does not match owned window "
                        f"{width}x{height} at scale {case['scale_factor']}"
                    )
                case["capture"] = {
                    "file": screenshot.name,
                    "sha256": sha256_file(screenshot),
                    "pixelSize": capture_size,
                    "window": {
                        key: active.get(key)
                        for key in ("pid", "class", "address", "size", "at", "fullscreen")
                    },
                }
                cases.append(case)
                (evidence / f"case-{index}-captured").write_text("captured\n", encoding="utf-8")
                if time.monotonic() > case_deadline:
                    raise TimeoutError(f"Case {index} exceeded its {CASE_TIMEOUT:g}-second budget")

            wait_for(app_result_path, process, overall_deadline, native_error)
            if native_error.exists():
                raise RuntimeError(native_error.read_text(encoding="utf-8"))
            app_result = json.loads(app_result_path.read_text(encoding="utf-8"))
            if app_result.get("status") != "passed":
                raise RuntimeError(f"Native display probe did not pass: {app_result!r}")
            if app_result.get("cases") != len(EXPECTED):
                raise RuntimeError(
                    f"Native display probe reported {app_result.get('cases')!r} cases; "
                    f"expected {len(EXPECTED)}"
                )

            comparisons = []
            for index in range(0, len(cases), 2):
                comparisons.append(compare_pair(evidence, cases[index], cases[index + 1]))
                if time.monotonic() > overall_deadline:
                    raise TimeoutError(f"Native display check exceeded {OVERALL_TIMEOUT:g} seconds")
            alpha_comparisons = []
            for offset in range(0, len(BASE_CASES), 2):
                alpha_comparisons.append(
                    compare_alpha_pair(
                        evidence,
                        cases[10 + offset],
                        cases[11 + offset],
                        cases[20 + offset],
                        cases[21 + offset],
                    )
                )
                if time.monotonic() > overall_deadline:
                    raise TimeoutError(f"Native display check exceeded {OVERALL_TIMEOUT:g} seconds")
            report = {
                "status": "passed"
                if all(item["passed"] for item in comparisons)
                and all(item["passed"] for item in alpha_comparisons)
                else "failed",
                "purpose": "bounded native tiled/reference display comparison across checkerboard, black, and white mattes",
                "snappingLimit": "Independent GPUI quads may snap up to one device pixel differently at fractional scale. Comparisons exclude a two-device-pixel viewport boundary. Uniform matte and derived-alpha comparisons use a 3x3 reference envelope with per-channel tolerance 2; checkerboard envelope results are diagnostic because spatial matte phase couples with foreground sampling.",
                "gatePolicy": {
                    "checkerboard": "strict constant-band seam samples; whole-viewport RGB envelope retained as diagnostic",
                    "uniformMattes": "strict constant-band seam samples and whole-viewport RGB envelope",
                    "derivedAlpha": "black/white-derived tiled alpha against monolithic alpha envelope; source-alpha-175 assertion when supported by reference output",
                },
                "binary": {"name": executable.name, "sha256": binary_sha256},
                "harnessSourceSha256": harness_sha256,
                "source": {"sha256": source_sha256, "fileCount": source_file_count},
                "startedUnixSeconds": started,
                "finishedUnixSeconds": time.time(),
                "durationSeconds": time.monotonic() - (overall_deadline - OVERALL_TIMEOUT),
                "window": launch_window,
                "cases": cases,
                "comparisons": comparisons,
                "alphaComparisons": alpha_comparisons,
                "appResult": app_result,
            }
            write_json(evidence / "verified-display-results.json", report)
            if report["status"] != "passed":
                raise RuntimeError("One or more tiled/reference display comparisons exceeded tolerance")
            print(json.dumps(report, indent=2, sort_keys=True))
            return 0
    except Exception as error:
        failure = {
            "status": "failed",
            "error": str(error),
            "binary": {"name": executable.name, "sha256": binary_sha256},
            "harnessSourceSha256": harness_sha256,
            "source": {"sha256": source_sha256, "fileCount": source_file_count},
            "finishedUnixSeconds": time.time(),
        }
        write_json(evidence / "display-check-error.json", failure)
        print(f"Native display check failed: {error}", file=sys.stderr)
        return 1
    finally:
        # Only the exact synthetic child started above is ever signalled.
        if process is not None and process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)


if __name__ == "__main__":
    raise SystemExit(main())
