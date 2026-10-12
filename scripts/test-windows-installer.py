#!/usr/bin/env python3
"""Offline installer provenance and publication regressions; no Windows execution."""
import copy
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


if __name__ == '__main__':
    unittest.main()
