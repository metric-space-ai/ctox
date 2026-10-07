#!/usr/bin/env python3
"""Install the shared artifact into a NEW synthetic acceptance prefix. No service restarts."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time

OWNER = '01a087a0-169a-7e23-ba3d-71352256cbfb'
SOURCE = '5fbba4af6ab4bc58135d71246608484ed8364035'
CONTRACTS = ['rxdb/manifest.json', 'rxdb/src/protocol-contract.generated.mjs',
             'rxdb/src/frame-contract.generated.mjs', 'shared/rxdb-runtime.js',
             'rxdb/dist/ctox-rxdb-js.mjs', 'index.html', 'app.js']


def sha(path):
    result = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1048576), b''):
            result.update(chunk)
    return result.hexdigest()


def prepare(producer_path, acceptance_base, port, host, workjet_revision):
    assert os.uname().sysname == 'Linux', 'Only isolated Linux acceptance hosts'
    assert os.environ.get('CARGO_TARGET_DIR') and os.environ.get('TMPDIR'), 'Run through gpu-build-run.sh'
    assert os.environ.get('CARGO_BUILD_JOBS') in ('1', '2'), 'At most two workers'
    producer_path = producer_path.resolve(strict=True)
    producer = json.loads(producer_path.read_text())
    assert producer['owner'] == OWNER and producer['source'] == SOURCE
    assert producer['native_package_ready'] and not producer.get('error')
    assert all(s['terminal'] and s['exit'] == 0 and s['cleanup'] == 'owned group absent'
               for s in producer['steps']), 'Exact shared producer must be terminal and verified'
    assert 19000 <= port <= 29999
    base_parent = acceptance_base.parent.resolve(strict=True)
    assert str(base_parent).startswith(('/mnt/nvme1/', '/home/metricspace/'))
    assert not any(x in str(acceptance_base).lower() for x in ('thesen', 'welsch'))
    assert shutil.disk_usage(base_parent).free >= 30 * 2**30, 'Acceptance volume floor'
    assert not acceptance_base.exists(), 'Never overwrite or reuse a customer or prior authority'
    acceptance_base.mkdir(mode=0o700)
    receipt = {'owner': OWNER, 'source': SOURCE, 'host': host, 'isolated': True,
               'at': time.time(), 'installed': False, 'pass': False,
               'acceptanceBase': str(acceptance_base), 'customerDataCopied': False,
               'processesStarted': False, 'steps': []}
    result = acceptance_base / 'install-receipt.json'
    root = acceptance_base / ('native-main-' + SOURCE[:12])
    try:
        archive = Path(producer['artifacts']['ctox-linux-x64.tar.gz']['path']).resolve(strict=True)
        assert sha(archive) == producer['artifacts']['ctox-linux-x64.tar.gz']['sha256']
        # Use the producer's unpacked official runtime after verifying each member
        # against the checksum-verified archive. No developer/customer runtime copy.
        import tarfile
        root.mkdir(mode=0o700)
        with tarfile.open(archive) as package:
            total = 0
            for member in package.getmembers():
                target = (root / member.name).resolve()
                assert target == root or target.is_relative_to(root), 'Archive escaped prefix'
                assert member.isfile() or member.isdir(), 'No symlinks/devices in isolated package'
                total += member.size
                assert total <= 2 * 2**30, 'Official runtime size bound'
            # Ubuntu Python builds before3.12 may lack the backported filter API.
            # Member paths/types were checked above; no links/devices are accepted.
            if hasattr(tarfile, 'data_filter'):
                package.extractall(root, filter='data')
            else:
                package.extractall(root)
        assert not (root / 'runtime').exists(), 'No source authority may be copied'
        binary = root / 'bin/ctox'
        assert sha(binary) == producer['native_binary_sha256']
        provenance = json.loads((root / 'build-provenance.json').read_text())
        assert provenance['source'] == SOURCE and provenance['native_binary_sha256'] == sha(binary)
        env = {**os.environ, 'CTOX_ROOT': str(root), 'CTOX_STATE_ROOT': str(root / 'runtime'),
               'CARGO_BUILD_JOBS': '2', 'RUST_TEST_THREADS': '2'}
        version = subprocess.run([str(binary), 'version'], cwd=root, env=env,
                                 capture_output=True, text=True, timeout=30, check=True)
        assert json.loads(version.stdout)['version'] == 'main-' + SOURCE
        initialized = subprocess.run([str(binary), 'business-os', 'rxdb', 'init', '--root', str(root)],
                                     cwd=root, env=env, stdout=subprocess.DEVNULL,
                                     stderr=subprocess.DEVNULL, timeout=60)
        assert initialized.returncode == 0, 'New isolated database initialization failed'
        marker = {'owner': OWNER, 'source': SOURCE, 'synthetic': True,
                  'archiveSha256': producer['artifacts']['ctox-linux-x64.tar.gz']['sha256']}
        (root / '.devops-isolated-acceptance.json').write_text(json.dumps(marker))
        contract_hashes = {path: sha(root / 'src/apps/business-os' / path) for path in CONTRACTS}
        config = {'owner': OWNER, 'isolated': True, 'source': SOURCE, 'host': host,
                  'root': str(root), 'acceptanceBase': str(acceptance_base), 'port': port,
                  'binary': str(binary), 'binarySha256': sha(binary),
                  'contractHashes': contract_hashes, 'workjetRevision': workjet_revision,
                  'goals': [5, 6, 7], 'output': str(acceptance_base / 'measurement')}
        config_path = acceptance_base / 'runner.private.json'
        with config_path.open('x') as stream:
            os.chmod(config_path, 0o600)
            json.dump(config, stream, indent=2)
        receipt.update(installed=True, binarySha256=sha(binary), contractHashes=contract_hashes,
                       root=str(root), privateConfigReference=str(config_path),
                       steps=['Archive digest verified', 'Official runtime extracted into fresh prefix',
                              'Installed binary version/digest verified', 'New synthetic state initialized'],
                       acceptancePending=True)
    except BaseException as error:
        receipt['failure'] = type(error).__name__
        raise
    finally:
        result.write_text(json.dumps(receipt, indent=2) + '\n')
    return result


if __name__ == '__main__':
    os.umask(0o077)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--producer', required=True, type=Path)
    parser.add_argument('--acceptance-base', required=True, type=Path)
    parser.add_argument('--port', type=int, default=19865)
    parser.add_argument('--host', choices=['gpu3', 'gpu4'], required=True)
    parser.add_argument('--workjet-revision', required=True)
    args = parser.parse_args()
    print(prepare(args.producer, args.acceptance_base.absolute(), args.port, args.host, args.workjet_revision))
