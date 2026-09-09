"""Private per-worktree settings, reloaded on Desktop resume."""
import json
from pathlib import Path

CONTEXT_WINDOW = 256000
COMPACT_LIMIT = 230400


def install(worktree, catalog, run):
    root = Path(worktree)
    directory = root / '.codex'
    config = directory / 'config.toml'
    ignore = directory / '.gitignore'
    paths = ['.codex/config.toml', '.codex/.gitignore']
    if run('git', 'ls-files', '--', *paths, cwd=root):
        raise ValueError('Worker context configuration is tracked; ask the parent to resolve it')
    content = ('# Private worker settings; never publish this file.\n'
               f'model_context_window = {CONTEXT_WINDOW}\n'
               f'model_auto_compact_token_limit = {COMPACT_LIMIT}\n'
               'model_catalog_json = ' + json.dumps(str(catalog)) + '\n')
    expected = {config: content, ignore: '/config.toml\n/.gitignore\n'}
    for path, text in expected.items():
        if path.is_symlink() or (path.exists() and path.read_text() != text):
            raise ValueError('Existing worker context file must not be overwritten: ' + str(path))
    if directory.is_symlink():
        raise ValueError('Worker configuration directory must not be a symlink')
    directory.mkdir(exist_ok=True)
    for path, text in expected.items():
        if not path.exists():
            with path.open('x') as stream:
                stream.write(text)
            path.chmod(0o600)
    ignored = set(run('git', 'check-ignore', '--', *paths, cwd=root).splitlines())
    if ignored != set(paths):
        raise ValueError('Private worker configuration is not excluded from Git')
    return str(config)


def verify(request, worktree):
    result = request(11, 'config/read', {'cwd': str(worktree), 'includeLayers': True})
    config = result.get('config', {})
    if (config.get('model_context_window') != CONTEXT_WINDOW or
            config.get('model_auto_compact_token_limit') != COMPACT_LIMIT):
        raise ValueError('Desktop worktree context settings are not effective; check project trust '
                         'and config layers before dispatch. Do not silently use fallback limits.')
