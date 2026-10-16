#!/usr/bin/env python3
"""Installer contracts. Kernel activation is a separate privileged host check."""
from pathlib import Path
import os
import subprocess
import tempfile
import types
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]
SCRIPT = ROOT / 'install.sh'


def load_helper():
    source = SCRIPT.read_text().split("<<'CTOX_APPARMOR_PY'\n", 1)[1].split('\nCTOX_APPARMOR_PY', 1)[0]
    module = types.ModuleType('ctox_installer_apparmor')
    exec(compile(source, str(SCRIPT) + ':CTOX_APPARMOR_PY', 'exec'), module.__dict__)
    return module


class InstallerContract(unittest.TestCase):
    def test_eligibility_requires_ubuntu_24_04_and_active_userns_restriction(self):
        helper = load_helper()
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            (root / 'etc').mkdir()
            sysctl = root / 'proc/sys/kernel/apparmor_restrict_unprivileged_userns'
            sysctl.parent.mkdir(parents=True)
            for identity, version, flag, expected in [
                ('ubuntu', '22.04', '1', False),
                ('ubuntu', '24.04', '0', False),
                ('debian', '26.04', '1', False),
                ('ubuntu', '24.04', '1', True),
                ('ubuntu', '26.04.1', '1', True),
                ('ubuntu', 'noble', '1', False),
            ]:
                (root / 'etc/os-release').write_text(f'ID={identity}\nVERSION_ID="{version}"\n')
                sysctl.write_text(flag + '\n')
                self.assertEqual(helper.restricted_ubuntu(root), expected,
                                 (identity, version, flag))
            sysctl.unlink()
            self.assertFalse(helper.restricted_ubuntu(root))
        # Exercise the shipped entry point. A non-eligible real host must not
        # request privilege or create a system profile.
        if not helper.restricted_ubuntu():
            result = subprocess.run(['bash', str(SCRIPT), '--ensure-process-sandbox'],
                                    capture_output=True, text=True, timeout=15)
            self.assertEqual(result.returncode, 0, result.stderr)

    def test_profile_reinstall_and_failure_preserve_installed_policy(self):
        helper = load_helper()
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            binary = root / 'bwrap'
            binary.write_text('#!/bin/sh\nexit 0\n')
            binary.chmod(0o755)
            profile = root / 'ctox-e20-bwrap'
            loaded = root / 'profiles'
            loaded.write_text('')
            calls = []
            fail_load = False

            def kernel_port(args):
                nonlocal fail_load
                calls.append(args)
                if '--replace' in args and fail_load:
                    raise subprocess.CalledProcessError(1, args)

            # Fake only the privileged ownership/kernel port, not eligibility,
            # profile rendering, file replacement or installer transaction.
            fstat = os.fstat
            root_stat = lambda fd: types.SimpleNamespace(st_uid=0, st_mode=fstat(fd).st_mode)
            with patch.object(helper, 'trusted', side_effect=lambda p: p.resolve(strict=True)), \
                 patch.object(helper.os, 'fstat', side_effect=root_stat), \
                 patch.object(helper, 'command', side_effect=kernel_port):
                self.assertEqual(helper.install_profile(binary, profile, Path('/bin/true'), loaded), 'installed')
                first = profile.stat()
                original = profile.read_bytes()
                self.assertEqual(helper.install_profile(binary, profile, Path('/bin/true'), loaded), 'reloaded')
                self.assertEqual(profile.read_bytes(), original)
                self.assertEqual(profile.stat().st_ino, first.st_ino)
                self.assertEqual(profile.stat().st_mtime_ns, first.st_mtime_ns)
                self.assertEqual(sum('--replace' in a for a in calls), 2)
                # Real byte replacement is rolled back on a rejected reload.
                profile.write_bytes(original + b'# prior local revision\n')
                previous = profile.read_bytes()
                fail_load = True
                with self.assertRaises(subprocess.CalledProcessError):
                    helper.install_profile(binary, profile, Path('/bin/true'), loaded)
                self.assertEqual(profile.read_bytes(), previous)
                profile.unlink()
                with self.assertRaises(subprocess.CalledProcessError):
                    helper.install_profile(binary, profile, Path('/bin/true'), loaded)
                self.assertFalse(profile.exists())
                self.assertFalse(list(root.glob('.ctox-e20-check-*')))

    def test_foreign_profiles_and_untrusted_paths_are_not_modified(self):
        helper = load_helper()
        with tempfile.TemporaryDirectory() as name:
            root = Path(name)
            binary = root / 'bwrap'
            binary.write_text('binary')
            binary.chmod(0o755)
            with self.assertRaises(RuntimeError):
                helper.trusted(binary)
            profile = root / 'ctox-e20-bwrap'
            loaded = root / 'profiles'
            loaded.write_text('')
            fstat = os.fstat
            root_stat = lambda fd: types.SimpleNamespace(st_uid=0, st_mode=fstat(fd).st_mode)
            with patch.object(helper, 'trusted', side_effect=lambda p: p.resolve(strict=True)), \
                 patch.object(helper.os, 'fstat', side_effect=root_stat), \
                 patch.object(helper, 'command') as command:
                profile.write_text('# operator managed\n')
                with self.assertRaises(RuntimeError):
                    helper.install_profile(binary, profile, Path('/bin/true'), loaded)
                self.assertEqual(profile.read_text(), '# operator managed\n')
                command.assert_not_called()
                profile.unlink()
                loaded.write_text('bwrap (enforce)\n')
                self.assertEqual(helper.install_profile(binary, profile, Path('/bin/true'), loaded), 'distribution')
                self.assertFalse(profile.exists())
                command.assert_not_called()
                for name in (helper.PROFILE, 'ctox-e20-payload'):
                    loaded.write_text(name + ' (enforce)\n')
                    with self.assertRaises(RuntimeError):
                        helper.install_profile(binary, profile, Path('/bin/true'), loaded)
                    self.assertFalse(profile.exists())
                    command.assert_not_called()
                loaded.write_text('')
                profile.symlink_to(binary)
                with self.assertRaises(RuntimeError):
                    helper.install_profile(binary, profile, Path('/bin/true'), loaded)
                self.assertEqual(binary.read_text(), 'binary')
                command.assert_not_called()


if __name__ == '__main__':
    unittest.main(verbosity=2)
