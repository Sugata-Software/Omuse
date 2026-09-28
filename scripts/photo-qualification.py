#!/usr/bin/env python3
"""Opt-in real-photo qualification. Requires Pillow, NumPy and ImageMagick.

Downloads only the explicitly pinned public fixtures when --fetch is given.
Results and artwork stay outside the tracked source. Reference decoding uses
Pillow/ImageMagick, separately from Omuse's Rust image decoder.
"""
import argparse
import ctypes
import hashlib
import io
import json
import os
from pathlib import Path
import resource
import subprocess
import time
import urllib.parse
import urllib.request

import numpy as np
from PIL import Image, ImageCms, ImageOps

REPO = Path(__file__).resolve().parents[1]
MANIFEST = REPO / "rust/tests/fixtures/photo-sources.json"


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def linear_profile():
    """Independent LittleCMS fixture with standard RGB primaries, gamma 1."""
    class XYy(ctypes.Structure):
        _fields_ = [(key, ctypes.c_double) for key in ("x", "y", "Y")]

    class Primaries(ctypes.Structure):
        _fields_ = [(key, XYy) for key in ("red", "green", "blue")]

    api = ctypes.CDLL("liblcms2.so.2")
    api.cmsBuildGamma.argtypes = [ctypes.c_void_p, ctypes.c_double]
    api.cmsBuildGamma.restype = ctypes.c_void_p
    api.cmsCreateRGBProfile.argtypes = [ctypes.POINTER(XYy), ctypes.POINTER(Primaries), ctypes.POINTER(ctypes.c_void_p)]
    api.cmsCreateRGBProfile.restype = ctypes.c_void_p
    api.cmsSaveProfileToMem.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.POINTER(ctypes.c_uint32)]
    api.cmsSaveProfileToMem.restype = ctypes.c_int
    api.cmsCloseProfile.argtypes = [ctypes.c_void_p]
    api.cmsFreeToneCurve.argtypes = [ctypes.c_void_p]
    curve = api.cmsBuildGamma(None, 1.0)
    if not curve:
        raise RuntimeError("Cannot construct fixture tone curve")
    profile = None
    try:
        curves = (ctypes.c_void_p * 3)(curve, curve, curve)
        white = XYy(.3127, .3290, 1)
        primaries = Primaries(XYy(.64, .33, 1), XYy(.30, .60, 1), XYy(.15, .06, 1))
        profile = api.cmsCreateRGBProfile(ctypes.byref(white), ctypes.byref(primaries), curves)
        size = ctypes.c_uint32()
        if not profile or not api.cmsSaveProfileToMem(profile, None, ctypes.byref(size)) or size.value > 1_048_576:
            raise RuntimeError("Cannot serialize fixture profile")
        buffer = ctypes.create_string_buffer(size.value)
        if not api.cmsSaveProfileToMem(profile, buffer, ctypes.byref(size)):
            raise RuntimeError("Cannot serialize fixture profile")
        return buffer.raw[:size.value]
    finally:
        if profile:
            api.cmsCloseProfile(profile)
        api.cmsFreeToneCurve(curve)


