import { chromium, type Browser, type Page } from 'playwright-core';
import { createServer } from 'node:http';
import { WebSocketServer, WebSocket } from 'ws';
import { readFile, writeFile, mkdir, cp } from 'node:fs/promises';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn, execFileSync, type ChildProcess } from 'node:child_process';
import { once } from 'node:events';
import assert from 'node:assert/strict';
import { PNG } from 'pngjs';
import { mcpProof } from './mcp-client.ts';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '../..');
const artifacts = resolve(repo, '.spike/browser');
await mkdir(artifacts, { recursive: true, mode: 0o700 });
const results: Record<string, unknown> = { timestamp: new Date().toISOString() };
const events: any[] = [];
let owner: string = 'agent:spike';
let leaseUntil = Date.now() + 120_000;
let upstream = '';
let rejectedCommands=0;
const secret = crypto.randomUUID();
const connections = new Set<WebSocket>();
const server = createServer(async (request, response) => {
  const path = request.url?.split('?')[0];
  response.setHeader('Content-Type', 'text/html; charset=utf-8');
  if (path === '/login') { response.setHeader('Set-Cookie','zuruf_fixture=synthetic; Path=/; Max-Age=3600; SameSite=Lax'); }
  if (path === '/ui') return response.end('<!doctype html><title>Zuruf Companion</title><h1>Lokales Companion-UI</h1>');
  try { response.end(await readFile(resolve(here, path === '/frame.html' ? 'fixture/frame.html' : 'fixture/index.html'))); }
  catch { response.writeHead(404).end(); }
});
const wsServer = new WebSocketServer({ noServer: true });
server.on('upgrade', (request, socket, head) => {
  if (request.url === `/extension?token=${secret}` && request.headers.origin?.startsWith('chrome-extension://')) {
    wsServer.handleUpgrade(request, socket, head, client => {
      connections.add(client);
      client.on('message', data => events.push(JSON.parse(data.toString())));
    });
    return;
  }
  if (request.url !== '/agent' || request.headers.authorization !== `Bearer ${secret}`) {
    socket.end('HTTP/1.1 401 Unauthorized\r\n\r\n'); return;
  }
  wsServer.handleUpgrade(request, socket, head, client => {
    connections.add(client);
    const remote = new WebSocket(upstream);
    connections.add(remote);
    const pending: string[] = [];
    remote.on('open', () => pending.splice(0).forEach(message => remote.send(message)));
    remote.on('message', data => { if (client.readyState === WebSocket.OPEN) client.send(data.toString()); });
    client.on('message', data => {
      const message = JSON.parse(data.toString());
      // Fail closed for ALL CDP commands: evaluate can mutate just like Input.*.
      if (owner !== 'agent:spike' || Date.now() > leaseUntil) {
        rejectedCommands++;
        client.send(JSON.stringify({ id: message.id, sessionId: message.sessionId, error: { code: -32000, message: 'Zuruf: human takeover / lease expired' } }));
      } else if (remote.readyState === WebSocket.OPEN) remote.send(data.toString());
      else pending.push(data.toString());
    });
    client.on('close', () => remote.close());
    remote.on('error', () => client.close());
  });
});
server.listen(0, '127.0.0.1');
await once(server, 'listening');
const address = server.address();
assert(address && typeof address === 'object');
const port = address.port;
const base = `http://127.0.0.1:${port}`;
const extension = resolve(artifacts, 'extension');
await cp(resolve(here, 'extension'), extension, { recursive: true });
for (const name of ['worker.js', 'panel.html']) {
  const path = resolve(extension, name);
  await writeFile(path, (await readFile(path, 'utf8')).replaceAll('__PORT__', String(port)).replaceAll('__TOKEN__',secret));
}
let processHandle: ChildProcess | undefined;
let browser: Browser | undefined;
let unit = '';
async function stop() {
  if (browser) {
    const samples=(results.resources ??= []) as unknown[];
    try {samples.push(execFileSync('systemctl',['--user','show',unit,'-p','MemoryPeak','-p','MemorySwapPeak'],{encoding:'utf8'}).trim());} catch {}
    try { const cdp = await browser.newBrowserCDPSession(); await cdp.send('Browser.close'); } catch { /* expected close */ }
    browser = undefined;
  }
  if (processHandle && processHandle.exitCode === null) {
    try { await Promise.race([once(processHandle, 'exit'), new Promise((_, reject) => setTimeout(() => reject(new Error('close timeout')), 3000))]); }
    catch { execFileSync('systemctl', ['--user', 'stop', unit]); }
  }
  processHandle = undefined;
}
async function launch(profile: string, extra: string[] = [], pipe = false) {
  assert(!processHandle, 'one browser at a time');
  unit = `zuruf-browser-${process.pid}-${Date.now()}.service`;
  const profileDir = resolve(artifacts, `run-${process.pid}`, profile);
  await mkdir(profileDir, { recursive: true, mode: 0o700 });
  const args = ['--no-first-run', '--no-default-browser-check', '--disable-session-crashed-bubble', '--password-store=basic', `--user-data-dir=${profileDir}`, '--ozone-platform=wayland', ...extra];
  if (!pipe) args.push('--remote-debugging-address=127.0.0.1', '--remote-debugging-port=0');
  if (process.env.HEADLESS === '1') args.push('--headless=new');
  args.push(base);
  const log: string[] = [];
  processHandle = spawn('systemd-run', ['--user', '--wait', '--collect', '--pipe', `--unit=${unit}`, '-p', 'MemoryMax=3G', '-p', 'MemorySwapMax=1G', '--', '/usr/bin/chromium', ...args], { stdio: ['ignore', 'pipe', 'pipe'] });
  processHandle.stderr?.on('data', data => log.push(data.toString()));
  processHandle.stdout?.on('data', data => log.push(data.toString()));
  let endpoint = '';
  for (let i = 0; i < 150; i++) {
    endpoint = log.join('').match(/DevTools listening on (ws:\/\/[^\s]+)/)?.[1] || '';
    if (endpoint) break;
    if (processHandle.exitCode !== null) throw new Error(log.join(''));
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  if (!endpoint) throw new Error(`No debugging endpoint: ${log.join('')}`);
  upstream = endpoint;
  await writeFile(resolve(artifacts, `${profile}-launch.log`), log.join(''));
  browser = await chromium.connectOverCDP(endpoint);
  browser.contexts()[0].on('console', message => console.log('browser-console',message.text()));
  const page = browser.contexts()[0].pages()[0];
  await page.waitForLoadState();
  return { browser, page };
}
async function waitEvent(type: string, start: number) {
  for (let i = 0; i < 50; i++) {
    const event = events.slice(start).find(event => event.type === type);
    if (event) return event;
    await new Promise(resolve => setTimeout(resolve, 50));
  }
  throw new Error(`missing event ${type}`);
}
async function targetId(browser: Browser, page: Page) {
  const session = await browser.contexts()[0].newCDPSession(page);
  const result = await session.send('Target.getTargetInfo');
  await session.detach();
  return result.targetInfo.targetId;
}
try {
  let running = await launch('persistent', [`--load-extension=${extension}`]);
  const { page } = running;
  results.initialTargets=(await (await running.browser.newBrowserCDPSession()).send('Target.getTargets')).targetInfos;
  results.workerState=await running.browser.contexts()[0].serviceWorkers()[0]?.evaluate('({state:socket.readyState})');
  await waitEvent('hello', 0);
  results.extension = { loaded: true, worker: events.find(e => e.type === 'hello') };
  const context = running.browser.contexts()[0];
  const worker = context.serviceWorkers()[0] || await context.waitForEvent('serviceworker');
  results.sidePanelWithoutGesture = await worker.evaluate(async () => {
    const chrome = (globalThis as any).chrome;
    const [tab] = await chrome.tabs.query({active:true});
    try { await chrome.sidePanel.open({windowId:tab.windowId}); return 'opened'; } catch (e) { return String(e); }
  });
  const hotkeys: Record<string, unknown> = {};
  for (const selector of ['#text', '#password', '#area', '#editable', '#shadow input']) {
    const start = events.length;
    await page.locator(selector).click();
    await page.keyboard.type('wsd');
    hotkeys[selector] = { value: await page.locator(selector).evaluate((el: any) => el.value ?? el.textContent), actions: events.slice(start).filter(e => e.type === 'capture').length };
    assert.equal((hotkeys[selector] as any).actions, 0);
    assert.match((hotkeys[selector] as any).value, /wsd/);
    if(selector==='#password') (hotkeys[selector] as any).value='[redacted; endsWith wsd verified]';
  }
  await page.locator('.monaco-editor').click();
  let start = events.length;
  await page.keyboard.type('wsd');
  assert.equal(events.slice(start).filter(e => e.type === 'capture').length, 0);
  hotkeys.editorGuard = true;
  const frame = page.frameLocator('#frame');
  await frame.locator('input').click(); await page.keyboard.type('wsd');
  assert.equal(await frame.locator('input').inputValue(), 'wsd');
  hotkeys.frameInput = true;
  await page.locator('h1').click();
  await page.mouse.move(450, 380);
  start = events.length;
  await page.keyboard.press('w');
  await waitEvent('capture', start);
  hotkeys.bodyCaptureDespitePageStopPropagation = true;
  await page.locator('#select').focus(); start=events.length; await page.keyboard.type('wsd');
  assert.equal(events.slice(start).filter(e=>e.type==='capture').length,0); hotkeys.selectGuard=true;
  await worker.evaluate(async () => {const chrome=(globalThis as any).chrome;const [tab]=await chrome.tabs.query({active:true});await chrome.tabs.sendMessage(tab.id,{type:'configure',review:false});});
  await page.locator('h1').click(); start=events.length;await page.keyboard.press('w');await new Promise(resolve=>setTimeout(resolve,60));
  assert.equal(events.slice(start).filter(e=>e.type==='capture').length,0); hotkeys.reviewOff=true;
  await worker.evaluate(async () => {const chrome=(globalThis as any).chrome;const [tab]=await chrome.tabs.query({active:true});await chrome.tabs.sendMessage(tab.id,{type:'configure',review:true});});
  results.hotkeys = hotkeys;
  const domResults = [];
  for (const [scope, locator] of [[page, page.getByTestId('counter')], [page, page.getByTestId('shadow-button')]] as const) {
    await locator.hover();
    start = events.length;
    await page.keyboard.press('d');
    const event = await waitEvent('capture', start);
    const dom = event.dom;
    assert(dom && !JSON.stringify(dom).includes('fixture-secret'));
    const resolved: any[] = [];
    for (const candidate of dom.candidates) {
      let match;
      if (candidate.kind === 'testid') match = scope.getByTestId(candidate.value);
      if (candidate.kind === 'role') match = scope.getByRole(candidate.value, {name:candidate.name,exact:true});
      if (candidate.kind === 'text') match = scope.getByText(candidate.value, {exact:true});
      if (candidate.kind === 'css') match = scope.locator(dom.shadowPath.length ? dom.shadowPath.join(' ') + ' ' + candidate.value : candidate.value);
      if (candidate.kind === 'xpath' && !dom.shadowPath.length) match = scope.locator('xpath='+candidate.value);
      resolved.push({ ...candidate, count: match ? await match.count() : null });
    }
    assert.equal(resolved[0].count, 1);
    domResults.push({...dom,candidates:resolved});
  }
  await frame.getByTestId('frame-button').click();
  await frame.getByTestId('frame-button').hover();
  start=events.length; await page.keyboard.press('d');
  const frameDom = (await waitEvent('capture',start)).dom;
  assert.equal(await frame.getByTestId(frameDom.candidates[0].value).count(),1);
  domResults.push(frameDom);
  await page.locator('h1').click();await page.locator('#password').hover();
  start=events.length;await page.keyboard.press('d');const passwordDom=(await waitEvent('capture',start)).dom;
  assert(!JSON.stringify(passwordDom).includes('fixture-password'));assert(!('value' in passwordDom.attributes));
  results.passwordMetadataRedacted=true;
  results.dom = domResults;
  // Same target, independent Playwright client through authenticated arbitration proxy.
  const id = await targetId(running.browser, page);
  const agent = await chromium.connectOverCDP(`ws://127.0.0.1:${port}/agent`, {headers:{Authorization:`Bearer ${secret}`}});
  const agentPage = agent.contexts()[0].pages().find(p=>p.url()===base+'/')!;
  assert.equal(await targetId(agent,agentPage), id);
  await agentPage.getByTestId('counter').click();
  assert.equal(await page.getByTestId('counter').getAttribute('data-count'),'1');
  owner='human';
  let rejection = '';
  try { await agentPage.evaluate(() => document.querySelector('button')?.click()); } catch (e) { rejection=String(e); }
  assert(rejection); assert(rejectedCommands>0);
  assert.equal(await page.getByTestId('counter').getAttribute('data-count'),'1');
  owner='agent:spike'; leaseUntil=Date.now()+60_000;
  await agentPage.getByTestId('counter').click();
  assert.equal(await page.getByTestId('counter').getAttribute('data-count'),'2');
  results.shared = {sameTarget:true,targetId:id,takeoverRejection:rejection,rejectedCommands,releasedClick:true};
  results.mcp=await mcpProof(`ws://127.0.0.1:${port}/agent`,secret);
  assert.equal(await page.getByTestId('counter').getAttribute('data-count'),'3');
  // Open panel with genuine input routed through Chromium; localhost iframe must load.
  await page.locator('h1').click(); start=events.length; await page.keyboard.press('p');
  results.panelGesture = await waitEvent('panel-result',start);
  results.panelTargets = (await (await running.browser.newBrowserCDPSession()).send('Target.getTargets')).targetInfos.filter(t=>/panel.html|\/ui/.test(t.url));
  const panel = context.pages().find(p=>p.url().includes('panel.html'));
  results.panelIframeLoaded = panel ? await panel.frameLocator('iframe').getByRole('heading').textContent() : 'panel not exposed as Playwright Page';
  await worker.evaluate(async () => {
    const chrome=(globalThis as any).chrome;
    const [tab]=await chrome.tabs.query({active:true});
    await chrome.sidePanel.close({windowId:tab.windowId});
  });
  const screenshots = [];
  const cdp = await context.newCDPSession(page);
  for (const dpr of [1,2]) {
    await cdp.send('Emulation.setDeviceMetricsOverride',{width:900,height:700,deviceScaleFactor:dpr,mobile:false});
    for (const zoom of [1,1.25]) {
      await worker.evaluate(async zoom => {
        const chrome=(globalThis as any).chrome;
        const [tab]=await chrome.tabs.query({active:true}); await chrome.tabs.setZoom(tab.id,zoom);
      }, zoom);
      await page.evaluate(()=>scrollTo(0,0));
      await page.locator('h1').click();
      await page.mouse.move(450,380);
      start=events.length;
      const began=performance.now(); await page.keyboard.press('w');
      const event=await waitEvent('capture',start);
      await worker.evaluate(async () => {
        const chrome=(globalThis as any).chrome;
        const [tab]=await chrome.tabs.query({active:true}); await chrome.tabs.sendMessage(tab.id,{type:'hide'});
      });
      const raw=await cdp.send('Page.captureScreenshot',{format:'png'});
      const buffer=Buffer.from(raw.data,'base64');
      const path=resolve(artifacts,`viewport-${dpr}-${zoom}.png`);
      await writeFile(path,buffer);
      const latency=performance.now()-began;
      const png=PNG.sync.read(buffer);
      const viewport=await page.evaluate(()=>({width:innerWidth,height:innerHeight,dpr:devicePixelRatio}));
      const sx=png.width/viewport.width,sy=png.height/viewport.height;
      const cx=event.cursor.x*sx,cy=event.cursor.y*sy;
      for(let y=Math.max(0,Math.floor(cy-22*sy));y<Math.min(png.height,cy+22*sy);y++) for(let x=Math.max(0,Math.floor(cx-22*sx));x<Math.min(png.width,cx+22*sx);x++) {
        const radius=Math.hypot((x-cx)/sx,(y-cy)/sy);
        if(radius>17 && radius<20) {const index=(png.width*y+x)*4;png.data.set([240,60,20,255],index);}
      }
      await writeFile(resolve(artifacts,`cursor-${dpr}-${zoom}.png`),PNG.sync.write(png));
      const compositeMs=Math.round(performance.now()-began);
      const rect=await page.locator('#tile').boundingBox(); assert(rect);
      const selection={x:Math.ceil(rect.x+5),y:Math.ceil(rect.y+5),width:80,height:40};
      await page.keyboard.press('s');
      await page.mouse.move(selection.x,selection.y); await page.mouse.down();
      await page.mouse.move(selection.x+selection.width,selection.y+selection.height); start=events.length; await page.mouse.up();
      const region=await waitEvent('capture',start);
      assert.deepEqual(region.rect,selection);
      const metrics=await cdp.send('Page.getLayoutMetrics');
      // CDP clip is in CSS page coordinates; screenshot device pixels scale with effective DPR.
      const shot=await cdp.send('Page.captureScreenshot',{format:'png',clip:{x:(selection.x+metrics.cssVisualViewport.pageX)*zoom,y:(selection.y+metrics.cssVisualViewport.pageY)*zoom,width:selection.width*zoom,height:selection.height*zoom,scale:1}});
      const regionBuffer=Buffer.from(shot.data,'base64');
      await writeFile(resolve(artifacts,`region-${dpr}-${zoom}.png`),regionBuffer);
      const pixels=PNG.sync.read(regionBuffer);
      assert.equal(pixels.width,Math.round(selection.width*viewport.dpr));
      assert.equal(pixels.height,Math.round(selection.height*viewport.dpr));
      let wrong=0;
      for(let i=0;i<pixels.data.length;i+=4) if(pixels.data[i]!==24||pixels.data[i+1]!==120||pixels.data[i+2]!==200) wrong++;
      assert.equal(wrong,0);
      screenshots.push({dpr,zoom,viewport,output:[png.width,png.height],region:[pixels.width,pixels.height],allRegionPixelsMatch:true,keyToRawFileMs:Math.round(latency),keyToCompositeMs:compositeMs});
    }
  }
  results.screenshots=screenshots;
  await page.getByTestId('deep').scrollIntoViewIfNeeded(); await page.getByTestId('deep').hover();
  start=events.length; await page.keyboard.press('d'); results.scrolledDOM=(await waitEvent('capture',start)).dom;
  // Cookie persistence and context isolation; fixture-only synthetic cookie.
  await page.goto(base+'/login');
  assert.equal((await context.cookies(base)).find(c=>c.name==='zuruf_fixture')?.value,'synthetic');
  const isolated=await running.browser.newContext();
  assert.equal((await isolated.cookies(base)).length,0); await isolated.close();
  results.isolation={separateContext:true};
  // Hyprland operations only against this process's own window.
  const clients=JSON.parse(execFileSync('hyprctl',['clients','-j'],{encoding:'utf8'}));
  const client=clients.find((c:any)=>c.title.includes('Zuruf Browser-Spike'));
  if(client) {
    const address=`address:${client.address}`;
    results.hyprland={xwayland:client.xwayland,commands: [
      execFileSync('hyprctl',['dispatch',`hl.dsp.window.float({action="set",window="${address}"})`],{encoding:'utf8'}),
      execFileSync('hyprctl',['dispatch',`hl.dsp.window.resize({x=900,y=700,relative=false,window="${address}"})`],{encoding:'utf8'}),
      execFileSync('hyprctl',['dispatch',`hl.dsp.window.move({x=50,y=60,relative=false,window="${address}"})`],{encoding:'utf8'})]};
  }
  // Close Chromium gracefully through its owning CDP client, not through an agent.
  await new Promise(resolve=>setTimeout(resolve,1200));
  await stop();
  running=await launch('persistent',[`--load-extension=${extension}`]);
  assert.equal((await running.browser.contexts()[0].cookies(base)).find(c=>c.name==='zuruf_fixture')?.value,'synthetic');
  results.persistence=true;
  await stop();
  running=await launch('isolated-profile',[`--load-extension=${extension}`,`--app=${base}/ui`]);
  assert.equal((await running.browser.contexts()[0].cookies(base)).length,0);
  (results.isolation as any).separateProfile=true;
  results.appWindow=(await running.browser.newBrowserCDPSession()).send('Target.getTargets').then(r=>r.targetInfos.map(t=>({type:t.type,url:t.url})));
  results.appWindow=await results.appWindow;
  await running.page.getByRole('heading').click();start=events.length;await running.page.keyboard.press('p');
  results.appSidePanelGesture=await waitEvent('panel-result',start);
  await new Promise(resolve=>setTimeout(resolve,150));
  results.appPanelTargets=(await (await running.browser.newBrowserCDPSession()).send('Target.getTargets')).targetInfos.filter(t=>t.url.includes('panel.html') || t.type==='iframe').map(t=>({type:t.type,url:t.url}));
  await stop();
} catch(error) {
  results.error=String(error); console.error(error); process.exitCode=1;
} finally {
  await stop();
  for(const connection of connections) connection.terminate();
  wsServer.close(); server.close();
  await writeFile(resolve(artifacts,'results.json'),JSON.stringify(results,null,2));
  console.log(JSON.stringify(results,null,2));
}
