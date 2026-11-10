import {createRequire} from 'node:module';
import {readFile,readdir,writeFile} from 'node:fs/promises';
import {resolve} from 'node:path';
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {createInterface} from 'node:readline';
const require=createRequire(new URL('../../apps/extension/package.json',import.meta.url));
const {chromium}=require('playwright-core');
const {PNG}=require('pngjs');
const endpoint=process.env.ZURUF_ENDPOINT,token=process.env.ZURUF_TOKEN,base=process.env.ZURUF_BASE,out=process.env.ZURUF_OUTPUT;
const headers={Authorization:`Bearer ${token}`};
const evidence={};
let browser;
const wait=ms=>new Promise(r=>setTimeout(r,ms));
async function control(action){const response=await fetch(`${base}/test/${action}`,{headers});assert(response.ok);}
async function records(){return (await readdir(out)).filter(n=>n.endsWith('.json')&&n!=='evidence.json');}
async function capture(page,key,action){const before=new Set(await records());const start=performance.now();await page.keyboard.press(key);if(action)await action();for(let n=0;n<100;n++){for(const name of await records()){if(!before.has(name)){const record=JSON.parse(await readFile(resolve(out,name),'utf8'));record.testKeypressMs=performance.now()-start;return record;}}await wait(30);}throw Error(`capture timeout ${key}`);}
async function mcp(){
 const child=spawn(process.execPath,[resolve(require.resolve('@playwright/mcp/package.json'),'../cli.js'),'--cdp-endpoint',endpoint,'--cdp-header',`Authorization: Bearer ${token}`,'--timeout-action','1500'],{stdio:['pipe','pipe','pipe']});
 const lines=createInterface({input:child.stdout});const pending=new Map();let next=0;let stderr='';child.stderr.on('data',d=>stderr+=d);
 lines.on('line',line=>{try{const message=JSON.parse(line);const p=pending.get(message.id);if(p){pending.delete(message.id);clearTimeout(p.timer);message.error?p.reject(Error(JSON.stringify(message.error))):p.resolve(message.result);}}catch{}});
 const call=(method,params)=>new Promise((resolve,reject)=>{const id=++next;const timer=setTimeout(()=>reject(Error(`MCP timeout: ${stderr}`)),15000);pending.set(id,{resolve,reject,timer});child.stdin.write(JSON.stringify({jsonrpc:'2.0',id,method,params})+'\n');});
 try{await call('initialize',{protocolVersion:'2024-11-05',capabilities:{},clientInfo:{name:'zuruf-integration',version:'1'}});child.stdin.write(JSON.stringify({jsonrpc:'2.0',method:'notifications/initialized'})+'\n');const result=await call('tools/call',{name:'browser_evaluate',arguments:{function:'() => { document.querySelector("[data-testid=counter]").click(); return document.querySelector("[data-testid=counter]").dataset.count; }'}});assert(!result.isError,JSON.stringify(result));return result;}
 finally{for(const p of pending.values())clearTimeout(p.timer);child.kill('SIGTERM');await new Promise(r=>child.exitCode!==null?r():child.once('exit',r));lines.close();}
}
try{
 browser=await chromium.connectOverCDP(endpoint,{headers,timeout:10000});const context=browser.contexts()[0];const page=context.pages().find(p=>p.url().startsWith(base));assert(page);await page.goto(base+'/login',{waitUntil:'domcontentloaded'});await wait(1000);
 evidence.sameTab=page.url();
 for(const selector of ['#text','#password','#area','#editable','#shadow input']){const before=(await records()).length;await page.locator(selector).click();await page.keyboard.type('wsd');assert.match(await page.locator(selector).evaluate(e=>e.value??e.textContent),/wsd/);await wait(60);assert.equal((await records()).length,before);}
 await page.frameLocator('#frame').locator('input').click();await page.keyboard.type('wsd');assert.equal(await page.frameLocator('#frame').locator('input').inputValue(),'wsd');evidence.AT17=true;
 await page.locator('h1').click();await page.mouse.move(450,380);const shot=await capture(page,'w');evidence.viewport=shot;assert(shot.keypressRawMs<1500);
 await page.getByTestId('counter').hover();const dom=await capture(page,'d');assert.equal(dom.dom.locators[0].kind,'testid');assert(!JSON.stringify(dom.dom).includes('fixture-secret'));assert.equal(dom.dom.role,'button');assert.equal(dom.dom.name,'Zähler erhöhen');evidence.dom=dom.dom;
 await page.getByTestId('shadow-button').hover();const shadow=await capture(page,'d');assert(shadow.dom.shadowPath.length>0);assert.equal(shadow.dom.locators[0].value,'shadow-button');
 // Pointer is in frame while keyboard focus remains in top document.
 await page.frameLocator('#frame').getByTestId('frame-button').hover();const frame=await capture(page,'d');assert.equal(frame.dom.locators[0].value,'frame-button');assert(frame.dom.framePath.length>0);evidence.frameRouting=true;
 await page.getByTestId('counter').click();assert.equal(await page.getByTestId('counter').getAttribute('data-count'),'1');
 await control('takeover');let blocked=false;try{await page.evaluate(()=>document.querySelector('button').click());}catch{blocked=true;}assert(blocked);await control('release');await page.getByTestId('counter').click();assert.equal(await page.getByTestId('counter').getAttribute('data-count'),'2');evidence.takeover=true;
 evidence.mcp=await mcp();assert.equal(await page.getByTestId('counter').getAttribute('data-count'),'3');
 const cdp=await context.newCDPSession(page);await assert.rejects(()=>cdp.send('Browser.close'));
 const all=await cdp.send('Target.getTargetInfo');assert(all.targetInfo.url.startsWith(base));
 const workerTargets=await browser.newBrowserCDPSession();const visible=await workerTargets.send('Target.getTargets');assert(visible.targetInfos.every(t=>t.type==='page'&&!t.url.startsWith('chrome-extension:')));evidence.filteredTargets=true;
 // Region at normal zoom/DPR. Full matrix uses daemon-side metrics (see README).
 await page.locator('h1').click();const region=await capture(page,'s',async()=>{await page.mouse.move(420,370);await page.mouse.down();await page.mouse.move(500,410);await page.mouse.up();});
 const png=PNG.sync.read(await readFile(resolve(out,`${region.requestId}-raw.png`)));assert.equal(png.width,80);assert.equal(png.height,40);for(let i=0;i<png.data.length;i+=4)assert.deepEqual([...png.data.subarray(i,i+4)],[24,120,200,255]);evidence.region=true;
 await wait(1200);assert((await context.cookies()).some(c=>c.name==='zuruf_fixture'));
 await browser.close();browser=undefined;await control('restart');
 browser=await chromium.connectOverCDP(endpoint,{headers,timeout:10000});assert((await browser.contexts()[0].cookies()).some(c=>c.name==='zuruf_fixture'));evidence.cookieRestart=true;
 await writeFile(resolve(out,'evidence.json'),JSON.stringify(evidence,null,2));console.log(JSON.stringify(evidence,null,2));
}finally{await browser?.close();}
