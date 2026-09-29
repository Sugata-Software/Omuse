#!/usr/bin/env python3
"""Run the synthetic native GPUI journey and capture only its verified window."""
import argparse
import ctypes
import hashlib
import json
import math
import os
from pathlib import Path
import re
import shutil
import subprocess
import time

parser = argparse.ArgumentParser()
MAX_RESUME_HISTORY_BYTES = (256 + 96) * 1024 * 1024 + 512 * 1024
MAX_RESUME_HISTORY_FILES = 1 + 16 * (4 + 8)
parser.add_argument('executable', type=Path)
parser.add_argument('evidence', type=Path)
parser.add_argument('--desktop-env', type=Path)
parser.add_argument('--capture', action='store_true')
parser.add_argument('--capture-startup', action='store_true', help='Hold only this test launch to capture two splash frames')
parser.add_argument('--reduced-motion', action='store_true', help='Disable splash animation for accessibility verification')
parser.add_argument('--ai', action='store_true', help='Explicit live subscription check: one generation, one background edit and one editable carousel plan; consumes the chosen Codex allowance')
parser.add_argument('--ai-resume-history', type=Path, metavar='EVIDENCE_DIRECTORY', help='Resume one retained Generate review from an earlier isolated --ai evidence directory, then run only the remaining background and assistant requests')
parser.add_argument('--ai-image-intent', choices=('generate','replace','remove','background','expand'), help='Explicit live subscription check for exactly one image intent; keeps, saves, reopens, undoes and redoes the reviewed result')
parser.add_argument('--small-window', action='store_true')
parser.add_argument('--require-usable-startup', action='store_true')
parser.add_argument('--panel', choices=('crop','curves','mixer','geometry','text','layers','develop','selection','canvas','luminosity-range','color-range','filter-stack','blend-if','advanced-retouch','controlled-removal','editable-warp','refine-workspace','brush-studio','smart-source','colour-management','automation','multi-image','vector-path','vector-mask','create','templates','assistant','content-export','motion','commands','shortcuts'))
parser.add_argument('--theme', choices=('system','dark','light'), default='system')
parser.add_argument('--minimum-window', action='store_true', help='Run the interaction journey at 800x600')
parser.add_argument('--x11', action='store_true', help='Run only the owned app through the desktop XWayland connection')
args = parser.parse_args()
executable = args.executable.resolve(strict=True)
executable_sha256 = hashlib.sha256(executable.read_bytes()).hexdigest()
evidence = args.evidence.resolve()
evidence.mkdir(parents=True, exist_ok=True)
if args.ai_image_intent and (args.ai or args.ai_resume_history):
    raise SystemExit('--ai-image-intent cannot be combined with --ai or --ai-resume-history')
if args.ai_resume_history and not args.ai:
    raise SystemExit('--ai-resume-history requires --ai')
env = os.environ.copy()
if args.desktop_env:
    source = json.loads(args.desktop_env.read_text())
    for key in ('WAYLAND_DISPLAY', 'XDG_RUNTIME_DIR', 'DBUS_SESSION_BUS_ADDRESS', 'HYPRLAND_INSTANCE_SIGNATURE'):
        if key in source:
            env[key] = source[key]
for key, folder in [
    ('XDG_DATA_HOME', 'data'),
    ('XDG_CONFIG_HOME', 'config'),
    ('XDG_CACHE_HOME', 'cache'),
    ('XDG_STATE_HOME', 'state'),
]:
    env[key] = str(evidence / folder)
    Path(env[key]).mkdir(exist_ok=True)
env['OMUSE_NATIVE_WAIT'] = '1'
if args.ai or args.ai_image_intent:
    env['OMUSE_NATIVE_AI_TEST'] = '1'
else:
    env.pop('OMUSE_NATIVE_AI_TEST', None)
if args.ai_image_intent:
    env['OMUSE_NATIVE_AI_IMAGE_INTENT'] = args.ai_image_intent
else:
    env.pop('OMUSE_NATIVE_AI_IMAGE_INTENT', None)
if args.capture_startup:
    env['OMUSE_NATIVE_STARTUP_WAIT'] = '1'
