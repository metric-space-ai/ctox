#!/usr/bin/env python3
"""Prepare provider-scoped Desktop tasks and track their PR lifecycle."""
import argparse
from contextlib import contextmanager
import fcntl
import datetime as dt
import json
import os
from pathlib import Path
import re
import selectors
import subprocess
import time
import uuid
import experience
import availability
import preparation

HOME = Path.home() / '.codex'
JOBS = HOME / 'proxy-workers' / 'jobs'
EXPERIENCE = HOME / 'proxy-workers' / 'MODEL-EXPERIENCE.md'
AVAILABILITY = HOME / 'proxy-workers' / 'AVAILABILITY.json'
OPERATOR_PRIOR = HOME / 'proxy-workers' / 'OPERATOR-EXPERIENCE.json'
BINARY = Path('/Applications/ChatGPT.app/Contents/Resources/codex')
PROXY_MODELS = {'grok-4.6-exact', 'glm-5.3-flash', 'kimi-k3'}

@contextmanager
def job_lock(name):
    JOBS.mkdir(parents=True, exist_ok=True)
    with (JOBS / (name + '.lock')).open('a') as handle:
        try:
            fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise ValueError('Another operation owns this worker registry lock; wait for it to finish') from None
        try:
            yield
        finally:
            fcntl.flock(handle, fcntl.LOCK_UN)


def run(*args, cwd=None):
    return subprocess.check_output(args, cwd=cwd, text=True, stderr=subprocess.PIPE, timeout=30).strip()


def read(path):
    return run('greppy', 'rg', '--no-heading', '--no-line-number', '^', str(path))


