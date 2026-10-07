"""Exercise the canonical workflow's actual identity/source/immutability scripts.

Run on the build lane with Node.js and PyYAML (lane provisioning dependency).
No signer secret or GitHub writes are used.
"""
import os
import pathlib
import subprocess
import tempfile
import unittest
import yaml

ROOT = pathlib.Path(__file__).resolve().parents[1]
WORKFLOW = yaml.load((ROOT / '.github/workflows/business-os-shell-release.yml').read_text(), Loader=yaml.BaseLoader)
STEPS = {step['name']: step for step in WORKFLOW['jobs']['publish-shell']['steps']}


def execute(script, cwd, extra):
    env = dict(os.environ, **extra)
    return subprocess.run(['bash', '-e', '-c', script], cwd=cwd, env=env, text=True, capture_output=True, timeout=20)


class ShellReleaseWorkflow(unittest.TestCase):
    def identity(self, event, ref, version):
        with tempfile.TemporaryDirectory() as temp:
            output = pathlib.Path(temp) / 'output'
            result = execute(STEPS['Validate release identity']['run'], ROOT, {
                'EVENT_NAME': event, 'REF_NAME': ref, 'INPUT_VERSION': version, 'GITHUB_OUTPUT': str(output),
            })
            values = dict(line.split('=', 1) for line in output.read_text().splitlines()) if output.exists() else {}
            return result, values

    def test_dispatch_signing_is_reachable(self):
        for name in ['Build SPDX SBOM', 'Sign release manifest and channel pointer',
                     'Publish GitHub release assets', 'Publish signed channel pointer']:
            with self.subTest(step=name):
                self.assertNotIn('if', STEPS[name], 'manual releases must reach the canonical signer and publisher')
        result, values = self.identity('workflow_dispatch', 'main', '0.1.46-beta.76')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(values, {'version': '0.1.46-beta.76', 'release_tag': 'business-os-shell-v0.1.46-beta.76', 'channel': 'beta'})

    def test_existing_tag_release_identity(self):
        result, values = self.identity('push', 'business-os-shell-v0.1.46-beta.76', '')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(values['release_tag'], 'business-os-shell-v0.1.46-beta.76')

    def test_stable_and_nightly_channels(self):
        for version, channel in [('1.2.3', 'stable'), ('1.2.3-nightly.4', 'nightly')]:
            with self.subTest(version=version):
                result, values = self.identity('workflow_dispatch', 'main', version)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(values['channel'], channel)

    def test_invalid_release_identity_is_refused(self):
        for event, ref, version in [('workflow_dispatch', 'main', 'main'),
                                    ('workflow_dispatch', 'main', '1.2.3\nunsafe'),
                                    ('push', 'wrong-prefix-v1.2.3', '')]:
            with self.subTest(ref=ref, version=version):
                result, _ = self.identity(event, ref, version)
                self.assertNotEqual(result.returncode, 0)

    def test_source_guard_uses_actual_main_ancestry(self):
        with tempfile.TemporaryDirectory() as temp:
            repo = pathlib.Path(temp)
            def git(*args):
                return subprocess.check_output(['git', *args], cwd=repo, text=True, stderr=subprocess.DEVNULL).strip()
            git('init', '-q')
            git('config', 'user.name', 'Workflow fixture')
            git('config', 'user.email', 'fixture@example.invalid')
            (repo / 'fixture').write_text('main')
            git('add', 'fixture')
            git('commit', '-qm', 'main fixture')
            main = git('rev-parse', 'HEAD')
            git('update-ref', 'refs/remotes/origin/main', main)
            output = repo / 'output'
            script = STEPS['Verify main release source']['run']
            for source in ['', main]:
                result = execute(script, repo, {'INPUT_SOURCE': source, 'GITHUB_OUTPUT': str(output)})
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn('commit=' + main, output.read_text())
            result = execute(script, repo, {'INPUT_SOURCE': 'main', 'GITHUB_OUTPUT': str(output)})
            self.assertNotEqual(result.returncode, 0)
            (repo / 'fixture').write_text('unmerged')
            git('add', 'fixture')
            git('commit', '-qm', 'unmerged fixture')
            feature = git('rev-parse', 'HEAD')
            for source in [main, feature]:
                result = execute(script, repo, {'INPUT_SOURCE': source, 'GITHUB_OUTPUT': str(output)})
                self.assertNotEqual(result.returncode, 0, 'mismatch and unmerged commits must be refused')

    def test_existing_release_and_catalogue_errors_fail_closed(self):
        with tempfile.TemporaryDirectory() as temp:
            fixture = pathlib.Path(temp)
            gh = fixture / 'gh'
            for status, error, accepted in [(0, '', False), (1, 'gh: HTTP 404', True), (1, 'gh: HTTP 403', False)]:
                with self.subTest(status=status, error=error):
                    gh.write_text('#!/bin/sh\nprintf "%s\\n" ' + repr(error) + ' >&2\nexit ' + str(status) + '\n')
                    gh.chmod(0o700)
                    result = execute(STEPS['Guard immutable release version']['run'], fixture, {
                        'PATH': str(fixture) + os.pathsep + os.environ['PATH'],
                        'GITHUB_REPOSITORY': 'fixture/repo', 'RELEASE_TAG': 'business-os-shell-v1.2.3',
                    })
                    self.assertEqual(result.returncode == 0, accepted, result.stderr)

    def test_artifact_and_publication_bind_to_the_selected_commit(self):
        commit = '${{ steps.source.outputs.commit }}'
        tag = '${{ steps.identity.outputs.release_tag }}'
        self.assertIn(commit, STEPS['Build shell artifact']['run'])
        self.assertIn(commit, STEPS['Sign release manifest and channel pointer']['run'])
        self.assertIn(tag, STEPS['Build SPDX SBOM']['run'])
        self.assertIn(tag, STEPS['Sign release manifest and channel pointer']['run'])
        self.assertEqual(STEPS['Publish GitHub release assets']['with']['target_commitish'], commit)
        self.assertEqual(STEPS['Publish GitHub release assets']['with']['tag_name'], tag)
        self.assertEqual(WORKFLOW['concurrency']['cancel-in-progress'], 'false')


if __name__ == '__main__':
    unittest.main(verbosity=2)
