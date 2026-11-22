"""Register the two local MCP endpoints without replacing other servers."""
import argparse
import json
from pathlib import Path
import os
import shutil
import tomllib
from datetime import datetime


def save(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists():
        backup = path.with_name(path.name + '.' + datetime.now().strftime('%Y%m%d%H%M%S%f') + '.bak')
        shutil.copy2(path, backup)
    temp = path.with_name(path.name + '.cursor-mcp.tmp')
    temp.write_text(text, encoding='utf-8')
    temp.replace(path)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--workspace', type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument('--cli-command', help='JSON argv array, e.g. ["/path/to/agent"]')
    args = parser.parse_args()
    workspace = args.workspace.resolve(strict=True)
    script = Path(__file__).with_name('server.mjs').resolve(strict=True)
    node = shutil.which('node')
    if not node:
        parser.error('Node.js 22+ is required')
    cli = json.loads(args.cli_command) if args.cli_command else None
    if cli is None and os.name == 'nt':
        versions = Path(os.environ['LOCALAPPDATA']) / 'cursor-agent' / 'versions'
        installed = sorted((p for p in versions.glob('*') if (p / 'node.exe').is_file() and (p / 'index.js').is_file()), key=lambda p: p.stat().st_mtime, reverse=True)
        if installed:
            cli = [str(installed[0] / 'node.exe'), str(installed[0] / 'index.js')]
    if cli is not None and (not isinstance(cli, list) or not cli or any(not isinstance(v, str) or not v for v in cli)):
        parser.error('--cli-command must be a nonempty JSON string array')
    common = [str(script), '--workspace', str(workspace)]
    codex_path = workspace / '.codex' / 'config.toml'
    original = codex_path.read_text(encoding='utf-8-sig') if codex_path.exists() else ''
    config = tomllib.loads(original)
    if 'cursor' in config.get('mcp_servers', {}):
        parser.error(f'{codex_path} already has mcp_servers.cursor; update it manually')
    entry = '\n[mcp_servers.cursor]\ncommand = ' + json.dumps(node) + '\nargs = ' + json.dumps(common + ['--role', 'codex']) + '\nstartup_timeout_sec = 10\ntool_timeout_sec = 1860\n'
    if cli:
        entry += '\n[mcp_servers.cursor.env]\nCURSOR_MCP_COMMAND = ' + json.dumps(json.dumps(cli)) + '\n'
    tomllib.loads(original + entry)
    cursor_path = workspace / '.cursor' / 'mcp.json'
    cursor = json.loads(cursor_path.read_text(encoding='utf-8-sig')) if cursor_path.exists() else {}
    servers = cursor.setdefault('mcpServers', {})
    if 'cursor_handoff' in servers:
        parser.error(f'{cursor_path} already has cursor_handoff; update it manually')
    servers['cursor_handoff'] = {'command': node, 'args': common + ['--role', 'cursor']}
    save(codex_path, original + entry)
    save(cursor_path, json.dumps(cursor, ensure_ascii=False, indent=2) + '\n')
    print(f'Codex: {codex_path}\nCursor: {cursor_path}')
    print('CLI command: ' + (json.dumps(cli) if cli else 'agent (from PATH)'))
    print('Reload the workspace in both applications to load the MCP tools.')


if __name__ == '__main__':
    main()