def prepare(root, fetch):
    source_root = root / "sources"
    source_root.mkdir(parents=True, exist_ok=True)
    sources = json.loads(MANIFEST.read_text())["sources"]
    for source in sources:
        path = source_root / source["name"]
        if not path.exists() and fetch:
            url = urllib.parse.quote(source["url"], safe=":/?=&")
            with urllib.request.urlopen(url, timeout=60) as response:
                data = response.read(source["bytes"] + 1)
            if len(data) != source["bytes"] or hashlib.sha256(data).hexdigest() != source["sha256"]:
                raise RuntimeError(f"Source checksum/size mismatch: {source['name']}")
            path.write_bytes(data)
        if not path.is_file() or digest(path) != source["sha256"]:
            raise RuntimeError(f"Missing or altered fixture {path}; use --fetch to download missing sources")
    fixtures = root / "fixtures"
    fixtures.mkdir(exist_ok=True)
    image = Image.open(source_root / "astronaut.png").convert("RGB")
    srgb = ImageCms.ImageCmsProfile(ImageCms.createProfile("sRGB")).tobytes()
    linear = linear_profile()
    image.save(fixtures / "portrait.png")
    image.save(fixtures / "portrait.jpg", quality=95, subsampling=0)
    image.convert("L").save(fixtures / "grayscale.jpg", quality=95)
    for orientation in [3, 6, 8]:
        exif = Image.Exif()
        exif[274] = orientation
        image.save(fixtures / f"orientation-{orientation}.jpg", quality=95, subsampling=0, exif=exif)
    rgba = np.array(image.convert("RGBA"))
    rgba[:, :, 3] = np.arange(image.width, dtype=np.uint16)[None, :] * 255 // (image.width - 1)
    Image.fromarray(rgba).save(fixtures / "transparent.png")
    image.save(fixtures / "srgb-icc.png", icc_profile=srgb)
    image.save(fixtures / "linear-icc.png", icc_profile=linear)
    image.save(fixtures / "linear-icc.tiff", icc_profile=linear)
    exact = np.array(image.convert("RGBA"), dtype=np.uint16) * 257
    low_bits = (np.indices((image.height, image.width)).sum(axis=0) % 127).astype(np.int32)
    exact[:, :, :3] = np.clip(exact[:, :, :3].astype(np.int32) + low_bits[:, :, None] - 63, 0, 65535).astype(np.uint16)
    for name in ["precision", "precision-alpha"]:
        if name.endswith("alpha"):
            exact[:, :, 3] = 18001 + np.arange(image.width, dtype=np.uint16)[None, :] * 60
        raw = fixtures / "precision.rgba16"
        raw.write_bytes(exact.astype("<u2").tobytes())
        for extension in ["png", "tiff"]:
            command = ["magick", "-size", f"{image.width}x{image.height}", "-depth", "16", "-endian", "LSB", f"rgba:{raw}"]
            if extension == "png":
                command += ["-define", "png:color-type=6", "-define", "png:bit-depth=16"]
            command += [str(fixtures / f"{name}.{extension}")]
            subprocess.run(command, check=True)
        raw.unlink()
    cases = [{"name": path.stem + "-" + path.suffix[1:], "path": str(path.resolve()), "precision": path.stem.startswith("precision"), "raw": False}
             for path in sorted(fixtures.iterdir()) if path.suffix in [".png", ".jpg", ".tiff"]]
    cases += [{"name": Path(source["name"]).stem, "path": str((source_root/source["name"]).resolve()), "precision": True, "raw": True}
              for source in sources if Path(source["name"]).suffix.lower() in [".cr2", ".nef"]]
    (root / "cases.json").write_text(json.dumps(cases, indent=2) + "\n")
    (root / "source-manifest.json").write_text(MANIFEST.read_text())
    return cases


def reference8(path):
    image = Image.open(path)
    icc = image.info.get("icc_profile")
    image = ImageOps.exif_transpose(image).convert("RGBA")
    if icc:
        profile = ImageCms.ImageCmsProfile(io.BytesIO(icc))
        srgb = ImageCms.ImageCmsProfile(ImageCms.createProfile("sRGB"))
        image = ImageCms.profileToProfile(image, profile, srgb, renderingIntent=1, outputMode="RGBA")
    result = np.array(image)
    result[result[:, :, 3] == 0] = 0
    return result


def reference16(path):
    decoded = subprocess.run(["magick", str(path), "-auto-orient", "-alpha", "on", "-depth", "16", "-endian", "LSB", "rgba:-"], check=True, capture_output=True)
    if decoded.stderr.strip():
        raise RuntimeError(f"Independent decoder warned about {path.name}: {decoded.stderr.decode(errors='replace').strip()}")
    return np.frombuffer(decoded.stdout, dtype="<u2")


