// Real SDK transport against the executable fake daemon. Never prints credentials.
import assert from 'node:assert/strict';
import {spawn} from 'node:child_process';
import {mkdtemp,readFile,rm} from 'node:fs/promises';
import {once} from 'node:events';
import {Client} from '@modelcontextprotocol/sdk/client/index.js';
import {StreamableHTTPClientTransport} from '@modelcontextprotocol/sdk/client/streamableHttp.js';
const dir=await mkdtemp(new URL('./.run-',import.meta.url).pathname);
const port=48673, base=`http://127.0.0.1:${port}`;
const daemon=spawn('../../target/debug/zuruf',['daemon','--seed-demo'],{cwd:new URL('.',import.meta.url),env:{...process.env,ZURUF_FAKE_ADAPTERS:'1',ZURUF_PORT:String(port),ZURUF_DATA_DIR:`${dir}/data`,ZURUF_STATE_DIR:`${dir}/state`,ZURUF_CONFIG_DIR:`${dir}/config`},stdio:'ignore'});
let client;
try {
 let ui;
 for(let n=0;n<100;n++){try{ui=await readFile(`${dir}/state/token`,'utf8');await fetch(`${base}/api/v1/state`,{headers:{Authorization:`Bearer ${ui}`}});break;}catch{await new Promise(r=>setTimeout(r,100));}}
 assert.ok(ui,'daemon started');
 async function api(path,body){const r=await fetch(`${base}/api/v1/${path}`,{method:body===undefined?'GET':'POST',headers:{Authorization:`Bearer ${ui}`,Origin:base,'Content-Type':'application/json'},body:body===undefined?undefined:JSON.stringify(body)});assert.equal(r.status,200,`API ${path}`);return r.json();}
 let state=await api('state');const sid=state.status.activeSessionId;
 await api(`sessions/${sid}/start`,{});
 await api('actions/capture.viewport',{});
 await new Promise(r=>setTimeout(r,300));
 const dispatched=await api(`batches/${state.status.activeBatchId}/dispatch`,{});
 state=await api('state');
 const scope={projectId:dispatched.task.projectId,taskId:dispatched.task.id,sessionId:sid};
 const credentials=await api('mcp/credentials',{scope});
 const headers={Authorization:`Bearer ${credentials.token}`,Origin:base};
 for(const [extra,status] of [[{Authorization:''},401],[{Origin:''},403],[{Origin:'https://evil.invalid'},403],[{Authorization:`Bearer ${ui}`},401]]){
  const r=await fetch(`${base}/mcp`,{method:'POST',headers:{...headers,...extra,'Content-Type':'application/json'},body:'{}'});assert.equal(r.status,status);
 }
 client=new Client({name:'zuruf-verification',version:'1.0.0'});
 await client.connect(new StreamableHTTPClientTransport(new URL(credentials.url),{requestInit:{headers}}));
 const listed=await client.listTools();assert.equal(listed.tools.length,15);
 async function tool(name,args={},failure=false){const r=await client.callTool({name,arguments:args});assert.equal(r.isError??false,failure,name);return r;}
 for(const name of ['list_projects','list_sessions','get_session_context','list_reviews','get_batch_manifest','list_tasks','list_annotations'])await tool(name);
 const annotations=state.annotations.filter(a=>a.batchId===dispatched.task.batchId);
 const annotation=annotations.find(a=>a.assets.length>0);assert.ok(annotation,'capture asset exists');
 await tool('get_annotation',{annotationId:annotation.id});
 const image=await tool('get_annotation_asset',{assetId:annotation.assets[0].id});assert.equal(image.content[0].type,'image');
 await tool('update_annotation_status',{annotationId:annotation.id,status:'done'});
 await tool('report_progress',{message:'SDK-Verifikation'});
 const secondAnnotation=await api('annotations',{batchId:dispatched.activeBatch.id,type:'text',note:'Task B — isolated context'});
 const second=await api(`batches/${dispatched.activeBatch.id}/dispatch`,{});
 const credentialsB=await api('mcp/credentials',{scope:{...scope,taskId:second.task.id}});
 await tool('get_annotation',{annotationId:secondAnnotation.id},true);
 const bClient=new Client({name:'agent-b',version:'1.0.0'});
 try {
  await bClient.connect(new StreamableHTTPClientTransport(new URL(credentialsB.url),{requestInit:{headers:{...headers,Authorization:`Bearer ${credentialsB.token}`}}}));
  const denied=await bClient.callTool({name:'get_annotation',arguments:{annotationId:annotation.id}});assert.equal(denied.isError,true);
  const tasks=await bClient.callTool({name:'list_tasks',arguments:{}});assert.equal(JSON.parse(tasks.content[0].text)[0].id,second.task.id);
 } finally {await bClient.close();}
 await tool('list_annotations',{batchId:'foreign-task-batch'},true);
 await tool('get_cdp_endpoint',{},true);
 await tool('request_browser_control');await tool('get_cdp_endpoint');
 await tool('renew_browser_control',{},true); // Explicit contract gap; no unsafe release/regrant.
 await tool('release_browser_control');await tool('get_cdp_endpoint',{},true);
 await tool('request_browser_control',{leaseSeconds:1});
 await new Promise(r=>setTimeout(r,1200));
 await tool('get_cdp_endpoint',{},true);
 await api(`sessions/${sid}/control`,{action:'release'});
 await tool('request_browser_control');
 await api(`sessions/${sid}/control`,{action:'takeover'});
 await tool('get_cdp_endpoint',{},true);await tool('request_browser_control',{},true);
 await api('mcp/credentials',{scope,revoke:true});
 const revoked=await fetch(`${base}/mcp`,{method:'POST',headers:{...headers,'Content-Type':'application/json'},body:'{}'});assert.equal(revoked.status,401);
 console.log('PASS: official SDK initialize/list/all 15 tools, image, auth, task scope, revoke, lease/release/takeover; renewal fails closed (contract gap).');
} finally {
 await client?.close().catch(()=>{});
 const stopped=once(daemon,'exit');daemon.kill('SIGTERM');await stopped;
 await rm(dir,{recursive:true,force:true});
}
