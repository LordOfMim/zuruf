// Run in a memory-capped systemd unit. This creates ONE real T3 test thread.
import { chromium } from '../../apps/page/node_modules/playwright-core/index.mjs';
import { spawn } from 'node:child_process';
import { readFile, mkdir, writeFile, copyFile, chmod } from 'node:fs/promises';
import { createServer } from 'node:http';
import { resolve } from 'node:path';
import assert from 'node:assert/strict';
const root = resolve(import.meta.dirname, '../..');
const out = resolve(root, '.spike/wiring');
await mkdir(out, { recursive: true, mode: 0o700 });
const run = String(Date.now());
const directory = resolve(process.env.HOME, ".local/share/zuruf/e2e", run);
await mkdir(directory, { recursive: true, mode: 0o700 });
const env = { ...process.env, ZURUF_PORT: '47639', ZURUF_DATA_DIR: `${directory}/data`, ZURUF_STATE_DIR: `${directory}/state`, ZURUF_CONFIG_DIR: `${directory}/config` };
delete env.ZURUF_FAKE_ADAPTERS;
const base = 'http://127.0.0.1:47639';
const sleep = ms => new Promise(r => setTimeout(r, ms));
const evidence = { run, timings: {}, threads: [] };
let token, browser, daemon;
const server = createServer(async (req, res) => { res.setHeader('Content-Type','text/html'); res.end(await readFile(resolve(root, 'tests/browser/fixture/index.html'))); });
await new Promise(r => server.listen(0, '127.0.0.1', r));
const fixture = `http://127.0.0.1:${server.address().port}`;
async function api(path, body, method = 'POST') {
 const response = await fetch(`${base}/api/v1${path}`, { method, headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json' }, body: body === undefined ? undefined : JSON.stringify(body) });
 const data = await response.json(); assert(response.ok, JSON.stringify(data)); return data;
}
const state = () => api('/state', undefined, 'GET');
async function until(fn, limit = 20000) { const end = Date.now()+limit; while(Date.now()<end) { const result=await fn(); if(result) return result; await sleep(50); } throw Error('condition timeout'); }
try {
 assert(process.env.T3_URL, 'Set T3_URL to the verified local T3 origin');
 // Credential stays private outside the repository and is reused across test runs.
 const credentialState = process.env.ZURUF_E2E_CREDENTIAL_STATE || `${process.env.HOME}/.local/state/zuruf`;
 await mkdir(env.ZURUF_STATE_DIR, {recursive:true,mode:0o700});
 await copyFile(`${credentialState}/t3-credentials.json`, `${env.ZURUF_STATE_DIR}/t3-credentials.json`);
 await chmod(`${env.ZURUF_STATE_DIR}/t3-credentials.json`, 0o600);
 daemon = spawn(process.env.ZURUF_E2E_BINARY || resolve(root, 'target/debug/zuruf'), ['daemon'], { cwd: root, env, stdio: ['ignore','ignore','pipe'] });
 let stderr = ''; evidence.stages=[]; daemon.stderr.on('data', d => { stderr += d; for(const line of String(d).split('\n')) { const match=line.replace(/\x1b\[[0-9;]*m/g,'').match(/ZURUF_CAPTURE (\{.*\})/);if(match) evidence.stages.push(JSON.parse(match[1])); } });
 await until(async()=> { try { token=(await readFile(`${env.ZURUF_STATE_DIR}/token`,'utf8')).trim(); return (await state()).status.t3.state==='connected'; } catch { if(daemon.exitCode!==null) throw Error(stderr); return false; } }, 60000);
 const s = await state();
 const rawEvents=[];
 const ws = new WebSocket(base.replace('http:','ws:')+'/api/v1/events');
 await new Promise((resolve,reject)=>{ws.onopen=()=>{ws.send(JSON.stringify({type:'auth',token}));resolve()};ws.onerror=reject});
 ws.onmessage=e=>{const v=JSON.parse(e.data);if(v.type==='annotation.upserted'&&v.payload.assets.some(a=>a.kind==='screenshot')&&!v.payload.assets.some(a=>a.kind==='screenshot_marked'))rawEvents.push({id:v.payload.id,at:Date.now()})};
 daemon.once('exit',()=>ws.close());
 const profile = await api('/profiles', { name: 'E2E', providerInstanceId: 'codex', model: 'gpt-6-luna', options: {reasoningEffort:'low'}, runtimeMode:'approval-required', interactionMode:'default', workspaceStrategy:'existing_worktree', worktreePath:root });
 const project = await api('/projects', { name:'E2E', repositoryPath:process.env.ZURUF_E2E_PROJECT || '/home/mika/Projects/Humaify/zuruf/main', baseUrl:fixture, defaultProfileId:profile.id });
 const review = await api('/reviews', {projectId:project.id,name:'E2E'});
 const session = await api('/sessions', {projectId:project.id,name:'E2E',mode:'shared',agentAccess:true});
 await api(`/sessions/${session.id}/start`,{});

 await api('/active',{sessionId:session.id,reviewId:review.id,projectId:project.id});
 const {token:agentToken} = await api(`/sessions/${session.id}/agent-access`,{agentId:'e2e'});
 await api(`/sessions/${session.id}/control`,{action:'grant',agentId:'e2e',leaseSeconds:300});
 browser = await chromium.connectOverCDP(`ws://127.0.0.1:47639/cdp/${session.id}`, {headers:{Authorization:`Bearer ${agentToken}`}});
 const page = await until(()=>browser.contexts()[0].pages().find(p=>p.url().startsWith(fixture)));
 await page.bringToFront(); await sleep(1000);
 await page.evaluate(()=>{
   window.captureMeasurements=[];
   window.addEventListener('zuruf:capture-start',e=>window.captureMeasurements.push({keyAt:e.detail}));
   window.addEventListener('zuruf:capture-confirmed',e=>{const last=window.captureMeasurements.find(x=>!x.confirmedAt);if(last)last.confirmedAt=e.detail});
 });
 await api('/review-mode',{on:true});
 const settings = s.settings; settings.speech.model='base.en'; settings.speech.language='en'; settings.speech.backend='continuous';
 await api('/settings',settings,'PUT');
 await api('/actions/speech.toggle',{});
 await page.locator('h1').click();
 for (const key of ['w','s','d']) {
  const before=(await state()).annotations.length; const start=performance.now();
  await page.mouse.move(450,380); await page.keyboard.press(key);
  if(key==='s'){await page.mouse.move(420,370);await page.mouse.down();await page.mouse.move(500,410);await page.mouse.up();}
  await until(async()=> (await state()).annotations.length>before);
  await until(async()=> (await state()).annotations.at(-1).assets.some(a=>a.kind==='screenshot_marked'));
  await page.evaluate(()=> {
    if(window.captureMeasurements.at(-1)?.confirmedAt)return;
    return new Promise((resolve,reject)=>{
      const timer=setTimeout(()=>reject(Error('Capture confirmation missing')),3000);
      window.addEventListener('zuruf:capture-confirmed',()=>{clearTimeout(timer);resolve()},{once:true});
    });
  });
  evidence.timings[key]=performance.now()-start;
 }
 evidence.captureEvents=await page.evaluate(()=>window.captureMeasurements);
 evidence.rawEvents=rawEvents;
 if(process.argv.includes('--capture-only')) {
  const before=(await state()).annotations.length;
  await page.keyboard.press('w');await page.keyboard.press('d');
  const captures=await until(async()=> {const a=(await state()).annotations.slice(before);return a.length===2&&a.every(a=>a.assets.length>0)?a:false});
  assert.deepEqual(captures.map(a=>a.type).sort(),['dom','viewport']);
  evidence.rapidCaptures=true;evidence.passed=true;
 } else {
 const before = await state(); const sealed = before.status.activeBatchId;
 await api(`/batches/${sealed}`, {title:`zuruf-test-wiring-${run}`,note:process.argv.includes('--completion') ? 'Transport test only. Do not change any project files. Use the private Zuruf MCP credential path supplied below without printing the secret. Call get_batch_manifest, list_annotations, get_annotation_asset for one asset, and report_progress with message MCP_PROOF_OK. Then reply MCP_PROOF_OK. Read-only task except reporting progress.' : 'Transport test only. Reply exactly OK. Do not use tools or change any files.'},'PATCH');
 const readyBefore = new Set(before.segments.filter(s=>s.state==='ready'||s.state==='empty').map(s=>s.id));
 const start=performance.now(); await page.keyboard.press('Shift+N');
 await until(async()=> (await state()).status.activeBatchId!==sealed);
 evidence.timings.newDraftMs=performance.now()-start;
 // Continuous finalization must retain the sealed epoch even after new captures.
 await page.keyboard.press('w');
 await until(async()=> (await state()).segments.some(x=>x.batchId===sealed && !readyBefore.has(x.id) && ['ready','empty'].includes(x.state)),60000);
 const task=await until(async()=> (await state()).tasks.find(t=>t.batchId===sealed && t.threadId),60000);
 evidence.threads.push({id:task.threadId,title:task.title,url:task.threadUrl});
 if(process.argv.includes('--completion')) {
   const next = (await state()).status.activeBatchId;
   await until(async()=> (await state()).annotations.filter(a=>a.batchId===next).every(a=>a.assets.some(i=>i.kind==='screenshot_marked')));
   await api(`/batches/${next}`,{title:`zuruf-test-concurrent-${run}`,note:'Do not modify files. Run sleep 15 once, then reply CONCURRENT_OK.'},'PATCH');
   const second = await api(`/batches/${next}/dispatch`,{});
   const secondTask = await until(async()=> (await state()).tasks.find(t=>t.id===second.task.id&&t.threadId),60000);
   evidence.threads.push({id:secondTask.threadId,title:secondTask.title,url:secondTask.threadUrl});
   const concurrent = (await state()).tasks.filter(t=>[task.id,secondTask.id].includes(t.id));
   evidence.concurrentStatuses = concurrent.map(t=>t.status);
   assert(concurrent.every(t=>t.status==='running'), 'Both tasks must actually be running');
   const proof = await until(async()=> { const taskState=(await state()).tasks.find(t=>t.id===task.id); if(taskState.pendingRequest) throw Error('Manual T3 approval required; only the user may approve.'); return (await api(`/tasks/${task.id}/events`,undefined,'GET')).find(e=>e.text==='MCP_PROOF_OK'); },120000);
   evidence.mcpProgress = proof.text;
 }

 await until(async()=> (await state()).tasks.find(t=>t.id===task.id && ['running','completed','needs_review','needs_approval'].includes(t.status)),60000);
 const flight=page.evaluate(()=>new Promise(r=>setTimeout(()=>{document.body.dataset.late='yes';r(true)},900)));
 await sleep(100);const takeover=performance.now();await api(`/sessions/${session.id}/control`,{action:'takeover'});evidence.timings.takeoverMs=performance.now()-takeover;
 await flight; await assert.rejects(()=>page.evaluate(()=>1));
 evidence.sealedSegments=(await state()).segments.filter(x=>x.batchId===sealed).length;
 const outcome=await until(async()=> (await state()).tasks.find(t=>t.id===task.id && ['completed','needs_review','needs_approval','failed'].includes(t.status)),60000);
 evidence.taskStatus=outcome.status;
 if(process.argv.includes('--completion')) {
 await api(`/tasks/${task.id}/follow-up`,{text:'Do not change files. Inspect the attached screenshot and reply FOLLOWUP_OK.',annotationIds:before.annotations.filter(a=>a.batchId===sealed).slice(0,1).map(a=>a.id)});
 const follow = await until(async()=> (await state()).tasks.find(t=>t.id===task.id && t.lastMessage?.includes('FOLLOWUP_OK')),120000);
 evidence.followUp = !!follow;
 }
 if(!['completed','failed'].includes(outcome.status)) await api(`/tasks/${task.id}/cancel`,{});
 evidence.passed=true;
 }
} catch(error) { evidence.error=String(error); throw error; }
finally {
 await browser?.close().catch(()=>{});
 if(token) {
  const snapshot=await state().catch(()=>null);
  for(const task of snapshot?.tasks ?? []) if(task.threadId && !['completed','failed','cancelled'].includes(task.status)) await api(`/tasks/${task.id}/cancel`,{}).catch(()=>{});
  await api('/shutdown',{}).catch(()=>{});
 }
 if(daemon) { await Promise.race([new Promise(r=>daemon.once('exit',r)),sleep(10000)]); if(daemon.exitCode===null) daemon.kill('SIGTERM'); }
 await new Promise(r=>server.close(r));
 await writeFile(resolve(out,`evidence-${run}.json`),JSON.stringify(evidence,null,2));
 console.log(JSON.stringify(evidence,null,2));
}