else:
    env.pop('OMUSE_NATIVE_STARTUP_WAIT', None)
    env.pop('COMPOSITOR_NATIVE_STARTUP_WAIT', None)
if args.reduced_motion:
    env['OMUSE_REDUCED_MOTION'] = '1'
if args.panel:
    env['OMUSE_NATIVE_PANEL'] = args.panel
env['OMUSE_NATIVE_THEME'] = args.theme
report = evidence / 'native-results.json'
error = evidence / 'native-error.txt'
if report.exists() or error.exists() or any(evidence.glob('startup-*.json')) or (evidence/'continue-startup').exists():
    raise SystemExit('Use a fresh evidence directory so stale results cannot pass.')
if args.ai_resume_history:
    source = args.ai_resume_history.resolve()
    source_history = source / 'data' / 'omuse' / 'ai-history'
    target_history = Path(env['XDG_DATA_HOME']) / 'omuse' / 'ai-history'
    history_index = source_history / 'history.json'
    if not source_history.is_dir() or source_history.is_symlink() or not history_index.is_file():
        raise SystemExit('Resume source has no retained Omuse AI history')
    if target_history.exists():
        raise SystemExit('Resume target already contains AI history; use a fresh evidence directory')
    if any(path.is_symlink() for path in source_history.rglob('*')):
        raise SystemExit('Resume source contains a symbolic link')
    copied_bytes = 0
    copied_files = 0
    for path in source_history.rglob('*'):
        relative = path.relative_to(source_history)
        if path.is_dir():
            if relative.parts not in (('assets',), ('context',)):
                raise SystemExit('Resume source contains an unexpected directory')
            continue
        # The history lock coordinates live writers, not retained content.
        # Never transfer its inode or count it as a provider artifact.
        if relative.parts == ('.history.lock',):
            if not path.is_file() or path.stat().st_size != 0:
                raise SystemExit('Resume source contains an invalid history lock')
            continue
        if not path.is_file() or not (
            relative.parts == ('history.json',)
            or (len(relative.parts) == 2 and relative.parts[0] in ('assets', 'context'))
        ):
            raise SystemExit('Resume source contains an unexpected history file')
        copied_files += 1
        copied_bytes += path.stat().st_size
    if copied_files > MAX_RESUME_HISTORY_FILES or copied_bytes > MAX_RESUME_HISTORY_BYTES:
        raise SystemExit('Resume source exceeds Omuse bounded AI history limits')
    shutil.copytree(source_history, target_history, ignore=shutil.ignore_patterns('.history.lock'))
    env['OMUSE_NATIVE_AI_RESUME_GENERATE'] = '1'
else:
    env.pop('OMUSE_NATIVE_AI_RESUME_GENERATE', None)
