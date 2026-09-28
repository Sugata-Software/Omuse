#!/usr/bin/env python3
"""Regenerate portable Camera Raw references from the preserved upstream C source."""
import ctypes as c
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[1]
sources = [root / 'Compositor/Rendering' / f for f in ('AdjustPixels.c', 'LensPixels.c')]
with tempfile.TemporaryDirectory(prefix='compositor-reference-') as temp:
    library = Path(temp) / 'reference.so'
    subprocess.run(['cc', '-O2', '-shared', '-fPIC', *map(str, sources), '-lm', '-o', str(library)], check=True)
    lib = c.CDLL(str(library))
    basic = lib.adjust_camera_raw
    basic.argtypes = [c.POINTER(c.c_uint8), c.c_size_t, c.c_size_t, c.c_size_t] + [c.c_double] * 11 + [c.c_int]
    calibrate = lib.adjust_camera_raw_calibration
    calibrate.argtypes = [c.POINTER(c.c_uint8), c.c_size_t, c.c_size_t, c.c_size_t] + [c.c_double] * 7 + [c.c_int]
    effects = lib.adjust_camera_raw_effects
    effects.argtypes = [c.POINTER(c.c_uint8), c.c_size_t, c.c_size_t, c.c_size_t] + [c.c_double] * 4 + [c.c_int] + [c.c_double] * 8 + [c.c_int, c.c_double]
    detail = lib.adjust_camera_raw_detail
    detail.argtypes = [c.POINTER(c.c_uint8), c.c_size_t, c.c_size_t, c.c_size_t] + [c.c_double] * 11
    optics = lib.adjust_camera_raw_optics
    optics.argtypes = [c.POINTER(c.c_uint8), c.c_size_t, c.c_size_t, c.c_size_t, c.c_int, c.c_int] + [c.c_double] * 12
    # Broad colors, extrema, semitransparency, and hidden RGB at zero alpha.
    pixels = [[(i * 37) % 256, (i * 73 + 11) % 256, (i * 131 + 47) % 256, [255, 255, 128, 37, 0][i % 5]] for i in range(80)]
    cases = []
    for name, settings in [
        ('identity', {}),
        ('positive', dict(exposure=1.5, temperature=25, tint=-20, contrast=40, highlights=-60, shadows=55, whites=20, blacks=-30, vibrance=50, saturation=15)),
        ('negative', dict(exposure=-2, temperature=-40, tint=30, contrast=-55, highlights=30, shadows=-40, whites=-60, blacks=70, vibrance=-65, saturation=-45)),
        ('extrema', dict(exposure=5, contrast=100, highlights=-100, shadows=100, whites=-100, blacks=100, vibrance=100, saturation=-100)),
        ('calibration', {'calibration': dict(shadowTint=30, redHue=-20, redSaturation=45, greenHue=35, greenSaturation=-30, blueHue=-50, blueSaturation=20, process=6)}),
    ]:
        raw = [v for p in pixels for v in [*((channel * p[3] + 127) // 255 for channel in p[:3]), p[3]]]
        data = (c.c_uint8 * len(raw))(*raw)
        calibration = settings.get('calibration')
        if calibration:
            calibrate(data, 10, 8, 40, *(calibration[k] for k in ('shadowTint','redHue','redSaturation','greenHue','greenSaturation','blueHue','blueSaturation')), calibration['process'])
        w, m = settings.get('temperature', 0)/100, settings.get('tint',0)/100
        basic(data, 10, 8, 40, 1+.35*w+.15*m, 1-.3*m, 1-.35*w+.15*m,
              *(settings.get(k,0) for k in ('exposure','contrast','highlights','shadows','whites','blacks','vibrance','saturation')), 0)
        cases.append(dict(name=name, settings=settings, expectedPremultiplied=list(data)))
    output = dict(width=10, height=8, input=[v for p in pixels for v in p], sourceSha256={s.name:hashlib.sha256(s.read_bytes()).hexdigest() for s in sources}, cases=cases)
    dest = root / 'rust/tests/fixtures/camera-basic-reference.json'
    dest.parent.mkdir(exist_ok=True)
    dest.write_text(json.dumps(output, indent=2)+'\n')
    print(dest)

    # A larger, spatially varied source independently exercises the portable post-grade stages.
    # Core Image and RAW decoding are intentionally outside this fixture's claim.
    width, height = 18, 14
    advanced_pixels = []
    for y in range(height):
        for x in range(width):
            i = y * width + x
            advanced_pixels.append([
                (x * 29 + y * 17 + (i % 3) * 61) % 256,
                (x * 11 + y * 47 + (i % 5) * 31) % 256,
                (x * 53 + y * 7 + (i % 7) * 19) % 256,
                # The preserved color-noise C kernel leaves its chroma scratch value
                # uninitialized for zero-alpha pixels, so this spatial fixture uses
                # opaque and semitransparent pixels only. The basic fixture above still
                # covers zero-alpha/hidden-RGB behavior deterministically.
                [255, 219, 128, 43][i % 4],
            ])

    ordered_settings = dict(
        texture=-24, clarity=37, dehaze=-18, glow=27, glowStyle='Bloom',
        glowRange=35, glowSpread=18, glowWarmth=-26, vignetteAmount=31,
        vignetteStyle='Color Priority', vignetteMidpoint=56, vignetteRoundness=-35,
        vignetteFeather=42, vignetteHighlights=20,
        optics=dict(removeChromaticAberration=True, enableLensProfile=False,
                    profileDistortion=100, profileVignetting=100, distortion=13,
                    purpleAmount=31, purpleHueLow=270, purpleHueHigh=310,
                    greenAmount=22, greenHueLow=60, greenHueHigh=120,
                    vignetteAmount=19, vignetteMidpoint=61),
        detail=dict(sharpenAmount=44, sharpenRadius=62, sharpenDetail=38,
                    sharpenMasking=46, noiseLuminance=29, noiseLuminanceDetail=35,
                    noiseLuminanceContrast=18, noiseColor=26, noiseColorDetail=41,
                    noiseColorSmoothness=32))
    ordered_effects = {k: copy.deepcopy(v) for k, v in ordered_settings.items()
                       if k not in ('optics', 'detail')}
    ordered_effects_optics = copy.deepcopy(ordered_effects)
    ordered_effects_optics['optics'] = copy.deepcopy(ordered_settings['optics'])
    ordered_luminance = copy.deepcopy(ordered_effects_optics)
    ordered_luminance['detail'] = {
        k: ordered_settings['detail'][k] for k in
        ('noiseLuminance', 'noiseLuminanceDetail', 'noiseLuminanceContrast')
    }
    ordered_luminance_color = copy.deepcopy(ordered_luminance)
    ordered_luminance_color['detail'].update({
        k: ordered_settings['detail'][k] for k in
        ('noiseColor', 'noiseColorDetail', 'noiseColorSmoothness')
    })

    advanced_cases = [
        ('effects', dict(texture=42, clarity=-31, dehaze=28, glow=36,
                         glowStyle='Halation', glowRange=22, glowSpread=45, glowWarmth=33,
                         vignetteAmount=-38, vignetteMidpoint=44, vignetteRoundness=27,
                         vignetteFeather=63, vignetteHighlights=51,
                         vignetteStyle='Highlight Priority')),
        ('optics', dict(optics=dict(removeChromaticAberration=True, enableLensProfile=True,
                                    profileDistortion=24, profileVignetting=38, distortion=-17,
                                    purpleAmount=47, purpleHueLow=265, purpleHueHigh=318,
                                    greenAmount=29, greenHueLow=55, greenHueHigh=128,
                                    vignetteAmount=-21, vignetteMidpoint=37))),
        ('detail-luminance-noise', dict(detail=dict(noiseLuminance=41,
                                                    noiseLuminanceDetail=63,
                                                    noiseLuminanceContrast=22))),
        ('detail-color-noise', dict(detail=dict(noiseColor=34, noiseColorDetail=57,
                                                noiseColorSmoothness=71))),
        ('detail-sharpen', dict(detail=dict(sharpenAmount=58, sharpenRadius=36,
                                            sharpenDetail=67, sharpenMasking=24))),
        ('detail', dict(detail=dict(sharpenAmount=58, sharpenRadius=36, sharpenDetail=67,
                                    sharpenMasking=24, noiseLuminance=41,
                                    noiseLuminanceDetail=63, noiseLuminanceContrast=22,
                                    noiseColor=34, noiseColorDetail=57,
                                    noiseColorSmoothness=71))),
        ('ordered-effects', ordered_effects),
        ('ordered-effects-optics', ordered_effects_optics),
        ('ordered-effects-optics-luminance', ordered_luminance),
        ('ordered-effects-optics-luminance-color', ordered_luminance_color),
        ('effects-optics-detail', ordered_settings),
    ]
    generated = []
    style_number = {'Diffusion': 0, 'Bloom': 1, 'Halation': 2}
    vignette_number = {'Highlight Priority': 0, 'Color Priority': 1, 'Paint Overlay': 2}
    for name, settings in advanced_cases:
        raw = [v for p in advanced_pixels for v in [*((channel * p[3] + 127) // 255 for channel in p[:3]), p[3]]]
        data = (c.c_uint8 * len(raw))(*raw)
        if any(settings.get(k, 0) != 0 for k in ('texture', 'clarity', 'dehaze', 'glow', 'vignetteAmount')):
            effects(data, width, height, width * 4,
                    settings.get('texture', 0), settings.get('clarity', 0), settings.get('dehaze', 0),
                    settings.get('glow', 0), style_number.get(settings.get('glowStyle', 'Diffusion'), 0),
                    settings.get('glowRange', 0), settings.get('glowSpread', 0), settings.get('glowWarmth', 0),
                    settings.get('vignetteAmount', 0), settings.get('vignetteMidpoint', 50),
                    settings.get('vignetteRoundness', 0), settings.get('vignetteFeather', 50),
                    settings.get('vignetteHighlights', 0),
                    vignette_number.get(settings.get('vignetteStyle', 'Highlight Priority'), 0), 1.0)
        optic = settings.get('optics')
        if optic:
            distortion_k = optic['distortion'] / 100 * .35
            if optic['enableLensProfile']:
                distortion_k += optic['profileDistortion'] / 100 * .35
            optics(data, width, height, width * 4,
                   int(optic['removeChromaticAberration']), int(optic['enableLensProfile']),
                   optic['profileDistortion'], optic['profileVignetting'], distortion_k,
                   *(optic[k] for k in ('purpleAmount', 'purpleHueLow', 'purpleHueHigh',
                                        'greenAmount', 'greenHueLow', 'greenHueHigh',
                                        'vignetteAmount', 'vignetteMidpoint')), 1.0)
        detailed = settings.get('detail')
        if detailed:
            detail_defaults = dict(sharpenAmount=0, sharpenRadius=10, sharpenDetail=25,
                                   sharpenMasking=0, noiseLuminance=0,
                                   noiseLuminanceDetail=50, noiseLuminanceContrast=0,
                                   noiseColor=0, noiseColorDetail=50,
                                   noiseColorSmoothness=50)
            detail_defaults.update(detailed)
            detail(data, width, height, width * 4,
                   *(detail_defaults[k] for k in ('sharpenAmount', 'sharpenRadius', 'sharpenDetail',
                                                  'sharpenMasking', 'noiseLuminance', 'noiseLuminanceDetail',
                                                  'noiseLuminanceContrast', 'noiseColor', 'noiseColorDetail',
                                                  'noiseColorSmoothness')), 1.0)
        generated.append(dict(name=name, settings=settings, expectedPremultiplied=list(data)))
    advanced_output = dict(width=width, height=height,
                           input=[v for p in advanced_pixels for v in p],
                           sourceSha256={s.name: hashlib.sha256(s.read_bytes()).hexdigest() for s in sources},
                           stages=['effects', 'optics', 'detail'], cases=generated)
    advanced_dest = root / 'rust/tests/fixtures/camera-advanced-reference.json'
    advanced_dest.write_text(json.dumps(advanced_output, indent=2) + '\n')
    print(advanced_dest)
