import { execFileSync } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
const project = '/home/mika/Projects/Humaify/zuruf/main';
if (!process.argv.includes('--create')) {
  throw new Error('Explicit --create required: this creates one real T3 thread and worktree.');
}
const directory = `${process.env.HOME}/.local/share/zuruf/feasibility`;
mkdirSync(directory, { recursive: true });
writeFileSync(`${directory}/probe.txt`, 'ZURUF_ASSET_PROOF_20261110\n', { mode: 0o600 });
const title = `zuruf-feasibility-worktree-${Date.now()}`;
const output = execFileSync('t3', [
  'thread', 'start', '--title', title, '--provider', 'codex', '--model', 'gpt-6-luna',
  '--worktree', 'HEAD', '--branch', title, '--json', project,
  `Read ${directory}/probe.txt and reply with its exact contents and your working directory. Do not modify any files.`,
], { cwd: project, encoding: 'utf8', timeout: 60000 });
console.log(output);