def save(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.parent.chmod(0o700)
    stage = path.with_name(path.name + '.' + uuid.uuid4().hex + '.writing')
    stage.write_text(json.dumps(data, indent=2) + '\n')
    stage.chmod(0o600)
    os.replace(stage, path)


def refresh_experience():
    jobs = [json.loads(read(p)) for p in sorted(JOBS.glob('*.json'))]
    EXPERIENCE.parent.mkdir(parents=True, exist_ok=True)
    stage = EXPERIENCE.with_suffix('.writing')
    prior = json.loads(read(OPERATOR_PRIOR)) if OPERATOR_PRIOR.exists() else {}
    stage.write_text(experience.render(jobs, prior))
    stage.chmod(0o600)
    os.replace(stage, EXPERIENCE)


def require_archive_ready(job, pr):
    if pr['state'] != 'MERGED' or not pr.get('mergedAt'):
        raise ValueError('PR has not been merged; do not archive')
    if not any(r.get('pr_head') == pr['headRefOid'] and r.get('pr_state_at_review') == 'OPEN' for r in job.get('review_history', [])):
        raise ValueError('No retrospective recorded for this head before merge')
    review = job.get('learning_review', {})
    if review.get('pr_head') != pr['headRefOid']:
        raise ValueError('Parent must review model experience for the merged PR head before archive')
    if run('git', '-C', job['worktree'], 'symbolic-ref', '--short', 'HEAD') != job['branch']:
        raise ValueError('Worktree branch changed; inspect before archive')
    if run('git', '-C', job['worktree'], 'rev-parse', 'HEAD') != pr['headRefOid']:
        raise ValueError('Worker has additional or different commits; inspect before archive')
    if run('git', '-C', job['worktree'], 'status', '--porcelain'):
        raise ValueError('Worker has uncommitted files; inspect before archive')


def timestamp():
    return dt.datetime.now(dt.timezone.utc).isoformat()


def load_job(thread):
    if not re.fullmatch(r'[0-9a-f-]{36}', thread):
        raise ValueError('Invalid task ID')
    path = JOBS / (thread + '.json')
    return path, json.loads(read(path))


def validate_worktree(repo, worktree):
    repo, worktree = repo.resolve(), worktree.resolve()
    if not Path('/Volumes/tmp').is_mount():
        raise ValueError('/Volumes/tmp is not mounted')
    if not worktree.is_relative_to(Path('/Volumes/tmp/worktrees')):
        raise ValueError('Worker worktree must be under /Volumes/tmp/worktrees')
    entries = run('git', '-C', str(repo), 'worktree', 'list', '--porcelain').split('\n\n')
    if not any(('worktree ' + str(worktree)) in e.splitlines() for e in entries):
        raise ValueError('Worktree is not registered with this repository')
    if worktree == repo:
        raise ValueError('Use a separate worktree')
    branch = run('git', '-C', str(worktree), 'symbolic-ref', '--short', 'HEAD')
    if not branch.startswith('codex/'):
        raise ValueError('Use a dedicated codex/ branch')
    repository = run('gh', 'repo', 'view', '--json', 'nameWithOwner', '--jq', '.nameWithOwner', cwd=repo)
    return repo, worktree, branch, repository


def pr_info(job, url):
    match = re.fullmatch(r'https://github\.com/([^/]+/[^/]+)/pull/([1-9][0-9]*)', url)
    if not match or match[1].lower() != job['repository'].lower():
        raise ValueError('PR must belong to the registered repository')
    result = json.loads(run('gh', 'pr', 'view', url, '--json', 'url,state,mergedAt,headRefName,headRefOid'))
    if result['headRefName'] != job['branch']:
        raise ValueError('PR branch differs from the worker branch')
    return result


def create(args):
    if args.model not in PROXY_MODELS and not args.model.startswith('gpt-'):
        raise ValueError('Unregistered model; validate its provider before adding it')
    deferred = json.loads(read(AVAILABILITY)) if AVAILABILITY.exists() else {}
    if args.model in deferred and availability.active(deferred[args.model]):
        raise ValueError('Model temporarily unavailable until ' + deferred[args.model]['retry_at'] + '; retry then or explicitly select another available model')
    repo, worktree, branch, repository = validate_worktree(args.repo, args.worktree)
    issue_match = re.fullmatch(r'https://github\.com/([^/]+/[^/]+)/issues/([1-9][0-9]*)', args.issue)
    if not issue_match or issue_match[1].lower() != repository.lower():
        raise ValueError('Link a dedicated issue in the same repository')
    issue = json.loads(run('gh', 'issue', 'view', args.issue, '--json', 'url,state,title'))
    if issue['state'] != 'OPEN':
        raise ValueError('The implementation issue must be open')
    JOBS.mkdir(parents=True, exist_ok=True)
    worker_number = 1
    for existing in JOBS.glob('*.json'):
        job = json.loads(read(existing))
        if job.get('parent_thread') == args.parent_thread:
            worker_number = max(worker_number, int(job.get('worker_number', 0)) + 1)
        if job.get('worktree') == str(worktree) and not job.get('archived_at'):
            raise ValueError('An unarchived worker already owns this worktree: ' + job['thread_id'] +
                             '. Inspect ' + str(existing) + ' and recover that task; do not create a duplicate.')
    title = f'[Worker{worker_number}@{args.parent_title}]: {args.title}'
    prompt = read(args.prompt_file)
    if not prompt:
        raise ValueError('Assignment must not be empty')
    provider = 'cli_proxy' if args.model in PROXY_MODELS else 'openai'
    output = Path('/Volumes/tmp/dev-artifacts') / repo.name / ('worker-' + branch.replace('/', '-'))
    output.mkdir(parents=True, exist_ok=True)
    cmd = [str(BINARY), '-c', 'model_catalog_json=' + json.dumps(str(HOME / 'model-catalogs/cli-proxy.json'))]
    config = read(HOME / 'config.toml')
    for name in re.findall(r'^\[mcp_servers\.([^\].]+)\]', config, re.M):
        cmd += ['-c', 'mcp_servers.' + name.strip('"') + '.enabled=false']
    cmd += ['-c', 'features.plugins=false', '-c', 'features.apps=false', '-c', 'notify=[]', 'app-server', '--stdio']
    log_path = output / 'prepare.stderr'
    with log_path.open('w') as log:
        log_path.chmod(0o600)
        proc = subprocess.Popen(cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=log,
                                cwd=worktree, env={**os.environ, 'TMPDIR': str(output)}, start_new_session=True)
        process_file = output / 'process.json'
        sel = None
        job = None
        path = None
        client = None
        try:
            process_file = output / 'process.json'
            save(process_file, {'owner': args.parent_thread, 'pid': proc.pid,
                               'purpose': 'persist task through a READY-only preparation turn', 'stop_condition': 'finally; startup 30 seconds and preparation at most 90 seconds'})
            sel = selectors.DefaultSelector()
            sel.register(proc.stdout, selectors.EVENT_READ)
            client = preparation.Client(proc, sel, time.monotonic() + 30)
            request, send = client.request, client.send
            request(1, 'initialize', {'clientInfo': {'name': 'proxy_worker_setup', 'version': '1.0'},
                                      'capabilities': {'experimentalApi': True}})
            send({'method': 'initialized', 'params': {}})
            result = request(2, 'thread/start', {'model': args.model, 'modelProvider': provider,
                'cwd': str(worktree), 'ephemeral': False,
                'config': {'model_reasoning_effort': args.reasoning}})
            thread = result['thread']['id']
            path = JOBS / (thread + '.json')
            prompt_path = JOBS / (thread + '.prompt.md')
            full_prompt = ('Issue: ' + issue['url'] + '\n\n' + prompt + '\n\nExecution contract: This is an analyzed, bounded implementation task. '
                'Read ~/.codex/skills/proxy-model-workers/SKILL.md before work. '
                'Stay within the assigned scope; return unclear decisions to the parent. '
                'Do not recursively delegate or merge. Use this tmp-volume worktree and the shared heavy-job gate. '
                'Commit, push and open a PR before handoff (draft if unfinished). '
                'After creating the PR, rename this task with set_thread_title to the required_title returned by bind-pr. '
                'Register the PR with worker.py bind-pr --thread ' + thread + ' --pr PR_URL. '
                'Report the PR, pushed commit, validation, remaining processes and cleanup status. '
                'If pushing is blocked, preserve source durably and report the blocker.\n')
            job = {'thread_id': thread, 'host_id': 'local', 'parent_thread': args.parent_thread,
                   'repository': repository, 'issue_url': issue['url'], 'repo': str(repo), 'worktree': str(worktree), 'branch': branch,
                   'model': args.model, 'provider': provider, 'reasoning': args.reasoning,
                   'title': title, 'summary': args.title, 'parent_title': args.parent_title, 'worker_number': worker_number,
                   'prompt_file': str(prompt_path), 'created_at': timestamp(), 'pr_url': None,
                   'rollout_path': result['thread'].get('path'), 'preparation_status': 'preparing'}
            save(path, job)
            if result['thread']['modelProvider'] != provider or result.get('model') != args.model:
                raise ValueError('App-server did not retain the requested model/provider')
            prompt_path.write_text(full_prompt)
            prompt_path.chmod(0o600)
            request(3, 'thread/name/set', {'threadId': thread, 'name': title})
            client.deadline = time.monotonic() + 90
            job['preparation_turn_id'] = client.prepare(thread, job['rollout_path'])
            job.update(preparation_status='ready', prepared_at=timestamp())
            save(path, job)
            print(json.dumps(job, indent=2))
        except Exception as exc:
            if job is not None:
                if client is not None:
                    client.interrupt(job['thread_id'])
                job.update(preparation_status='failed', preparation_error=str(exc),
                           preparation_failed_at=timestamp(),
                           preparation_turn_id=client.turn_id if client else None,
                           recovery='Do not rerun create or dispatch implementation. Inspect this registry and rollout_path; recover the same task through app-server, or have the parent explicitly retire the failed preparation before replacing it.')
                try:
                    save(path, job)
                except OSError as save_error:
                    raise RuntimeError('Preparation failed for task ' + job['thread_id'] +
                                       '; registry write also failed: ' + str(save_error) +
                                       '; original error: ' + str(exc)) from exc
                raise RuntimeError('Preparation failed; existing task retained in ' + str(path) + '\n' +
                                   json.dumps(job, indent=2)) from exc
            raise
        finally:
            proc.terminate()
            try:
                proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()
            if sel is not None:
                sel.close()
            save(process_file, {'pid': proc.pid, 'owner': args.parent_thread, 'status': 'stopped'})


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    create_parser = sub.add_parser('create')
    for name in ['repo', 'worktree', 'prompt-file']:
        create_parser.add_argument('--' + name, type=Path, required=True)
    for name in ['model', 'title', 'parent-thread', 'parent-title', 'issue']:
        create_parser.add_argument('--' + name, required=True)
    create_parser.add_argument('--reasoning', default='high', choices=['low', 'medium', 'high', 'xhigh', 'max'])
    bind = sub.add_parser('bind-pr')
    bind.add_argument('--thread', required=True)
    bind.add_argument('--pr', required=True)
    for command in ['archived', 'ready-to-archive']:
        archive_parser = sub.add_parser(command)
        archive_parser.add_argument('--thread', required=True)
    review_parser = sub.add_parser('record-review')
    review_parser.add_argument('--thread', required=True)
    review_parser.add_argument('--assessment', type=Path, required=True)
    review_parser.add_argument('--head', required=True, help='Exact PR head commit actually assessed by the parent')
    sub.add_parser('experience')
    defer_parser = sub.add_parser('defer')
    defer_parser.add_argument('--model', required=True)
    defer_parser.add_argument('--until', help='Provider reset in ISO 8601 with timezone; defaults to tomorrow')
    defer_parser.add_argument('--note', required=True)
    sub.add_parser('availability')
    status = sub.add_parser('status')
    status.add_argument('--limit', type=int, default=8)
    status.add_argument('--offset', type=int, default=0)
    args = parser.parse_args()
    if args.command == 'create':
        with job_lock('create'):
            create(args)
    elif args.command in ('defer', 'availability'):
        entries = json.loads(read(AVAILABILITY)) if AVAILABILITY.exists() else {}
        if args.command == 'defer':
            entries[args.model] = availability.defer(args.model, args.until, args.note)
            save(AVAILABILITY, entries)
        print(json.dumps({k: {**v, 'currently_deferred': availability.active(v)} for k, v in entries.items()}, indent=2))
    elif args.command == 'experience':
        refresh_experience()
        print(str(EXPERIENCE))
    elif args.command == 'status':
        if args.offset < 0 or not 1 <= args.limit <= 20:
            raise ValueError('Use a nonnegative offset and limit 1..20')
        jobs = [json.loads(read(p)) for p in sorted(JOBS.glob('*.json'))]
        jobs = [j for j in jobs if not j.get('archived_at')]
        result = []
        for job in jobs[args.offset:args.offset + args.limit]:
            item = {k: job[k] for k in ['thread_id', 'parent_thread', 'repository', 'worktree', 'branch', 'pr_url']}
            if job.get('pr_url'):
                try:
                    item['pr'] = pr_info(job, job['pr_url'])
                except (ValueError, subprocess.SubprocessError) as exc:
                    item['error'] = str(exc)
            result.append(item)
        print(json.dumps({'jobs': result, 'next_offset': args.offset + args.limit if len(jobs) > args.offset + args.limit else None}, indent=2))
    else:
        path, job = load_job(args.thread)
        url = args.pr if args.command == 'bind-pr' else job.get('pr_url')
        if not url:
            raise ValueError('No PR registered')
        if job.get('pr_url') and job['pr_url'] != url:
            raise ValueError('A disposable worker is bound to exactly one PR')
        pr = pr_info(job, url)
        if args.command == 'bind-pr':
            head = run('git', '-C', job['worktree'], 'rev-parse', 'HEAD')
            if head != pr['headRefOid']:
                raise ValueError('Worktree HEAD does not equal the pushed PR head')
            pr_number = pr['url'].rstrip('/').split('/')[-1]
            job.update(pr_url=pr['url'], pushed_commit=head, pr_registered_at=timestamp(),
                       required_title=f"#[PR{pr_number}]: {job['summary']}")
        elif args.command == 'record-review':
            if pr['state'] not in ('OPEN', 'MERGED'):
                raise ValueError('Retrospective requires an open or merged PR')
            if args.head != pr['headRefOid']:
                raise ValueError('PR changed since the parent assessment; review the new head first')
            assessment = experience.validate_assessment(json.loads(read(args.assessment)))
            if pr['state'] == 'MERGED' and not any(r.get('pr_head') == args.head and r.get('pr_state_at_review') == 'OPEN' for r in job.get('review_history', [])):
                raise ValueError('The initial retrospective must be recorded before merge')
            assessment.update(reviewed_at=timestamp(), pr_url=pr['url'], pr_head=pr['headRefOid'], pr_state_at_review=pr['state'])
            job.setdefault('review_history', []).append(assessment)
            job['learning_review'] = assessment
            save(path, job)
            refresh_experience()
        else:
            require_archive_ready(job, pr)
            if args.command == 'ready-to-archive':
                print(json.dumps({'thread_id': job['thread_id'], 'ready': True, 'next_step': 'Verify Desktop task is idle, then set_thread_archived and record archived.'}))
                return
            job.update(archived_at=timestamp(), merged_at=pr['mergedAt'])
        save(path, job)
        print(json.dumps(job, indent=2))


if __name__ == '__main__':
    try:
        with job_lock('registry'):
            main()
    except (ValueError, RuntimeError, TimeoutError, OSError, subprocess.SubprocessError) as exc:
        raise SystemExit(str(exc))
