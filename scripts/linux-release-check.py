#!/usr/bin/env python3
"""Run inside the KDE Swift SDK. Preserve every exit code and log, even on failure.

Usage: PATH=/usr/lib/sdk/swift6/bin:$PATH python3 scripts/linux-release-check.py
Requires the pinned Skia build in build/skia-src/out/Raster (see reliability docs).
This checks the local optimized candidate; it is not a Flatpak manifest rebuild.
"""
import datetime
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import xml.etree.ElementTree as ET

root = Path(__file__).resolve().parent.parent
out = root / '.release-evidence'
out.mkdir(exist_ok=True)
env = dict(os.environ, QT_QPA_PLATFORM='offscreen', QT_QPA_PLATFORMTHEME='', QT_STYLE_OVERRIDE='Fusion',
           COMPOSITOR_SKIA_BRIDGE=str(root / 'build/libCompositorSkiaBridge.so'),
           COMPOSITOR_IMAGEIO_BACKEND=str(root / 'build/libCompositorQtImageIO.so'))
# Isolate preferences, recent files, recovery and caches from real user data.
for key, directory in [('XDG_CONFIG_HOME', 'config'), ('XDG_DATA_HOME', 'data'), ('XDG_CACHE_HOME', 'cache')]:
    env[key] = str(out / directory)
    (out / directory).mkdir(exist_ok=True)
report = {'started_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
          'commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip(),
          'working_tree': subprocess.check_output(['git', 'status', '--porcelain'], cwd=root, text=True).splitlines(),
          'configuration': 'release', 'phases': [],
          'runtime_exclusions': ['BrushTests.bracketKeysReachTheBrushWhereverFocusIsExceptTextFields'],
          'exclusion_equivalent': 'Qt --ui-smoke: layers focus, text entry, brush size/hardness and non-brush tool',
          'limitations': ['Mac runtime unavailable', 'physical tablet unavailable', 'second display unavailable',
                          'cached raster Skia; fresh Flatpak manifest/package build is a separate gate']}

def run(name, command, extra=None):
    print(f'RUN {name}', flush=True)
    start = time.monotonic()
    with (out / f'{name}.log').open('w') as log:
        result = subprocess.run(command, cwd=root, env=env | (extra or {}), stdout=log, stderr=subprocess.STDOUT)
    report['phases'].append({'name': name, 'command': command, 'exit_code': result.returncode,
                             'seconds': round(time.monotonic() - start, 2)})
    (out / 'results.json').write_text(json.dumps(report, indent=2) + '\n')
    print(f'{"PASS" if result.returncode == 0 else "FAIL"} {name} ({result.returncode})', flush=True)
    return result.returncode == 0

configured = run('native-configure', ['cmake', '-S', '.', '-B', 'build', '-DCMAKE_BUILD_TYPE=Release',
                 '-DCOMPOSITOR_REQUIRE_VENDORED_DEPS=OFF', '-DCOMPOSITOR_SKIA_ROOT=' + str(root / 'build/skia-src'),
                 '-DCOMPOSITOR_SKIA_LIB=' + str(root / 'build/skia-src/out/Raster/libskia.a')])
if configured and run('native-build', ['cmake', '--build', 'build', '--parallel', '2']):
    run('native-tests', ['ctest', '--test-dir', 'build', '--output-on-failure', '--output-junit', str(out / 'native.xml')])
# Swift test builds all package products, including the optimized Qt host.
# Use that exact artifact; rebuilding without testability would replace the
# dependency modules and unnecessarily compile the entire package twice.
if run('swift-tests', ['swift', 'test', '-c', 'release', '--jobs', '2', '--no-parallel', '--skip', report['runtime_exclusions'][0]]):
    binary = root / '.build/release/CompositorHostBootstrap'
    report['executable_sha256'] = hashlib.sha256(binary.read_bytes()).hexdigest()
    for mode in ['session', 'io', 'layers', 'brush', 'dialog']:
        run(f'{mode}-journey', [str(binary), f'--{mode}-smoke'])
    for scale in ['1', '1.5', '2']:
        run('ui-scale-' + scale, [str(binary), '--ui-smoke'], {'QT_SCALE_FACTOR': scale,
            'COMPOSITOR_UI_EVIDENCE': str(out / ('ui-scale-' + scale))})
# A release gate must not turn green because a required backend was skipped.
native = ET.parse(out / 'native.xml').findall('.//testcase') if (out / 'native.xml').exists() else []
report['native_skips'] = [case.attrib.get('name', '') for case in native if case.find('skipped') is not None]
report['phases'].append({'name': 'native-coverage', 'exit_code': 0 if native and not report['native_skips'] else 1})
exports = [out / ('ui-scale-' + scale) / 'linux-ui-fixture.png' for scale in ['1', '1.5', '2']]
report['scale_export_hashes'] = {str(path.relative_to(out)): hashlib.sha256(path.read_bytes()).hexdigest()
                                 for path in exports if path.exists()}
report['phases'].append({'name': 'scale-export-equality', 'exit_code':
                        0 if len(report['scale_export_hashes']) == 3 and len(set(report['scale_export_hashes'].values())) == 1 else 1})
report['completed_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
report['passed'] = all(phase['exit_code'] == 0 for phase in report['phases'])
(out / 'results.json').write_text(json.dumps(report, indent=2) + '\n')
sys.exit(0 if report['passed'] else 1)
