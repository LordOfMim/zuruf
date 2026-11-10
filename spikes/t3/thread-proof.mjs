import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { performance } from 'node:perf_hooks';
const exec = promisify(execFile);
const project = '/home/mika/Projects/Humaify/zuruf/main';
async function call(args) {
 const before = performance.now();
 const {stdout,stderr} = await exec('t3',args,{cwd:project,timeout:60000,maxBuffer:4*1024*1024});
 console.log(JSON.stringify({command:['t3',...args],ms:Math.round(performance.now()-before),stdout,stderr}));
 return JSON.parse(stdout);
}
const resume = process.argv[2];
// No implicit polling loop: rerun with the printed id until terminal. Never recreate on timeout.
const created = resume ? {threadId:resume} : await call(['thread','start','--title',`zuruf-feasibility-${new Date().toISOString().replaceAll(':','-')}`,'--provider','codex','--model','gpt-6-luna','--json',project,'Reply with the single word OK. Do not modify any files.']);
const id = created.threadId ?? created.id;
if (!id) throw new Error('Missing thread id; do not retry creation blindly');
console.log(JSON.stringify({threadId:id}));
await call(['thread','show','--json','--messages','3',id]);
// Follow-up is explicit to avoid accidental repeat on resume.
if (process.env.SEND_FOLLOWUP === '1') await call(['thread','send','--wait','--json',id,'Reply with the single word OK. Do not modify any files.']);
