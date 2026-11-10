import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { resolve } from 'node:path';
import assert from 'node:assert/strict';
const exec = promisify(execFile);
const root = resolve(import.meta.dirname, '../..');
const env = {...process.env,ZURUF_PORT:'47638',ZURUF_DATA_DIR:`${process.env.HOME}/.local/share/zuruf/e2e-app`,ZURUF_STATE_DIR:`${process.env.HOME}/.local/state/zuruf/e2e-app`,ZURUF_CONFIG_DIR:`${process.env.HOME}/.local/share/zuruf/e2e-app-config`};
delete env.ZURUF_FAKE_ADAPTERS;
delete env.T3_URL;
const cli = (...args)=>exec(resolve(root,'target/debug/zuruf'),args,{env});
try {
 await cli('open','--view','sessions');
 const {stdout}=await cli('status');assert(JSON.parse(stdout).status.protocolVersion===1);
 await exec('systemctl',['--user','is-active','zuruf-app.service']);
 const {stdout:pidText}=await exec('systemctl',['--user','show','zuruf-app.service','--property=MainPID','--value']);
 const pid=Number(pidText.trim());
 let found=false;let owned=[];
 for(let i=0;i<100;i++) {
  const {stdout}=await exec('hyprctl',['clients','-j']);
  owned=JSON.parse(stdout).filter(w=>w.pid===pid).map(w=>({class:w.class,initialClass:w.initialClass}));
  if(owned.some(w=>w.class==='zuruf'||(process.argv.includes('--allow-generated-id')&&w.class==='chrome-127.0.0.1__-Default'))) { found=true;break; }
  await new Promise(r=>setTimeout(r,100));
 }
 assert(found,'App window class: '+JSON.stringify(owned));
 console.log('Real app window and daemon autostart: passed; identity '+JSON.stringify(owned));
} finally { await cli('stop'); }
