#!/usr/bin/env python3
"""Build on the compiler lane; consume the same binaries on the private fixture host."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import time

assert sys.platform == 'linux' and os.getenv('BUILD_LANE_BIN')
mode = sys.argv[1]
assert mode in ('build', 'consume')
source = Path.cwd()
base = (Path(os.environ['BUILD_LANE_BIN']) / '../artifacts').resolve()
out = base / ('v4-e2e-m7-fix-' + mode + '-' + time.strftime('%Y%m%dT%H%M%SZ', time.gmtime()))
out.mkdir(mode=0o700)
fixture = base / 'v4-m7-fix-fixture-20261016'
old = base / 'v4-e2e-old-portable-20261015'
record = {'scope': mode, 'startedAt': time.time(), 'source': str(source), 'steps': [], 'terminal': False, 'pass': False, 'fullM7Accepted': False}
env = dict(os.environ, CARGO_PROFILE_DEV_DEBUG='0', CARGO_PROFILE_TEST_DEBUG='0', CARGO_INCREMENTAL='0', CARGO_BUILD_JOBS='2', RUST_TEST_THREADS='2', CMAKE_BUILD_PARALLEL_LEVEL='2')
sha = lambda p: hashlib.sha256(Path(p).read_bytes()).hexdigest()
product = source / 'src/core/install/mod.rs'
gold = product.read_text()
assignments = ('                state.current_release = Some(result.current_release.clone());\n'
               '                state.previous_release = result.previous_release.clone();\n')

def save():
    (out/'qualification.json').write_text(json.dumps(record, indent=2))

def run(name, command, expected=0, timeout=2400):
    at = time.time()
    with (out/(name+'.log')).open('w') as log:
        p = subprocess.Popen(command, env=env, stdout=log, stderr=log, start_new_session=True)
        row = {'name': name, 'command': command, 'pid': p.pid, 'pgid': p.pid, 'stopAfterSeconds': timeout}
        record['steps'].append(row);save()
        try:
            code = p.wait(timeout=timeout)
        except BaseException:
            os.killpg(p.pid, signal.SIGTERM)
            try:p.wait(timeout=5)
            except subprocess.TimeoutExpired:os.killpg(p.pid, signal.SIGKILL);p.wait()
            raise
    row.update(exit=code, seconds=time.time()-at);save()
    print(name, 'exit', code, flush=True)
    if expected is not None and code != expected:
        print((out/(name+'.log')).read_text()[-14000:], flush=True)
        raise RuntimeError(name+' unexpected exit '+str(code))
    return code

def receipt(name):
    binary = out/('ctox-'+name)
    shutil.copy2(Path(env['CARGO_TARGET_DIR'])/'debug/ctox', binary)
    value = {'revision': 'fb2f6ac1557e5ea7879f1cae4d899b416a296d02', 'source': str(source), 'sourceModified': True,
             'qualificationVariant': name, 'terminal': True, 'pass': True, 'binary': str(binary), 'binarySha256': sha(binary),
             'sourceHashes': {'src/core/install/mod.rs': sha(product), 'src/core/install/rollback_tests.rs': sha(source/'src/core/install/rollback_tests.rs')}}
    p=out/(name+'-build.json');p.write_text(json.dumps(value,indent=2));return p

try:
    run('runner-syntax', ['node','--check','src/core/install/rollback_rehearsal.mjs'])
    run('test-format', ['rustfmt','--check','--edition','2021','src/core/install/rollback_tests.rs'])
    if mode == 'build':
        previous = Path(sys.argv[2]);old_build=json.loads(previous.read_text())
        assert gold.count(assignments)==1
        assert old_build['sourceHashes']['src/core/install/mod.rs']==hashlib.sha256(gold.replace(assignments,'').encode()).hexdigest()
        assert sha(old_build['binary'])==old_build['binarySha256']
        run('prepare',['bash','-lc','ctox-prep.sh'])
        build=['cargo','build','--locked','--bin','ctox','--no-default-features','-j','2']
        run('candidate-build',build,timeout=1200);candidate=receipt('fixed')
        run('restore-invariant',['cargo','test','--locked','--no-default-features','--bin','ctox','-j','2','restore_state_backup_','--','--test-threads=1'],timeout=1200)
        begin=gold.index("    // The operation's live control store");end=gold.index('        path.file_name()',begin)
        mutant=gold[:begin]+'    copy_filtered(backup_root, state_root, &|path, _| {\n'+gold[end:]
        needle='        if database == maintenance {\n            continue;\n        }\n';assert mutant.count(needle)==1
        product.write_text(mutant.replace(needle,''))
        run('mutation-build',build,timeout=1200);mutated=receipt('mutated')
        product.write_text(gold)
        shutil.copy2(old_build['binary'],out/'ctox-metadata-mutated')
        old_build['binary']=str(out/'ctox-metadata-mutated');old_build['qualificationVariant']='metadata-mutated'
        (out/'metadata-mutated-build.json').write_text(json.dumps(old_build,indent=2))
        for name in ['mod.rs','rollback_tests.rs','rollback_rehearsal.mjs','rollback_qualify.py']:
            shutil.copy2(source/'src/core/install'/name,out/('source-'+name))
        capsule={p.name:sha(p) for p in out.iterdir() if p.name.startswith(('ctox-','source-')) or p.name.endswith('-build.json')}
        (out/'capsule.json').write_text(json.dumps(capsule,indent=2))
        members=list(capsule)+['capsule.json']
        run('package',['tar','-I','zstd -T2 -3','-cf',str(out/'package.tar.zst'),'-C',str(out),*members],timeout=180)
        record.update(pass_=True,passBuild=True,package=str(out/'package.tar.zst'),packageSha256=sha(out/'package.tar.zst'),packageBytes=(out/'package.tar.zst').stat().st_size)
        record['pass']=True
    else:
        package=Path(sys.argv[2]).resolve()
        capsule=json.loads((package/'capsule.json').read_text())
        for name,digest in capsule.items():assert sha(package/name)==digest, name
        for name in ['mod.rs','rollback_tests.rs','rollback_rehearsal.mjs','rollback_qualify.py']:
            assert sha(source/'src/core/install'/name)==capsule['source-'+name], 'consumer source differs: '+name
        assets=base/'v4-e2e-m7-fb2-received-r2-20261015/verified/source'
        assert (assets/'contracts/binary_bundle_manifest.txt').is_file()
        receipts={}
        for variant in ['fixed','mutated','metadata-mutated']:
            value=json.loads((package/(variant+'-build.json')).read_text())
            value['binary']=str(package/Path(value['binary']).name);assert sha(value['binary'])==value['binarySha256']
            value['source']=str(assets);value['runtimeAssetsRevision']=value['revision'];value['compiledSourceCapsule']=str(package/'capsule.json')
            p=out/(variant+'-build.json');p.write_text(json.dumps(value,indent=2));receipts[variant]=str(p)
        assert json.loads(Path(receipts['fixed']).read_text())['sourceHashes']['src/core/install/mod.rs']==sha(product)
        args=['node','src/core/install/rollback_rehearsal.mjs','rollback',str(fixture),str(old/'build.json')]
        run('rollback-fixed',args+[receipts['fixed'],str(out/'fixed')],timeout=660)
        record['fixed']=json.loads((out/'fixed/verdict.json').read_text());save()
        run('metadata-mutated',args+[receipts['metadata-mutated'],str(out/'metadata-mutated')],expected=1,timeout=660)
        failure=json.loads((out/'metadata-mutated/run/result.json').read_text())
        assert 'Rollback maintenance receipt disagrees with the actual release' in failure['failure']['error']
        assert any(c['args']==['update','rollback'] and c['code']==0 for c in failure['installer']['calls'])
        record['metadataMutation']={'red':True,'cliRollbackExit':0,'sourceDelta':'omit only completion current_release/previous_release assignments'};save()
        run('rollback-mutated',args+[receipts['mutated'],str(out/'mutated')],expected=1,timeout=660)
        failure=json.loads((out/'mutated/run/result.json').read_text())
        log='\n'.join(p.read_text() for p in (out/'mutated/run/upgrade').glob('installer-*.log'))
        assert 'CTOX maintenance lease changed while upgrade was running' in log
        state=json.loads((Path(failure['root'])/'runtime/update_state.json').read_text())
        old_hash=json.loads((old/'build.json').read_text())['binarySha256']
        assert state['phase']=='rolled_back' and sha(Path(failure['root'])/'managed/current/bin/ctox')==old_hash
        record['mutation']={'red':True,'phase':state['phase'],'activeOldSha256':old_hash};save()
        run('rollback-restored',args+[receipts['fixed'],str(out/'restored')],timeout=660)
        record['restored']=json.loads((out/'restored/verdict.json').read_text());save()
        record['rollbackQualified']=record['fixed']['pass'] and record['restored']['pass'] and record['mutation']['red'] and record['metadataMutation']['red']
        static=['node','src/core/install/rollback_rehearsal.mjs','static',str(fixture),str(out/'restored/run/result.json'),str(out/'static'),str(base/'v4-m7-fix-signed-reference-20261016/0.1.46-beta.106')]
        record['staticExit']=run('static-http',static,expected=None,timeout=190)
        if (out/'static/static.json').exists():record['static']=json.loads((out/'static/static.json').read_text())
        record['pass']=record['rollbackQualified'] and record.get('static',{}).get('pass',False)
except Exception as error:
    record['error']=str(error)
    raise
finally:
    product.write_text(gold)
    record.update(sourceRestored=product.read_text()==gold,terminal=True,finishedAt=time.time());save()
    print(json.dumps({'output':str(out),'pass':record['pass'],'error':record.get('error'),'staticExit':record.get('staticExit')}),flush=True)
if not record['pass']:sys.exit(1)
