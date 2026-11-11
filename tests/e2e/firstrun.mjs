// Explicit live acceptance: one temporary OAuth identity and one read-only T3 thread.
import { chromium } from '../../apps/page/node_modules/playwright-core/index.mjs';
import { spawn } from 'node:child_process';
import { mkdtemp, readFile, rm, mkdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { createServer } from 'node:http';
import assert from 'node:assert/strict';
const root = resolve(import.meta.dirname, '../..');
const directory = await mkdtemp(`${tmpdir()}/zuruf-firstrun-`);
const port = 47643;
const base = `http://127.0.0.1:${port}`;
const env = { ...process.env, ZURUF_PORT: String(port), ZURUF_DATA_DIR: `${directory}/data`, ZURUF_STATE_DIR: `${directory}/state`, ZURUF_CONFIG_DIR: `${directory}/config` };
delete env.ZURUF_FAKE_ADAPTERS;
delete env.T3_URL;
const sleep = ms => new Promise(r => setTimeout(r, ms));
async function until(fn, timeout = 60000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) { const value = await fn(); if (value) return value; await sleep(100); }
  throw Error('condition timeout');
}
let token, daemon, browser;
const evidence = { stages: [], threads: [], oauthRevoked: false };
const server = createServer((req, res) => { res.setHeader('Content-Type', 'text/html'); res.end('<!doctype html><title>Zuruf First run</title><h1>Lokale Erststartprüfung</h1>'); });
await new Promise(r => server.listen(0, '127.0.0.1', r));
const fixture = `http://127.0.0.1:${server.address().port}`;
async function api(path, body, method = 'POST') {
  const response = await fetch(`${base}/api/v1${path}`, { method, headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json' }, body: body === undefined ? undefined : JSON.stringify(body) });
  const value = await response.json();
  assert(response.ok, `${path}: ${JSON.stringify(value)}`);
  return value;
}
const state = () => api('/state', undefined, 'GET');
try {
  daemon = spawn(resolve(root, 'target/debug/zuruf'), ['daemon'], { cwd: root, env, stdio: 'ignore' });
  await until(async () => { try { token = (await readFile(`${env.ZURUF_STATE_DIR}/token`, 'utf8')).trim(); return await state(); } catch { assert(daemon.exitCode === null, 'daemon exited'); return false; } });
  const empty = await state();
  assert.equal(empty.status.t3.state, 'disconnected');
  assert.equal(empty.status.t3.version ?? null, null);
  assert.equal(empty.status.t3.error ?? null, null);
  assert.equal(empty.projects.length, 0);
  evidence.stages.push('empty: disconnected, no version/error/projects');
  const connection = await api('/t3/pair', process.env.T3_URL ? { origin: process.env.T3_URL } : {});
  assert.equal(connection.state, 'connected');
  assert(connection.version && connection.version !== 'fake-1');
  evidence.version = connection.version;
  const identity = JSON.parse(await readFile(`${env.ZURUF_STATE_DIR}/t3-credentials.json`, 'utf8')).session_id;
  evidence.oauthSessionId = identity;
  await api('/t3/pair', {});
  assert.equal(JSON.parse(await readFile(`${env.ZURUF_STATE_DIR}/t3-credentials.json`, 'utf8')).session_id, identity);
  await api('/t3/reconnect', {});
  const cli = spawn(resolve(root, 'target/debug/zuruf'), ['t3', 'pair', '--origin', process.env.T3_URL || 'http://127.0.0.1:3773'], { cwd: root, env, stdio: 'ignore' });
  assert.equal(await new Promise(r => cli.once('exit', r)), 0);
  assert.equal((await state()).status.t3.state, 'connected');
  assert.equal(JSON.parse(await readFile(`${env.ZURUF_STATE_DIR}/t3-credentials.json`, 'utf8')).session_id, identity);
  evidence.stages.push('paired live, repeated pair and CLI reuse identity, reconnect works');
  const catalog = await api('/t3/catalog', undefined, 'GET');
  const projects = await api('/t3/projects', undefined, 'GET');
  const registered = projects.find(p => p.workspaceRoot === '/home/mika/Projects/Humaify/zuruf/main');
  assert(registered, 'Zuruf repository must be registered in T3');
  const project = await api('/projects', { name: 'First run', repositoryPath: registered.workspaceRoot, baseUrl: fixture });
  assert.equal(project.t3ProjectId, registered.id);
  let snapshot = await state();
  assert(snapshot.status.activeReviewId && snapshot.status.activeBatchId);
  const provider = catalog.providers.find(p => p.enabled && p.instanceId === 'codex');
  const model = provider?.models.find(m => m.id === 'gpt-6-luna') ?? provider?.models[0];
  assert(model, 'Live Codex model required');
  const profile = await api('/profiles', { name: 'First run', providerInstanceId: provider.instanceId, model: model.id, options: {}, runtimeMode: catalog.runtimeModes.includes('approval-required') ? 'approval-required' : catalog.runtimeModes[0], interactionMode: catalog.interactionModes[0], workspaceStrategy: 'existing_worktree', worktreePath: root });
  snapshot = await state();
  assert.equal(snapshot.projects[0].defaultProfileId, profile.id);
  assert.equal(snapshot.batches[0].profileId, profile.id);
  evidence.stages.push('catalog, projects, project/review/draft, first profile defaults');
  const session = await api('/sessions', { projectId: project.id, name: 'First run', mode: 'isolated', agentAccess: true });
  await api(`/sessions/${session.id}/start`, {});
  const access = await api(`/sessions/${session.id}/agent-access`, { agentId: 'firstrun-test' });
  await api(`/sessions/${session.id}/control`, { action: 'grant', agentId: 'firstrun-test', leaseSeconds: 300 });
  browser = await chromium.connectOverCDP(`ws://127.0.0.1:${port}/cdp/${session.id}`, { headers: { Authorization: `Bearer ${access.token}` } });
  const page = await until(() => browser.contexts()[0].pages().find(p => p.url().startsWith(fixture)));
  await page.bringToFront();
  await page.waitForTimeout(1200);
  await api('/review-mode', { on: true });
  await page.mouse.move(200, 150);
  await page.keyboard.press('w');
  const annotation = await until(async () => (await state()).annotations.find(a => a.assets.some(asset => asset.kind === 'screenshot_marked')));
  evidence.stages.push('session starts at baseUrl, review on, W capture persisted');
  const batch = (await state()).status.activeBatchId;
  await api(`/batches/${batch}`, { title: `zuruf-test-firstrun-${Date.now()}`, note: 'Transport test only. Reply exactly OK. Do not use tools or change any files.' }, 'PATCH');
  const dispatched = await api(`/batches/${batch}/dispatch`, {});
  const task = await until(async () => (await state()).tasks.find(t => t.id === dispatched.task.id && t.threadId));
  evidence.threads.push({ id: task.threadId, link: task.threadUrl, title: task.title });
  await api(`/tasks/${task.id}/cancel`, {});
  evidence.stages.push('dispatch creates real T3 thread, interrupted');
  await mkdir(`${directory}/second`);
  const second = await api('/projects', { name: 'Second', repositoryPath: `${directory}/second`, baseUrl: fixture });
  assert.equal((await state()).status.reviewMode, false);
  await api('/active', { projectId: project.id });
  snapshot = await state();
  assert.equal(snapshot.status.activeSessionId, session.id);
  assert.equal(snapshot.batches.find(b => b.id === snapshot.status.activeBatchId).projectId, project.id);
  await api('/active', { projectId: second.id });
  assert.equal((await state()).status.activeSessionId ?? null, null);
  await api('/active', { projectId: project.id });
  await api(`/sessions/${session.id}/stop`, {});
  assert(annotation.id);
  evidence.stages.push('second project and back, session selection restored, browser stopped');
} finally {
  const errors = [];
  await browser?.close().catch(e => errors.push(String(e)));
  if (token) {
    const snapshot = await state().catch(() => null);
    for (const task of snapshot?.tasks ?? []) if (!['completed', 'failed', 'cancelled'].includes(task.status)) await api(`/tasks/${task.id}/cancel`, {}).catch(e => errors.push(String(e)));
    await api('/shutdown', {}).catch(e => errors.push(String(e)));
  }
  if (daemon && daemon.exitCode === null) {
    await Promise.race([new Promise(r => daemon.once('exit', r)), sleep(10000)]);
    if (daemon.exitCode === null) { daemon.kill('SIGTERM'); await new Promise(r => daemon.once('exit', r)); }
  }
  const credentials = await readFile(`${env.ZURUF_STATE_DIR}/t3-credentials.json`, 'utf8').then(JSON.parse).catch(() => null);
  if (credentials) {
    evidence.oauthSessionId = credentials.session_id;
    const revoke = spawn('t3', ['auth', 'session', 'revoke', credentials.session_id], { stdio: 'ignore' });
    const code = await new Promise(r => revoke.once('exit', r));
    evidence.oauthRevoked = code === 0;
    if (code !== 0) errors.push('OAuth revocation failed');
  }
  await new Promise(r => server.close(r));
  if (!errors.length) await rm(directory, { recursive: true, force: true });
  evidence.cleanupErrors = errors;
  console.log(JSON.stringify(evidence, null, 2));
  assert.equal(errors.length, 0, 'Cleanup failed; temporary directory retained');
}
