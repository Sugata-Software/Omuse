#!/usr/bin/env python3
"""Stage, attest and publish an additive Windows installer for a reviewed ZIP.

No runtime rebuild, tag changes, replacement assets or release-note edits.
Native installer tests and an explicit, reviewed declaration precede publication.
"""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import urllib.request


def sibling(name):
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), Path(__file__).with_name(name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


downloads = sibling('release-downloads')
bundle, notes = downloads.bundle, downloads.notes
ROOT = Path(__file__).resolve().parent.parent
APP_ID = '{A6FD8840-423A-4C49-BDFC-EE180DCE143B}'
BUILD_WORKFLOW = '.github/workflows/windows-installer.yml'
BUILD_NAME = 'Omuse Windows installer'
CHECKS = {'fresh-install', 'installed-payload', 'start-menu-shortcut', 'desktop-shortcut',
          'uninstall-registration', 'previous-release-upgrade', 'repair', 'downgrade-refusal',
          'native-version', 'editing-journey', 'runtime-library-load', 'uninstall',
          'shortcuts-removed', 'profile-and-project-preservation'}
LIMITATIONS = [
    'Unsigned experimental Windows x64 installer; SmartScreen may warn.',
    'Native automated checks run on Windows Server 2025; interactive Windows 10/11 and standard-user clean-machine acceptance remain separate.',
    'The Visual C++ x64 Redistributable, FFmpeg and AI command-line tools are optional external prerequisites, not installed by this package.',
]


def require(condition, message):
    if not condition:
        raise ValueError(message)


def version(value):
    require(isinstance(value, str) and bundle.VERSION.fullmatch(value), 'Invalid version')
    return value


def release(v):
    return downloads.read_manifest(ROOT / downloads.manifest_path(version(v)))


def portable(manifest):
    return next(a for a in manifest['assets'] if a['target'] == 'windows-x86_64')


def setup_name(v):
    return f'omuse-{version(v)}-windows-x86_64-setup.exe'


def receipt_name(v):
    return f'omuse-{version(v)}-windows-installer.json'


def declaration_path(v):
    return f'docs/releases/installers/v{version(v)}.json'


def write_json(path, value):
    with path.open('xb') as stream:
        stream.write(downloads.canonical(value))


def stage(v, output, archive=None):
    manifest = release(v)
    expected = portable(manifest)
    require(not output.exists(), 'Use a new staging directory')
    output.mkdir(parents=True)
    if archive is None:
        archive = output / expected['fileName']
        url = f'https://github.com/{notes.REPOSITORY}/releases/download/{manifest["tag"]}/{expected["fileName"]}'
        request = urllib.request.Request(url, headers={'User-Agent': 'Omuse-Windows-Installer'})
        with urllib.request.urlopen(request, timeout=120) as response, archive.open('xb') as target:
            require(response.url.startswith('https://'), 'Insecure download redirect')
            size = 0
            while chunk := response.read(1024 * 1024):
                size += len(chunk)
                require(size <= expected['size'], 'Download exceeds reviewed size')
                target.write(chunk)
    require(archive.is_file() and not archive.is_symlink(), 'Archive must be a regular file')
    require(archive.stat().st_size == expected['size'] and bundle.digest(archive) == expected['sha256'], 'Portable release differs from its reviewed hash/size')
    actual = bundle.inspect_archive(archive, 'windows-x86_64', manifest['sourceRevision'], v)
    for key in ('binarySha256', 'inventorySha256', 'dependencyCount'):
        require(actual[key] == expected[key], f'Portable release differs: {key}')
    bundle.extract_verified(archive, 'windows-x86_64', manifest['sourceRevision'], output / 'payload')
    write_json(output / 'payload.json', actual)


def validate(value, manifest):
    fields = {'schemaVersion', 'version', 'sourceRevision', 'portableSha256', 'binarySha256',
              'packagingRevision', 'buildRun', 'buildAttempt', 'signed', 'qualification',
              'upgradeFrom', 'checks', 'smokeSha256', 'compiler', 'installer', 'limitations'}
    require(isinstance(value, dict) and set(value) == fields, 'Invalid installer declaration fields')
    require(type(value['schemaVersion']) is int and value['schemaVersion'] == 1, 'Invalid installer schema')
    require(value['version'] == manifest['version'] and value['sourceRevision'] == manifest['sourceRevision'], 'Installer source/version differs')
    expected = portable(manifest)
    require(value['portableSha256'] == expected['sha256'] and value['binarySha256'] == expected['binarySha256'], 'Installer payload differs from the reviewed ZIP')
    require(isinstance(value['packagingRevision'], str) and bundle.SOURCE.fullmatch(value['packagingRevision']), 'Invalid packaging revision')
    require(all(downloads.positive(value[k]) for k in ('buildRun', 'buildAttempt')), 'Invalid build identity')
    require(value['signed'] is False and value['qualification'] == 'automated-preview', 'Unqualified signing/stability claim')
    require(value['checks'] == sorted(CHECKS) and value['limitations'] == LIMITATIONS, 'Incomplete installer qualification')
    require(tuple(map(int, version(value['upgradeFrom']).split('.'))) < tuple(map(int, version(value['version']).split('.'))), 'Upgrade baseline must be an older release')
    compiler = value['compiler']
    require(isinstance(compiler, dict) and set(compiler) == {'version', 'sha256'} and isinstance(compiler['version'], str) and compiler['version'].startswith('6.'), 'Invalid compiler record')
    installer = value['installer']
    require(isinstance(installer, dict) and set(installer) == {'fileName', 'size', 'sha256'}, 'Invalid installer asset')
    require(installer['fileName'] == setup_name(value['version']) and downloads.positive(installer['size']) and installer['size'] < bundle.MAX_ARCHIVE, 'Invalid installer filename/size')
    require(all(isinstance(h, str) and bundle.SHA256.fullmatch(h) for h in (value['smokeSha256'], compiler['sha256'], installer['sha256'])), 'Invalid evidence hash')
    return value


def read_declaration(path, manifest):
    require(path.is_file() and not path.is_symlink() and path.stat().st_size < 32768, 'Missing or oversized installer declaration')
    raw = path.read_bytes()
    value = validate(bundle.parse_json(raw), manifest)
    require(raw == downloads.canonical(value), 'Installer declaration must retain canonical bytes')
    return value


def expected_assets(value):
    installer = value['installer']
    raw = downloads.canonical(value)
    result = {installer['fileName']: {k: installer[k] for k in ('size', 'sha256')}}
    result[receipt_name(value['version'])] = {'content': raw, 'size': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()}
    sums = ''.join(f'{a["sha256"]}  {name}\n' for name, a in sorted(result.items())).encode()
    result[installer['fileName'] + '.sha256'] = {'content': sums, 'size': len(sums), 'sha256': hashlib.sha256(sums).hexdigest()}
    return result


def attest(directory, smoke, v):
    manifest = release(v)
    result = bundle.parse_json(smoke.read_bytes())
    exe = directory / setup_name(v)
    require(result.get('status') == 'passed' and result.get('checks') == sorted(CHECKS), 'Native installer lifecycle did not pass')
    require(result.get('installerSha256') == bundle.digest(exe) and result.get('binarySha256') == portable(manifest)['binarySha256'], 'Smoke tested a different installer/payload')
    value = {
        'schemaVersion': 1, 'version': v, 'sourceRevision': manifest['sourceRevision'],
        'portableSha256': portable(manifest)['sha256'], 'binarySha256': portable(manifest)['binarySha256'],
        'packagingRevision': os.environ.get('GITHUB_SHA'), 'buildRun': int(os.environ.get('GITHUB_RUN_ID', '0')),
        'buildAttempt': int(os.environ.get('GITHUB_RUN_ATTEMPT', '0')), 'signed': False, 'qualification': 'automated-preview',
        'upgradeFrom': result['upgradeFrom'], 'checks': result['checks'], 'smokeSha256': bundle.digest(smoke),
        'compiler': bundle.parse_json((directory / 'compiler.json').read_bytes()),
        'installer': {'fileName': exe.name, 'sha256': bundle.digest(exe), 'size': exe.stat().st_size}, 'limitations': LIMITATIONS,
    }
    validate(value, manifest)
    for name, record in expected_assets(value).items():
        if 'content' in record:
            (directory / name).write_bytes(record['content'])
    return value


def publish(v):
    manifest = release(v)
    path = declaration_path(v)
    value = read_declaration(ROOT / path, manifest)
    env = os.environ.copy()
    require(env.get('GITHUB_REPOSITORY') == notes.REPOSITORY and env.get('GH_TOKEN'), 'Publication requires this repository workflow')
    ref = env.get('GITHUB_REF')
    require((env.get('GITHUB_EVENT_NAME') == 'push' and ref == f'refs/heads/publish-windows-installer/v{v}') or
            (env.get('GITHUB_EVENT_NAME') == 'workflow_dispatch' and ref == 'refs/heads/main'), 'Publication requires an explicit installer publication request')
    require(env.get('GITHUB_ACTIONS') == 'true' and env.get('GITHUB_WORKFLOW_SHA') == env.get('GITHUB_SHA') and
            env.get('GITHUB_WORKFLOW_REF') == f'{notes.REPOSITORY}/.github/workflows/publish-windows-installer.yml@{ref}', 'Unexpected publication workflow')
    api = notes.GitHub()
    main = api.request(f'{notes.API_ROOT}/git/ref/heads/main')['object']['sha']
    require(env.get('GITHUB_SHA') == main, 'Publication request must point to reviewed main')
    require(notes.remote_file(api, path, main).encode() == downloads.canonical(value), 'Main declaration differs')
    downloads.check_run(api, value['buildRun'], value['packagingRevision'], BUILD_WORKFLOW, BUILD_NAME,
                        {'Test installer contracts', 'Build and test Windows installer'}, attempt=value['buildAttempt'])
    require(notes.tag_target(api, manifest['tag']) == manifest['sourceRevision'], 'Runtime tag changed')
    expected = expected_assets(value)
    base = downloads.expected_assets(manifest)
    release_record = api.request(f'{notes.API_ROOT}/releases/tags/{manifest["tag"]}')

    def inspect(record):
        require(record.get('draft') is False and record.get('prerelease') is True and record.get('tag_name') == manifest['tag'] and record.get('target_commitish') == manifest['sourceRevision'], 'Release identity changed')
        assets = record.get('assets', [])
        allowed = {**base, **expected}
        seen = {}
        for asset in assets:
            name = asset.get('name')
            require(name in allowed and name not in seen, 'Unexpected or duplicate release asset')
            require(asset.get('state') == 'uploaded' and asset.get('size') == allowed[name]['size'] and asset.get('digest') == 'sha256:' + allowed[name]['sha256'], 'Release asset changed; refusing replacement')
            seen[name] = asset['id']
        require(base.keys() <= seen.keys(), 'Original release downloads are incomplete')
        return seen

    original = inspect(release_record)
    with tempfile.TemporaryDirectory(prefix='omuse-installer-publish-') as temporary:
        directory = Path(temporary)
        downloads.gh(['run', 'download', str(value['buildRun']), '--repo', notes.REPOSITORY, '--name', f'omuse-windows-installer-attempt-{value["buildAttempt"]}', '--dir', str(directory)], env)
        for name, record in expected.items():
            asset = directory / name
            require(asset.is_file() and not asset.is_symlink() and asset.stat().st_size == record['size'] and bundle.digest(asset) == record['sha256'], 'Candidate asset differs from reviewed declaration')
        smoke = directory / 'smoke.json'
        require(smoke.is_file() and bundle.digest(smoke) == value['smokeSha256'], 'Native smoke receipt differs')
        for name in sorted(expected, key=lambda n: (n == receipt_name(v), n)):
            current = api.request(f'{notes.API_ROOT}/releases/{release_record["id"]}')
            seen = inspect(current)
            require(current.get('body') == release_record.get('body') and all(seen.get(n) == i for n, i in original.items()), 'Existing notes or assets changed')
            if name not in seen:
                downloads.gh(['release', 'upload', manifest['tag'], str(directory / name), '--repo', notes.REPOSITORY], env)
        final = api.request(f'{notes.API_ROOT}/releases/{release_record["id"]}')
        seen = inspect(final)
        require(expected.keys() <= seen.keys() and all(seen.get(n) == i for n, i in original.items()), 'Publication readback failed')
        require(final.get('body') == release_record.get('body') and notes.tag_target(api, manifest['tag']) == manifest['sourceRevision'], 'Release notes/tag changed')
    print(f'Published verified Windows installer for {v}')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=('stage', 'attest', 'check', 'publish'))
    parser.add_argument('--version', required=True, type=version)
    parser.add_argument('--output', type=Path)
    parser.add_argument('--archive', type=Path)
    parser.add_argument('--smoke', type=Path)
    args = parser.parse_args()
    if args.mode == 'stage':
        require(args.output is not None, 'Output is required')
        stage(args.version, args.output.resolve(), args.archive.resolve() if args.archive else None)
    elif args.mode == 'attest':
        require(args.output is not None and args.smoke is not None, 'Output and smoke are required')
        attest(args.output, args.smoke, args.version)
    elif args.mode == 'check':
        read_declaration(ROOT / declaration_path(args.version), release(args.version))
    else:
        publish(args.version)


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as error:
        sys.exit(f'Windows installer: {error}')
