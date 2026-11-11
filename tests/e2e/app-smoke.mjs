// Hyprland check of the app window: `zuruf open` starts the daemon, shows one window with the
// app id "zuruf", and a second `open` focuses that same window instead of opening another.
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { resolve } from 'node:path';
import assert from 'node:assert/strict';
const exec = promisify(execFile);
const root = resolve(import.meta.dirname, '../..');
const home = process.env.HOME;
const env = {
  ...process.env,
  ZURUF_PORT: '47638',
  ZURUF_DATA_DIR: process.env.ZURUF_SMOKE_DATA || `${home}/.local/share/zuruf/e2e-app`,
  ZURUF_STATE_DIR: process.env.ZURUF_SMOKE_STATE || `${home}/.local/state/zuruf/e2e-app`,
  ZURUF_CONFIG_DIR: process.env.ZURUF_SMOKE_CONFIG || `${home}/.local/share/zuruf/e2e-app-config`,
};
delete env.ZURUF_FAKE_ADAPTERS;
delete env.T3_URL;
const cli = (...args) => exec(process.env.ZURUF_SMOKE_BIN || resolve(root, 'target/debug/zuruf'), args, { env });
const windows = async () => {
  const { stdout } = await exec('hyprctl', ['clients', '-j']);
  return JSON.parse(stdout).filter((client) => client.class === 'zuruf');
};
try {
  const before = new Set((await windows()).map((client) => client.address));
  await cli('open', '--view', 'sessions');
  const { stdout } = await cli('status');
  assert.equal(JSON.parse(stdout).status.protocolVersion, 1);
  let own = [];
  for (let attempt = 0; attempt < 100 && own.length === 0; attempt++) {
    own = (await windows()).filter((client) => !before.has(client.address));
    if (own.length === 0) await new Promise((done) => setTimeout(done, 100));
  }
  assert.equal(own.length, 1, 'expected exactly one new window with app id "zuruf"');
  await cli('open', '--view', 'settings');
  await new Promise((done) => setTimeout(done, 300));
  const { stdout: focused } = await exec('hyprctl', ['activewindow', '-j']);
  assert.equal(JSON.parse(focused).address, own[0].address, 'the second open focuses the same window');
  const after = (await windows()).filter((client) => !before.has(client.address));
  assert.equal(after.length, 1, 'the second open must not create another window');
  console.log('Real app window and daemon autostart: passed; app id zuruf, pid ' + own[0].pid);
} finally {
  await cli('stop');
}
