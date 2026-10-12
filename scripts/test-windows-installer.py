#!/usr/bin/env python3
"""Offline installer provenance and publication regressions; no Windows execution."""
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest import mock


def load(name):
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), Path(__file__).with_name(name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


installer = load('windows-installer')
fixtures = load('test-release-downloads')


class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.manifest = fixtures.candidate(self.root / 'archives')
        self.archive = self.root / 'archives/windows-x86_64' / installer.portable(self.manifest)['fileName']
        patch = mock.patch.object(installer, 'release', return_value=self.manifest)
        patch.start()
        self.addCleanup(patch.stop)
        self.value = {
            'schemaVersion': 1, 'version': fixtures.VERSION, 'sourceRevision': fixtures.SOURCE,
            'portableSha256': installer.portable(self.manifest)['sha256'],
            'binarySha256': installer.portable(self.manifest)['binarySha256'],
            'packagingRevision': fixtures.MAIN, 'buildRun': 123, 'buildAttempt': 1,
            'signed': False, 'qualification': 'automated-preview', 'upgradeFrom': '0.7.0',
            'checks': sorted(installer.CHECKS), 'smokeSha256': 'c' * 64,
            'compiler': {'version': '6.6.1', 'sha256': 'd' * 64},
            'installer': {'fileName': installer.setup_name(fixtures.VERSION), 'sha256': 'e' * 64, 'size': 1024},
            'limitations': installer.LIMITATIONS,
        }

    def test_stage_preserves_every_reviewed_payload_byte(self):
        out = self.root / 'stage'
        installer.stage(fixtures.VERSION, out, self.archive)
        original = fixtures.payload('windows-x86_64')
        for name, content in original.items():
            self.assertEqual((out / 'payload' / name).read_bytes(), content)
        self.assertEqual(json.loads((out / 'payload.json').read_text())['sourceRevision'], fixtures.SOURCE)

    def test_stage_refuses_corrupted_zip_before_extracting(self):
        data = self.archive.read_bytes()
        self.archive.write_bytes(data[:-1] + bytes([data[-1] ^ 1]))
        with self.assertRaisesRegex(ValueError, 'hash/size'):
            installer.stage(fixtures.VERSION, self.root / 'stage', self.archive)
        self.assertFalse((self.root / 'stage/payload').exists())

    def test_stage_does_not_overwrite_existing_directory(self):
        out = self.root / 'stage'
        out.mkdir()
        (out / 'keep.txt').write_text('keep')
        with self.assertRaisesRegex(ValueError, 'new staging'):
            installer.stage(fixtures.VERSION, out, self.archive)
        self.assertEqual((out / 'keep.txt').read_text(), 'keep')

    def test_versions_cannot_inject_paths_or_compiler_defines(self):
        for value in ('../0.8.0', '0.8.0/Dfoo=bar', '0.8.0-rc1', '01.8.0', '0.8.0\n', '0.8'):
            with self.subTest(value=value), self.assertRaises(ValueError):
                installer.version(value)

    def test_declaration_binds_payload_source_and_qualification(self):
        installer.validate(self.value, self.manifest)
        for key, bad in [('portableSha256', 'f' * 64), ('binarySha256', 'f' * 64),
                         ('sourceRevision', 'f' * 40), ('version', '0.9.0'),
                         ('checks', []), ('signed', True), ('buildAttempt', True),
                         ('upgradeFrom', fixtures.VERSION), ('packagingRevision', 'main'),
                         ('limitations', [])]:
            with self.subTest(key=key), self.assertRaises(ValueError):
                installer.validate(dict(self.value, **{key: bad}), self.manifest)

    def test_installer_names_and_hashes_cannot_be_substituted(self):
        for key, bad in [('fileName', 'another.exe'), ('size', 0), ('sha256', 'not-a-hash')]:
            value = copy.deepcopy(self.value)
            value['installer'][key] = bad
            with self.subTest(key=key), self.assertRaises(ValueError):
                installer.validate(value, self.manifest)

    def test_checksums_cover_exact_installer_and_receipt(self):
        assets = installer.expected_assets(self.value)
        exe = installer.setup_name(fixtures.VERSION)
        receipt = installer.receipt_name(fixtures.VERSION)
        self.assertEqual(set(assets), {exe, receipt, exe + '.sha256'})
        self.assertEqual(assets[receipt]['content'], installer.downloads.canonical(self.value))
        self.assertIn(f'{assets[exe]["sha256"]}  {exe}\n'.encode(), assets[exe + '.sha256']['content'])
        self.assertIn(f'{assets[receipt]["sha256"]}  {receipt}\n'.encode(), assets[exe + '.sha256']['content'])

    def test_attestation_rejects_different_tested_executable(self):
        directory = self.root / 'candidate'
        directory.mkdir()
        (directory / installer.setup_name(fixtures.VERSION)).write_bytes(b'executable')
        smoke = directory / 'smoke.json'
        smoke.write_text(json.dumps({'status': 'passed', 'checks': sorted(installer.CHECKS), 'installerSha256': 'f' * 64}))
        with self.assertRaisesRegex(ValueError, 'different installer'):
            installer.attest(directory, smoke, fixtures.VERSION)

    def test_publication_refuses_non_publication_context_before_network(self):
        with mock.patch.object(installer, 'read_declaration', return_value=self.value), mock.patch.dict(os.environ, {}, clear=True), mock.patch.object(installer.notes, 'GitHub') as api:
            with self.assertRaisesRegex(ValueError, 'repository workflow'):
                installer.publish(fixtures.VERSION)
            api.assert_not_called()

    def publication_fixture(self):
        value = copy.deepcopy(self.value)
        executable = b'qualified installer fixture'
        smoke = installer.downloads.canonical({'status': 'passed', 'checks': sorted(installer.CHECKS)})
        value['smokeSha256'] = hashlib.sha256(smoke).hexdigest()
        value['installer'].update(size=len(executable), sha256=hashlib.sha256(executable).hexdigest())
        base = installer.downloads.expected_assets(self.manifest)
        expected = installer.expected_assets(value)
        record = {'id': 999, 'draft': False, 'prerelease': True, 'tag_name': self.manifest['tag'],
                  'target_commitish': fixtures.SOURCE, 'body': 'Keep original release notes.',
                  'assets': [dict(id=i, name=n, state='uploaded', size=a['size'], digest='sha256:' + a['sha256']) for i, (n, a) in enumerate(base.items(), 1)]}
        api = mock.Mock()
        api.request.side_effect = lambda path: {'object': {'sha': fixtures.MAIN}} if path.endswith('/git/ref/heads/main') else copy.deepcopy(record)
        calls = []

        def gh(command, env):
            calls.append(command)
            if command[:2] == ['run', 'download']:
                folder = Path(command[-1])
                for name, asset in expected.items():
                    (folder / name).write_bytes(asset.get('content', executable))
                (folder / 'smoke.json').write_bytes(smoke)
            else:
                self.assertEqual(command[:2], ['release', 'upload'])
                self.assertNotIn('--clobber', command)
                name = Path(command[3]).name
                asset = expected[name]
                record['assets'].append(dict(id=len(record['assets']) + 1, name=name, state='uploaded', size=asset['size'], digest='sha256:' + asset['sha256']))

        env = {'GITHUB_ACTIONS': 'true', 'GITHUB_REPOSITORY': installer.notes.REPOSITORY,
               'GH_TOKEN': 'test-fixture-not-a-credential', 'GITHUB_EVENT_NAME': 'workflow_dispatch',
               'GITHUB_REF': 'refs/heads/main', 'GITHUB_SHA': fixtures.MAIN, 'GITHUB_WORKFLOW_SHA': fixtures.MAIN,
               'GITHUB_WORKFLOW_REF': f'{installer.notes.REPOSITORY}/.github/workflows/publish-windows-installer.yml@refs/heads/main'}
        patches = [mock.patch.object(installer, 'read_declaration', return_value=value),
                   mock.patch.object(installer.notes, 'GitHub', return_value=api),
                   mock.patch.object(installer.notes, 'remote_file', return_value=installer.downloads.canonical(value).decode()),
                   mock.patch.object(installer.notes, 'tag_target', return_value=fixtures.SOURCE),
                   mock.patch.object(installer.downloads, 'check_run'),
                   mock.patch.object(installer.downloads, 'gh', side_effect=gh),
                   mock.patch.dict(os.environ, env, clear=True)]
        for patch in patches:
            patch.start()
            self.addCleanup(patch.stop)
        return record, calls, expected, value

    def test_publication_adds_three_assets_without_replacing_originals(self):
        record, calls, expected, value = self.publication_fixture()
        originals = copy.deepcopy(record['assets'])
        installer.publish(fixtures.VERSION)
        self.assertEqual(record['assets'][:len(originals)], originals)
        uploads = [c for c in calls if c[:2] == ['release', 'upload']]
        self.assertEqual(len(uploads), 3)
        self.assertEqual(Path(uploads[-1][3]).name, installer.receipt_name(fixtures.VERSION))
        installer.downloads.check_run.assert_called_once_with(mock.ANY, value['buildRun'], value['packagingRevision'], installer.BUILD_WORKFLOW,
            installer.BUILD_NAME, {'Test installer contracts', 'Build and test Windows installer'}, attempt=value['buildAttempt'])

    def test_publication_retry_preserves_already_uploaded_installer(self):
        record, calls, expected, value = self.publication_fixture()
        name = value['installer']['fileName']; asset = expected[name]
        record['assets'].append(dict(id=5, name=name, state='uploaded', size=asset['size'], digest='sha256:' + asset['sha256']))
        installer.publish(fixtures.VERSION)
        self.assertEqual(len([c for c in calls if c[:2] == ['release', 'upload']]), 2)
        self.assertEqual(record['assets'][4]['id'], 5)

    def test_publication_rejects_tampered_existing_asset_before_download(self):
        record, calls, expected, value = self.publication_fixture()
        record['assets'][0]['digest'] = 'sha256:' + 'f' * 64
        with self.assertRaisesRegex(ValueError, 'refusing replacement'):
            installer.publish(fixtures.VERSION)
        self.assertEqual(calls, [])

    def test_archive_publisher_accepts_only_declared_companion_hashes(self):
        record, calls, expected, value = self.publication_fixture()
        path = self.root / installer.declaration_path(fixtures.VERSION)
        path.parent.mkdir(parents=True)
        path.write_bytes(installer.downloads.canonical(value))
        supplemental = [dict(name=n, state='uploaded', size=a['size'], digest='sha256:' + a['sha256']) for n, a in expected.items()]
        with mock.patch.object(installer.downloads.bundle, 'ROOT', self.root):
            checked = installer.downloads.check_remote_assets(record['assets'] + supplemental, self.manifest, complete=True)
            self.assertEqual(checked, set(installer.downloads.expected_assets(self.manifest)))
            supplemental[0]['digest'] = 'sha256:' + 'f' * 64
            with self.assertRaises(ValueError):
                installer.downloads.check_remote_assets(record['assets'] + supplemental, self.manifest, complete=True)


if __name__ == '__main__':
    unittest.main()
