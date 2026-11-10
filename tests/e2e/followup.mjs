// Reuses the interrupted synthetic task from a completion run. No new thread.
import {spawn} from 'node:child_process';
import {readFile,writeFile} from 'node:fs/promises';
import {resolve} from 'node:path';
import assert from 'node:assert/strict';
const root=resolve(import.meta.dirname,'../..');
const run=process.env.ZURUF_FOLLOWUP_RUN;
assert(run&&/^\d+$/.test(run),'ZURUF_FOLLOWUP_RUN required');
const directory=`${process.env.HOME}/.local/share/zuruf/e2e/${run}`;
const env={...process.env,ZURUF_PORT:'47639',ZURUF_DATA_DIR:`${directory}/data`,ZURUF_STATE_DIR:`${directory}/state`,ZURUF_CONFIG_DIR:`${directory}/config`,T3_URL:'http://127.0.0.1:3773'};
const token=(await readFile(`${env.ZURUF_STATE_DIR}/token`,'utf8')).trim();
const daemon=spawn(resolve(root,'target/debug/zuruf'),['daemon'],{env,stdio:'ignore'});
const evidence={run};
async function api(path,body){const r=await fetch(`http://127.0.0.1:47639/api/v1${path}`,{method:body===undefined?'GET':'POST',headers:{authorization:`Bearer ${token}`,'content-type':'application/json'},body:body===undefined?undefined:JSON.stringify(body)});assert(r.ok,`HTTP ${r.status}`);return r.json();}
const pause=ms=>new Promise(r=>setTimeout(r,ms));
try {
 let state;
 for(let n=0;n<100;n++){try{state=await api('/state');break;}catch{await pause(100)}}
 const task=state.tasks.find(t=>t.threadId&&t.title.startsWith('zuruf-test-wiring-'));assert(task);
 evidence.threadId=task.threadId;
 // A text follow-up still uploads the task's marked assets through the real adapter.
 await api(`/tasks/${task.id}/follow-up`,{text:'Synthetic transport test only. Do not use tools. Do not change any files. Reply FOLLOWUP_OK after observing the attached screenshots.'});
 for(let n=0;n<300;n++){
  const t=(await api('/state')).tasks.find(t=>t.id===task.id);
  if(t.lastMessage?.includes('FOLLOWUP_OK')){evidence.reply=t.lastMessage;break;}
  if(t.pendingRequest)throw Error('Manual approval required');
  if(['failed','unknown_remote_state'].includes(t.status))throw Error(t.error);
  await pause(100);
 }
 assert(evidence.reply,'Follow-up reply not observed');
 evidence.passed=true;
} catch(e){evidence.error=String(e);process.exitCode=1;}
finally {
 const state=await api('/state').catch(()=>null);
 for(const t of state?.tasks??[])if(t.threadId&&!['completed','failed','cancelled'].includes(t.status))await api(`/tasks/${t.id}/cancel`,{}).catch(()=>{});
 await api('/shutdown',{}).catch(()=>{});
 if(daemon.exitCode===null)await Promise.race([new Promise(r=>daemon.once('exit',r)),pause(5000)]);
 if(daemon.exitCode===null)daemon.kill('SIGTERM');
 await writeFile(resolve(root,'.spike/wiring/followup.json'),JSON.stringify(evidence,null,2));
 console.log(JSON.stringify(evidence,null,2));
}