def verify(case, output):
    result = json.loads((output / "results.json").read_text())
    if result.get("status") != "passed":
        raise RuntimeError("Application journey did not pass")
    checks = {}
    if not case["raw"]:
        imported = np.array(Image.open(output/"imported.png").convert("RGBA"))
        if case["precision"]:
            expected16 = reference16(Path(case["path"]))
            actual16 = reference16(output/"retained16.png")
            if not np.array_equal(expected16, actual16):
                raise RuntimeError("Independent decoder found changed 16-bit master samples")
            checks["retained16_independent_decoder"] = "exact"
        else:
            expected = reference8(case["path"])
            if imported.shape != expected.shape:
                raise RuntimeError("Import orientation/dimensions differ from independent decoder")
            error = np.abs(imported.astype(np.int16)-expected.astype(np.int16))
            tolerance = 4 if Path(case["path"]).suffix == ".jpg" else 1 if "icc" in case["name"] else 0
            if error.max() > tolerance or error.mean() > 0.5:
                raise RuntimeError(f"Import differs from independent reference: max={error.max()}, mean={error.mean():.6f}")
            checks["import_reference"] = {"max_channel_error": int(error.max()), "mean_channel_error": float(error.mean()), "allowed_max": tolerance}
    else:
        expected16 = reference16(output/"raw-reference.ppm")
        actual16 = reference16(output/"retained16.png")
        if not np.array_equal(expected16, actual16):
            raise RuntimeError("RAW development differs from independently configured LibRaw camera white balance")
        checks["raw_reference"] = {
            "comparison": "All 16-bit RGBA samples exact",
            "reference_sha256": digest(output/"raw-reference.ppm"),
            "scope": "Independent C++ camera-white-balance configuration of the same pinned LibRaw engine; not a separate camera colour-science audit",
        }
    if case["precision"]:
        expected16 = reference16(output/"retained16.png")
        for extension in ["png", "tiff"]:
            actual16 = reference16(output/f"original16.{extension}")
            if not np.array_equal(expected16, actual16):
                raise RuntimeError(f"Independent decoder found changed export16 {extension} samples")
        checks["export16_independent_decoder"] = "PNG and TIFF exact"
    expected = np.array(Image.open(output/"expected.png").convert("RGBA"))
    for extension in ["png", "tiff", "webp"]:
        actual = np.array(Image.open(output/f"export.{extension}").convert("RGBA"))
        if not np.array_equal(expected, actual):
            raise RuntimeError(f"Independent decoder found changed lossless {extension} pixels")
    checks["lossless_independent_decoder"] = "PNG, TIFF and WebP exact"
    jpeg = np.array(Image.open(output/"export.jpg").convert("RGB"))
    alpha = expected[:, :, 3:4].astype(np.uint32)
    matte = np.array(result["jpeg_matte"], dtype=np.uint32)
    reference = ((expected[:, :, :3].astype(np.uint32)*alpha + matte*(255-alpha)+127)//255).astype(np.int16)
    error = np.abs(jpeg.astype(np.int16)-reference)
    mse = np.mean(error.astype(np.float64)**2)
    psnr = float(10*np.log10(255**2/max(mse, 1e-12)))
    if error.mean() > 2.5 or psnr < 35:
        raise RuntimeError(f"JPEG error exceeds tolerance: mean={error.mean():.6f}, PSNR={psnr:.3f}")
    checks["jpeg_reference"] = {"mean_channel_error":float(error.mean()), "maximum_channel_error":int(error.max()), "psnr_db":psnr}
    return checks


def address_limit():
    resource.setrlimit(resource.RLIMIT_AS, (4*1024**3, 4*1024**3))


def execute(executable, case, output, raw_library, raw_reference):
    env = os.environ.copy()
    state = output.parent / (output.name + "-state")
    for name in ["DATA", "CONFIG", "CACHE", "STATE"]:
        env[f"XDG_{name}_HOME"] = str(state / name.lower())
    if raw_library:
        env["OMUSE_LIBRAW"] = str(raw_library)
    command = [str(executable), case["path"], str(output)] + (["--precision"] if case["precision"] else [])
    log = output.with_suffix(".log")
    started = time.monotonic()
    peak = 0
    with log.open("w") as stream:
        process = subprocess.Popen(command, stdout=stream, stderr=subprocess.STDOUT, env=env, preexec_fn=address_limit)
        try:
            while process.poll() is None:
                try:
                    status = Path(f"/proc/{process.pid}/status").read_text()
                    rss = next(int(line.split()[1]) for line in status.splitlines() if line.startswith("VmRSS:"))
                    peak = max(peak, rss)
                except (FileNotFoundError, StopIteration):
                    pass
                if peak > 3*1024**2 or time.monotonic()-started > 600:
                    raise RuntimeError("Qualification exceeded its 3 GiB RSS or 10 minute process budget")
                time.sleep(.1)
            if process.returncode:
                raise RuntimeError(f"Photo journey failed ({process.returncode}); see {log}")
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()
    if case["raw"]:
        if raw_reference is None:
            raise RuntimeError("RAW colour qualification requires --raw-reference-executable")
        reference = subprocess.run([str(raw_reference.resolve()), case["path"], str(output/"raw-reference.ppm")],
                                   check=True, capture_output=True, text=True, timeout=180,
                                   preexec_fn=address_limit)
        (output/"raw-reference.log").write_text(reference.stdout+reference.stderr)
    return {"seconds":time.monotonic()-started,"observed_peak_rss_kib":peak,"checks":verify(case,output)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("evidence", type=Path)
    parser.add_argument("--executable", type=Path, default=REPO/"rust/target/release/examples/photo_qualification")
    parser.add_argument("--raw-library", type=Path)
    parser.add_argument("--raw-reference-executable", type=Path, help="Independent C++ LibRaw reference built from scripts/fixtures/raw-photo-reference.cpp")
    parser.add_argument("--fetch", action="store_true")
    parser.add_argument("--prepare-only", action="store_true")
    parser.add_argument("--only", help="Run one exact case name")
    args = parser.parse_args()
    root = args.evidence.resolve()
    root.mkdir(parents=True, exist_ok=True)
    cases = prepare(root,args.fetch)
    if args.prepare_only:
        print(json.dumps({"cases":len(cases),"status":"prepared"})); return
    selected = [case for case in cases if not args.only or case["name"] == args.only]
    if not selected:
        parser.error("--only did not match a case")
    if any(case["raw"] for case in selected) and (args.raw_reference_executable is None or not args.raw_reference_executable.is_file()):
        parser.error("RAW cases require --raw-reference-executable")
    results = {"executable_sha256":digest(args.executable),"source_manifest_sha256":digest(MANIFEST),
               "runner_sha256":digest(Path(__file__)),"reference_versions":{"pillow":Image.__version__,"numpy":np.__version__,
               "imagemagick":subprocess.check_output(["magick","--version"],text=True).splitlines()[0]},
               "expected_cases":[case["name"] for case in selected],"complete":False,"passed":False,"cases":[]}
    if args.raw_reference_executable:
        results["raw_reference_executable_sha256"] = digest(args.raw_reference_executable)
        results["raw_reference_source_sha256"] = digest(REPO/"scripts/fixtures/raw-photo-reference.cpp")
    if args.raw_library:
        results["libraw_sha256"] = digest(args.raw_library)
    report = root/("results.json" if not args.only else f"results-{args.only}.json")
    report.write_text(json.dumps(results,indent=2)+"\n")
    for case in selected:
        row = {"case":case["name"],"input_sha256":digest(Path(case["path"]))}
        try:
            row.update(execute(args.executable.resolve(),case,root/case["name"],args.raw_library,args.raw_reference_executable))
            row["status"] = "passed"
        except Exception as error:
            row.update(status="failed",error=str(error))
        results["cases"].append(row)
        print(json.dumps(row),flush=True)
        results["complete"] = len(results["cases"]) == len(selected)
        results["passed"] = results["complete"] and all(item["status"]=="passed" for item in results["cases"])
        report.write_text(json.dumps(results,indent=2)+"\n")
    if not results["passed"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
