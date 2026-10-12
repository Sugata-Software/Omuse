#!/usr/bin/env python3
"""Run only on a disposable Windows CI account: real installer lifecycle checks."""
from __future__ import annotations

import argparse
import ctypes
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys

SPEC = importlib.util.spec_from_file_location('installer', Path(__file__).with_name('windows-installer.py'))
installer = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(installer)
require, digest = installer.require, installer.bundle.digest


def main():
    require(sys.platform == 'win32' and os.environ.get('CI') == 'true', 'Use a disposable Windows CI account')
    import winreg
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate', type=Path, required=True)
    parser.add_argument('--baseline', type=Path, required=True)
    parser.add_argument('--version', required=True)
    parser.add_argument('--previous-version', required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    candidate = args.candidate.resolve()
    baseline = args.baseline.resolve()
    evidence = args.output.resolve()
    require(not evidence.exists(), 'Evidence directory must be new')
    evidence.mkdir(parents=True)
    root = evidence / 'Installed Omuse - café'
    key = 'Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\' + installer.APP_ID + '_is1'

    def registry():
        try:
            with winreg.OpenKey(winreg.HKEY_CURRENT_USER, key, 0, winreg.KEY_READ | winreg.KEY_WOW64_64KEY) as opened:
                return {name: winreg.QueryValueEx(opened, name)[0] for name in ('DisplayVersion', 'InstallLocation', 'UninstallString')}
        except FileNotFoundError:
            return None

    require(registry() is None, 'Refusing to touch an existing Omuse installation')
    # Ask Windows for the same per-user folders Inno uses; env overrides do not
    # redirect known folders, and are deliberately not used as a safety boundary.
    def shell_folder(csidl):
        buffer = ctypes.create_unicode_buffer(32768)
        require(ctypes.windll.shell32.SHGetFolderPathW(None, csidl, None, 0, buffer) == 0, 'Cannot resolve user shell folder')
        return Path(buffer.value)

    shortcuts = [shell_folder(0x0002) / 'Omuse' / 'Omuse.lnk', shell_folder(0x0010) / 'Omuse.lnk']
    require(all(not path.exists() for path in shortcuts), 'Refusing to replace an existing Omuse shortcut')
    sentinels = []
    for base in (shell_folder(0x001A) / 'omuse', shell_folder(0x001C) / 'omuse', evidence / 'User project.omuse'):
        base.mkdir(parents=True, exist_ok=True)
        sentinel = base / 'installer-lifecycle-sentinel.txt'
        with sentinel.open('x', encoding='utf-8') as stream:
            stream.write('Keep my settings, recovery and artwork.\n')
        sentinels.append((sentinel, digest(sentinel)))
    env = os.environ.copy()
    for variable in ('APPDATA', 'LOCALAPPDATA', 'XDG_DATA_HOME', 'XDG_CONFIG_HOME', 'XDG_CACHE_HOME', 'XDG_STATE_HOME'):
        directory = evidence / 'app-test-profile' / variable
        directory.mkdir(parents=True)
        env[variable] = str(directory)

    def run(command, label, expected=0, app=False):
        result = subprocess.run([str(x) for x in command], capture_output=True, timeout=300, env=env if app else None)
        (evidence / f'{label}.stdout.log').write_bytes(result.stdout + result.stderr)
        require((result.returncode == expected) if expected is not None else result.returncode != 0, f'{label} failed: exit {result.returncode}')
        return result.stdout.decode('utf-8', errors='replace').strip()

    def setup(path, label, expected=0):
        run([path, '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/NOCLOSEAPPLICATIONS',
             f'/DIR={root}', '/TASKS=desktopicon', f'/LOG={evidence / (label + ".log")}'], label, expected)

    def inventory(directory):
        return {p.relative_to(directory).as_posix(): digest(p) for p in directory.rglob('*') if p.is_file()}

    def payload_matches(folder):
        expected = inventory(folder / 'payload')
        require(expected and all((root / name).is_file() and digest(root / name) == h for name, h in expected.items()), 'Installed payload differs from the reviewed archive')

    old_exe = baseline / installer.setup_name(args.previous_version)
    new_exe = candidate / installer.setup_name(args.version)
    setup(old_exe, 'fresh-install')
    payload_matches(baseline)
    require(registry()['DisplayVersion'] == args.previous_version, 'Baseline registration differs')
    root_sentinel = root / 'My artwork.omuse'
    root_sentinel.write_text('Unrelated artwork in application directory must also survive.\n', encoding='utf-8')
    sentinels.append((root_sentinel, digest(root_sentinel)))
    setup(new_exe, 'upgrade')
    payload_matches(candidate)
    registered = registry()
    require(registered['DisplayVersion'] == args.version and Path(registered['InstallLocation']) == root, 'Upgrade registration differs')
    require(str(root / 'unins000.exe') in registered['UninstallString'], 'Uninstall command points elsewhere')
    require(all(path.is_file() for path in shortcuts), 'Start menu/desktop shortcuts missing')
    for index, shortcut in enumerate(shortcuts):
        shortcut_env = dict(os.environ, OMUSE_TEST_LINK=str(shortcut))
        result = subprocess.run(['powershell', '-NoProfile', '-NonInteractive', '-Command',
                                 '[Console]::OutputEncoding=[Text.UTF8Encoding]::new(); $s=(New-Object -ComObject WScript.Shell).CreateShortcut($env:OMUSE_TEST_LINK); $s.TargetPath; $s.WorkingDirectory'],
                                env=shortcut_env, capture_output=True, text=True, encoding='utf-8', timeout=30)
        require(result.returncode == 0 and result.stdout.strip().splitlines() == [str(root / 'omuse.exe'), str(root)], 'Shortcut target/workdir differs')
    (root / 'models/u2netp.onnx').unlink()
    setup(new_exe, 'repair')
    payload_matches(candidate)
    before_refusal = inventory(root)
    setup(old_exe, 'downgrade-refused', expected=None)
    require(inventory(root) == before_refusal and registry()['DisplayVersion'] == args.version, 'Refused downgrade altered installation')
    require(run([root / 'omuse.exe', '--version'], 'version', app=True) == f'Omuse {args.version}', 'Installed version differs')
    run([root / 'omuse.exe', '--self-test', evidence / 'editing-journey'], 'editing', app=True)
    result = json.loads((evidence / 'editing-journey/results.json').read_text(encoding='utf-8'))
    require(result.get('status') == 'passed', 'Installed editing journey failed')
    run([sys.executable, '-c', 'import ctypes,sys; [ctypes.CDLL(p) for p in sys.argv[1:]]',
         root / 'lib/libraw.dll', root / 'lib/onnxruntime.dll'], 'runtime-load', app=True)
    run([root / 'unins000.exe', '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', f'/LOG={evidence / "uninstall.log"}'], 'uninstall')
    require(not (root / 'omuse.exe').exists() and registry() is None, 'Uninstaller left executable or registration')
    require(all(not path.exists() for path in shortcuts), 'Uninstaller left shortcuts')
    require(all(path.is_file() and digest(path) == h for path, h in sentinels), 'User data changed during installer lifecycle')
    result = {'schemaVersion': 1, 'status': 'passed', 'checks': sorted(installer.CHECKS),
              'installerSha256': digest(new_exe), 'binarySha256': digest(candidate / 'payload/omuse.exe'),
              'upgradeFrom': args.previous_version, 'platform': 'Windows Server 2025 x64',
              'limits': 'Automated native lifecycle on a disposable runner; no clean standard-user desktop, SmartScreen or interactive GUI claim.'}
    installer.write_json(candidate / 'smoke.json', result)
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
