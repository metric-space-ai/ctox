#!/usr/bin/env bash
# Run inside the admitted gpu-build-run lane. --check rechecks a saved report.
set -euo pipefail
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
exec python3 - "$ROOT" "$@" <<'PY'
import datetime, hashlib, json, math, os, pathlib, signal, socket, subprocess, sys, time

ROOT = pathlib.Path(sys.argv[1])
FIXED_HOST = 'gpu3-a4500'
FILTER = 'plugins::replication_webrtc::index_mod::scheduler::lane_artifacts::tests::lane4_real_webrtc_50mb_control_latency_and_verified_arrival'
LIMITS = {'idle_control_p99_ms': 20.0, 'bulk_control_p99_ms': 100.0,
          'bulk_mb_per_second': 5.0, 'association_bytes': 128 * 1024}
SOURCE_FILES = [
    'src/core/rxdb/src/plugins/replication_webrtc/scheduler.rs',
    'src/core/rxdb/src/plugins/replication_webrtc/v4_scheduler/lane_artifacts.rs',
    'src/core/rxdb/src/plugins/replication_webrtc/v4_scheduler/lane_artifacts_tests.rs',
    'src/core/rxdb/patches/rtc/src/peer_connection/mod.rs',
    'src/core/rxdb/patches/webrtc/src/peer_connection/mod.rs',
    'scripts/sync-v4/gate-bench-lane4.sh',
]

def validate(report):
    errors = []
    if report.get('host') != FIXED_HOST:
        errors.append('benchmark requires fixed host ' + FIXED_HOST)
    if report.get('profile') != 'release' or report.get('command_exit') != 0:
        errors.append('release fixture did not complete successfully')
    m = report.get('measured', {})
    def number(key, predicate, reason):
        value = m.get(key)
        if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or not predicate(value):
            errors.append(reason)
    number('bytes', lambda n: n == 50 * 1024 * 1024, 'expected complete 50-MiB transfer')
    number('seconds', lambda n: n > 0, 'invalid transfer duration')
    number('mb_per_second', lambda n: n >= LIMITS['bulk_mb_per_second'], 'bulk throughput below 5 MB/s')
    number('control_roundtrip_p99_ms', lambda n: 0 <= n <= LIMITS['bulk_control_p99_ms'], 'control p99 exceeds 100 ms during bulk')
    number('idle_control_roundtrip_p99_ms', lambda n: 0 <= n <= LIMITS['idle_control_p99_ms'], 'idle control p99 exceeds 20 ms')
    number('sctp_buffer_upper_bound_peak', lambda n: 0 < n <= LIMITS['association_bytes'], 'source exceeds 128-KiB budget')
    number('target_buffer_peak', lambda n: 0 <= n <= LIMITS['association_bytes'], 'target exceeds 128-KiB budget')
    number('control_roundtrips_during_bulk', lambda n: n >= 1, 'control frames waited until bulk completed')
    if m.get('debug_assertions') is not False:
        errors.append('debug measurements are not benchmark acceptance')
    if m.get('sha256_verified') is not True:
        errors.append('complete SHA256 verification missing')
    return errors

def emit(report, path):
    report['criterion'] = LIMITS
    report['errors'] = validate(report)
    report['pass'] = not report['errors']
    path.write_text(json.dumps(report, indent=2, allow_nan=False) + '\n')
    print('LANE4_BENCH_GATE ' + json.dumps({'report': str(path), 'pass': report['pass'], 'errors': report['errors'], 'measured': report.get('measured', {})}), flush=True)
    return 0 if report['pass'] else 1

args = sys.argv[2:]
if len(args) == 2 and args[0] == '--check':
    path = pathlib.Path(args[1]).resolve()
    # Treat saved pass/criterion as untrusted; always reapply canonical limits.
    report = json.loads(path.read_text())
    sys.exit(emit(report, path))
if len(args) > 1:
    raise SystemExit('usage: gate-bench-lane4.sh [OUTPUT_DIR] | --check REPORT.json')
target = os.environ.get('CARGO_TARGET_DIR')
if not target or not os.environ.get('BUILD_LANE_BIN'):
    raise SystemExit('run through gpu-build-run.sh; an admitted lane target is required')
out = pathlib.Path(args[0] if args else pathlib.Path(target) / 'evidence' / ('lane4-bench-' + time.strftime('%Y%m%dT%H%M%SZ', time.gmtime()))).resolve()
if not out.is_relative_to(pathlib.Path('/mnt/sata8t')):
    raise SystemExit('benchmark reports must use the lane SATA volume')
out.mkdir(parents=True, exist_ok=False)
report = {'goal': 'P5 native lane4 performance', 'host': socket.gethostname().split('.')[0],
          'profile': 'release', 'timestamp': datetime.datetime.now(datetime.timezone.utc).isoformat(),
          'scope': 'native-native WebRTC loopback; no TURN or installed-stack claim',
          'head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
          'source_sha256': {p: hashlib.sha256((ROOT / p).read_bytes()).hexdigest() for p in SOURCE_FILES},
          'capacity': {'load': list(os.getloadavg()), 'logical_cpus': os.cpu_count()},
          'command_exit': None, 'measured': {}}
path = out / 'report.json'
report['log'] = str(out / 'fixture.log')
command = ['cargo', 'test', '--release', '--locked', '--manifest-path', 'src/core/rxdb/Cargo.toml',
           '-p', 'ctox-rxdb', '--jobs', '2', '--lib', FILTER, '--', '--exact', '--test-threads=1', '--nocapture']
report['command'] = command
if report['host'] != FIXED_HOST:
    sys.exit(emit(report, path))
child = None

def interrupted(signum, frame):
    raise KeyboardInterrupt()

signal.signal(signal.SIGTERM, interrupted)
start = time.monotonic()
try:
    with (out / 'fixture.log').open('w') as log:
        child = subprocess.Popen(command, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        try:
            report['command_exit'] = child.wait(timeout=900)
        except subprocess.TimeoutExpired:
            report['command_exit'] = 124
except KeyboardInterrupt:
    report['command_exit'] = 130
finally:
    if child is not None and child.poll() is None:
        os.killpg(child.pid, signal.SIGTERM)
        try:
            child.wait(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(child.pid, signal.SIGKILL)
            child.wait()
    report['wall_seconds'] = time.monotonic() - start
    lines = (out / 'fixture.log').read_text().splitlines() if (out / 'fixture.log').exists() else []
    samples = [line.split('LANE4_MEASUREMENT ', 1)[1] for line in lines if 'LANE4_MEASUREMENT ' in line]
    if len(samples) == 1:
        try:
            report['measured'] = json.loads(samples[0])
        except json.JSONDecodeError:
            pass
    rc = emit(report, path)
sys.exit(rc)
PY