with (evidence / 'native.log').open('w') as log:
    launch_env = env.copy()
    if args.x11:
        if not launch_env.get('DISPLAY'):
            raise SystemExit('The XWayland qualification needs the desktop DISPLAY connection')
        launch_env.pop('WAYLAND_DISPLAY', None)
    process = subprocess.Popen([str(executable), '--ui-smoke', str(evidence)], env=launch_env, stdout=log, stderr=log)
    try:
        deadline = time.monotonic() + (1100 if args.ai or args.ai_image_intent else 90)
        ready_deadline=time.monotonic()+30
        def hypr(*command):
            return subprocess.check_output(['hyprctl', *command], env=env, text=True)
        initial_signal = 'startup-painted.json' if args.capture_startup else 'window-ready'
        while not (evidence/initial_signal).exists():
            if error.exists():
                raise RuntimeError(error.read_text())
            if process.poll() is not None or time.monotonic()>ready_deadline:
                raise RuntimeError('Native app failed to initialize a window')
            time.sleep(0.1)
        # The app can finish its first paint before the compositor publishes
        # the mapped client, particularly through XWayland. Keep ownership
        # strict while allowing that asynchronous registration to complete.
        while True:
            owned=[c for c in json.loads(hypr('-j','clients')) if c.get('pid')==process.pid and c.get('class')=='omuse' and c.get('mapped')]
            if len(owned)==1:
                break
            if len(owned)>1 or process.poll() is not None or time.monotonic()>ready_deadline:
                raise RuntimeError('No unique native test window')
            time.sleep(0.1)
        if bool(owned[0].get('xwayland')) != args.x11:
            raise RuntimeError('The owned window did not use the requested Wayland/XWayland backend')
        (evidence/'launch-window.json').write_text(json.dumps({k:owned[0].get(k) for k in ('pid','class','size','at','fullscreen')},indent=2))
        if args.require_usable_startup and (owned[0]['size'][0] < 800 or owned[0]['size'][1] < 600):
            raise RuntimeError('App startup did not provide its minimum usable canvas window')
        address=owned[0]['address']
        def owned_active():
            current=json.loads(hypr('-j','activewindow'))
            if current.get('pid')!=process.pid or current.get('address')!=address or process.poll() is not None:
                raise RuntimeError('Owned native window lost focus or identity')
            return current
        def geometry_key(current):
            return tuple(current['at']), tuple(current['size']), current.get('floating'), current.get('fullscreen')
        def settled_owned_window():
            until=time.monotonic()+3
            previous=None
            matches=0
            while time.monotonic()<until:
                current=owned_active()
                key=geometry_key(current)
                matches=matches+1 if key==previous else 1
                if matches>=3:
                    # Hyprland can publish target geometry before its resize
                    # animation reaches the last pixel. Keep the capture gate
                    # held through a final compositor-settle interval.
                    time.sleep(0.75)
                    final=owned_active()
                    if geometry_key(final)==key:
                        return final
                    matches=0
                previous=key
                time.sleep(0.1)
            raise RuntimeError('Owned native window geometry did not settle')
        def x11_minimum(current):
            clients=subprocess.check_output(['xprop','-root','_NET_CLIENT_LIST'],env=launch_env,text=True)
            matches=[]
            for token in clients.partition('#')[2].split(','):
                xid=token.strip()
                if not re.fullmatch(r'0x[0-9a-fA-F]+',xid):
                    continue
                props=subprocess.check_output(['xprop','-id',xid,'_NET_WM_PID','WM_CLASS','WM_NORMAL_HINTS'],env=launch_env,text=True)
                if re.search(r'_NET_WM_PID\(CARDINAL\) = '+str(process.pid)+r'\b',props):
                    if 'WM_CLASS(STRING) = "omuse", "omuse"' not in props:
                        raise RuntimeError('Owned X11 window has unexpected class')
                    minimum=re.search(r'program specified minimum size: (\d+) by (\d+)',props)
                    if not minimum:
                        raise RuntimeError('Owned X11 window has no minimum-size hints')
                    matches.append((int(xid,16),[int(v) for v in minimum.groups()],props))
            if len(matches)!=1:
                raise RuntimeError('No unique X11 minimum-size target')
            xid, minimum, props=matches[0]
            if min(minimum)<=0 or max(minimum)>16384:
                raise RuntimeError('Invalid owned X11 minimum-size hints')
            (evidence/'normal-hints.txt').write_text(props)
            xlib=ctypes.CDLL('libX11.so.6')
            xlib.XOpenDisplay.argtypes=[ctypes.c_char_p]
            xlib.XOpenDisplay.restype=ctypes.c_void_p
            xlib.XCloseDisplay.argtypes=[ctypes.c_void_p]
            xlib.XGetGeometry.argtypes=[ctypes.c_void_p,ctypes.c_ulong,ctypes.POINTER(ctypes.c_ulong),ctypes.POINTER(ctypes.c_int),ctypes.POINTER(ctypes.c_int),ctypes.POINTER(ctypes.c_uint),ctypes.POINTER(ctypes.c_uint),ctypes.POINTER(ctypes.c_uint),ctypes.POINTER(ctypes.c_uint)]
            xlib.XGetGeometry.restype=ctypes.c_int
            def device_size():
                display=xlib.XOpenDisplay(launch_env['DISPLAY'].encode())
                if not display:
                    raise RuntimeError('Cannot inspect the owned X11 display')
                try:
                    root=ctypes.c_ulong()
                    x,y=ctypes.c_int(),ctypes.c_int()
                    width,height,border,depth=(ctypes.c_uint() for _ in range(4))
                    if not xlib.XGetGeometry(display,xid,ctypes.byref(root),ctypes.byref(x),ctypes.byref(y),ctypes.byref(width),ctypes.byref(height),ctypes.byref(border),ctypes.byref(depth)):
                        raise RuntimeError('Cannot inspect the owned X11 device geometry')
                    return [width.value,height.value]
                finally:
                    xlib.XCloseDisplay(display)
            dimensions=device_size()
            ratio=[d/c for d,c in zip(dimensions,current['size'])]
            if any(not math.isfinite(r) or not 0.1<=r<=8 for r in ratio) or abs(ratio[0]-ratio[1])>0.02:
                raise RuntimeError('Inconsistent X11 device-to-compositor scale')
            monitors=json.loads(hypr('-j','monitors'))
            monitor=next(m for m in monitors if m['id']==current['monitor'])
            request=[math.ceil(d/r) for d,r in zip(minimum,ratio)]
            return request,device_size,{'xid':xid,'minimum_device':minimum,'monitor_scale':monitor['scale'],'device_per_compositor':ratio,'requested_compositor':request}
        hypr('dispatch',f'hl.dsp.focus({{ window = "address:{address}" }})')
        time.sleep(0.3)
        active=json.loads(hypr('-j','activewindow'))
        if active.get('pid')!=process.pid: raise RuntimeError('Native window focus failed')
        if not active.get('fullscreen'):
            hypr('dispatch','hl.dsp.window.fullscreen({ mode = "maximized" })')
        time.sleep(0.4)
        active=json.loads(hypr('-j','activewindow'))
        if active.get('pid')!=process.pid: raise RuntimeError('Native window lost focus')
        if args.minimum_window:
            requested_size=[800,600]
            x11_measure=None
            if args.x11:
                active=settled_owned_window()
                requested_size,device_size,x11_measure=x11_minimum(active)
            if active.get('fullscreen'):
                hypr('dispatch','hl.dsp.window.fullscreen({ mode = "maximized" })')
            active=json.loads(hypr('-j','activewindow'))
            if active.get('pid')!=process.pid: raise RuntimeError('Owned window lost focus before resize')
            if not active.get('floating'):
                hypr('dispatch','hl.dsp.window.float({ action = "toggle" })')
            hypr('dispatch',f'hl.dsp.window.resize({{ x = {requested_size[0]}, y = {requested_size[1]}, relative = false }})')
            active=settled_owned_window()
            if active.get('pid')!=process.pid or (not args.x11 and active.get('size')!=[800,600]):
                raise RuntimeError('Owned window did not reach the requested minimum size')
            if x11_measure is not None:
                x11_measure.update({'observed_compositor':active['size'],'observed_device':device_size()})
                (evidence/'x11-minimum-size.json').write_text(json.dumps(x11_measure,indent=2))
                if any(actual<minimum for actual,minimum in zip(x11_measure['observed_device'],x11_measure['minimum_device'])):
                    raise RuntimeError('Owned X11 device size is below its declared minimum')
        (evidence/'initial-window.json').write_text(json.dumps({k:active.get(k) for k in ('pid','class','size','at','fullscreen')},indent=2))
        startup_capture = None
        if args.capture_startup:
            capture_window=settled_owned_window()
            fixed_geometry=geometry_key(capture_window)
            x,y=capture_window['at']
            width,height=capture_window['size']
            if min(width,height)<=0:
                raise RuntimeError('Invalid splash window geometry')
            hashes = []
            for index in range(2):
                active=owned_active()
                if geometry_key(active)!=fixed_geometry:
                    raise RuntimeError('Owned window geometry changed during splash capture')
                shot = evidence / f'startup-{index + 1}.png'
                subprocess.run(['grim', '-g', f'{x},{y} {width}x{height}', str(shot)], env=env, check=True)
                hashes.append(hashlib.sha256(shot.read_bytes()).hexdigest())
                if index == 0:
                    time.sleep(0.45)
            painted = json.loads((evidence/'startup-painted.json').read_text())
            changed = hashes[0] != hashes[1]
            reduced = painted['reduced_motion']
            if changed == reduced:
                raise RuntimeError('Splash frames did not match the requested motion policy')
            startup_capture = {'sha256': hashes, 'frames_changed': changed, 'reduced_motion': reduced, 'stable_geometry':{'at':[x,y],'size':[width,height]}}
            (evidence/'continue-startup').write_text('continue')
            while not (evidence/'window-ready').exists():
                if error.exists():
                    raise RuntimeError(error.read_text())
                if process.poll() is not None or time.monotonic() > ready_deadline:
                    raise RuntimeError('Native app failed to hand off from splash to editor')
                time.sleep(0.1)
        (evidence/'start').write_text('start')
        peak_app_rss_kib = 0
        def sample_app_memory():
            try:
                values = (Path('/proc') / str(process.pid) / 'status').read_text().splitlines()
                return max((int(line.split()[1]) for line in values if line.startswith(('VmRSS:', 'VmHWM:'))), default=0)
            except (OSError, ValueError, IndexError):
                return 0
        while not report.exists():
            peak_app_rss_kib = max(peak_app_rss_kib, sample_app_memory())
            if error.exists():
                raise RuntimeError(error.read_text())
            if process.poll() is not None:
                raise RuntimeError(f'Native app exited {process.returncode} before producing evidence')
            if time.monotonic() > deadline:
                raise TimeoutError('Native journey timed out')
            time.sleep(0.2)
        result = json.loads(report.read_text())
        if result.get('status') != 'passed':
            raise RuntimeError(result)
        if args.ai_image_intent:
            if result.get('schema') != 'omuse.native-ai-image-qualification.v1' or result.get('intent') != args.ai_image_intent:
                raise RuntimeError('Native image evidence does not match the requested intent')
            if result.get('provider') != 'codexSubscription' or not result.get('runtime') or not result.get('resultID'):
                raise RuntimeError('Native image evidence omitted its exact provider result identity')
            if result.get('variation') != {'index': 1, 'total': 1} or result.get('automaticRetries') != 0:
                raise RuntimeError('Native image qualification was not a single no-retry variation')
            hashes = result.get('hashes', {})
            if not hashes.get('sourceCanvas') or not hashes.get('resultAsset'):
                raise RuntimeError('Native image evidence omitted required content hashes')
            if (args.ai_image_intent == 'generate') != (hashes.get('sourceMask') is None):
                raise RuntimeError('Native image evidence has an incorrect mask-hash contract')
            dimensions = result.get('dimensions', {})
            expected_canvas = [92, 92] if args.ai_image_intent == 'expand' else [64, 80]
            if dimensions.get('source') != [64, 80] or dimensions.get('keptCanvas') != expected_canvas or dimensions.get('reopenedCanvas') != expected_canvas:
                raise RuntimeError('Native image evidence has incorrect source, kept or reopened dimensions')
            provider_dimensions = dimensions.get('providerAsset')
            if not isinstance(provider_dimensions, list) or len(provider_dimensions) != 2 or any(not isinstance(value, int) or value <= 0 for value in provider_dimensions):
                raise RuntimeError('Native image evidence has invalid provider asset dimensions')
            if not isinstance(result.get('sourceIdentity'), dict) or not result['sourceIdentity'].get('documentId'):
                raise RuntimeError('Native image evidence omitted its source identity')
            expected_captures = ['before.png','candidate.png','kept.png','undo.png','redo.png','reopened.png']
            captures = result.get('renderCaptures')
            if not isinstance(captures, list) or [capture.get('file') for capture in captures] != expected_captures:
                raise RuntimeError('Native image evidence omitted a required render capture')
            for capture in captures:
                capture_path = evidence / capture['file']
                if not capture.get('contentHash') or not capture_path.is_file() or capture_path.stat().st_size == 0:
                    raise RuntimeError('Native image render capture is missing or unhashed')
            protected_intent = args.ai_image_intent in ('replace','remove','background')
            if result.get('protectedPixelsByteExact') != protected_intent or result.get('expandedOriginalTranslatedByteExact') != (args.ai_image_intent == 'expand'):
                raise RuntimeError('Native image pixel-integrity evidence does not match the requested intent')
            if result.get('nativeOriginalSourcePreserved') is not True or result.get('brandPreserved') is not True or result.get('keepSaveReopenUndoRedo') != 'passed':
                raise RuntimeError('Native image package or reversible-edit evidence is incomplete')
        if args.minimum_window:
            # XWayland and the compositor can use different scale factors.
            # Verify GPUI's actual content size, not just compositor geometry.
            bounds = (evidence/'window-bounds.txt').read_text()
            viewport = re.search(r'viewport=Size \{ ([0-9.]+)px × ([0-9.]+)px \}', bounds)
            if not viewport:
                raise RuntimeError('Native journey did not record its actual viewport size')
            dimensions = [float(value) for value in viewport.groups()]
            if dimensions[0] < 799.99 or dimensions[1] < 599.99:
                raise RuntimeError(f'GPUI viewport is below the 800x600 minimum: {dimensions}')
            result['minimum_window_viewport'] = dimensions
        result['executable_sha256'] = executable_sha256
        result['harness_sha256'] = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
        result['app_peak_rss_kib'] = max(peak_app_rss_kib, sample_app_memory())
        result['memory_scope'] = 'Omuse process only; excludes provider children and GPU allocations'
        if args.panel:
            result['inspection_panel'] = args.panel
        result['inspection_theme'] = args.theme
        result['minimum_window_journey'] = args.minimum_window
        result['desktop_backend'] = 'XWayland' if args.x11 else 'Wayland'
        stages = [json.loads((evidence/f'startup-{stage}.json').read_text())
                  for stage in ('painted', 'prepared', 'editor-ready', 'dismissed')]
        if any(stage['pid'] != process.pid for stage in stages):
            raise RuntimeError('Startup evidence came from another process')
        times = [stage['elapsed_ms'] for stage in stages]
        if times != sorted(times):
            raise RuntimeError('Startup did not paint, prepare and dismiss in order')
        result['startup'] = {'stages': stages, 'capture': startup_capture}
        if args.capture:
            def hypr(*command):
                return subprocess.check_output(['hyprctl', *command], env=env, text=True)
            windows = [c for c in json.loads(hypr('-j', 'clients')) if c.get('pid') == process.pid and c.get('class') == 'omuse' and c.get('mapped')]
            if len(windows) != 1:
                raise RuntimeError('Cannot identify exactly one owned synthetic app window')
            window = windows[0]
            address = window['address']
            hypr('dispatch', f'hl.dsp.focus({{ window = "address:{address}" }})')
            time.sleep(0.4)
            active = json.loads(hypr('-j', 'activewindow'))
            if active.get('pid') != process.pid or active.get('address') != address:
                raise RuntimeError('Owned app did not become active; refusing capture')
            if args.small_window:
                if active.get('fullscreen'):
                    hypr('dispatch','hl.dsp.window.fullscreen({ mode = "maximized" })')
                hypr('dispatch', 'hl.dsp.window.float({ action = "toggle" })')
                hypr('dispatch', 'hl.dsp.window.resize({ x = 1000, y = 840, relative = false })')
                hypr('dispatch', 'hl.dsp.window.move({ x = 20, y = 70, relative = false })')
                time.sleep(0.4)
                active = json.loads(hypr('-j', 'activewindow'))
                if active.get('pid') != process.pid or active.get('address') != address:
                    raise RuntimeError('Owned window lost focus during resize')
            x, y = active['at']
            width, height = active['size']
            if min(width, height) <= 0:
                raise RuntimeError('Invalid owned window geometry')
            subprocess.run(['grim', '-g', f'{x},{y} {width}x{height}', str(evidence / 'native-window.png')], env=env, check=True)
            result['window'] = {k: active.get(k) for k in ('pid', 'class', 'size')}
        (evidence / 'verified-native.json').write_text(json.dumps(result, indent=2) + '\n')
        print(json.dumps(result, indent=2))
    finally:
        # This process is solely the synthetic test; no existing editor is touched.
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
