import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {mkdtemp,readFile,stat} from 'node:fs/promises';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
const exec=promisify(execFile);
const sleep=ms=>new Promise(resolve=>setTimeout(resolve,ms));
async function call(args) {
 const start=performance.now(); let result;
 try { result={...(await exec('voxtype',args,{timeout:45000})),code:0}; }
 catch(e) {result={stdout:e.stdout,stderr:e.stderr,code:e.code};}
 console.log(JSON.stringify({command:['voxtype',...args],ms:Math.round(performance.now()-start),...result}));
 return result;
}
const status=await call(['status']);
if(status.stdout.trim()!=='idle') throw new Error('Voxtype busy; no recording touched');
const directory=await mkdtemp(join(tmpdir(),'zuruf-voxtype-'));
const path=join(directory,'transcript.txt');
let owned=false;
try {
 const started=await call(['record','start',`--file=${path}`,'--no-auto-submit','--no-smart-auto-submit','--no-osd']);
 if(started.code!==0) throw new Error('Start failed');
 owned=true;
 await sleep(400);
 await call(['status','--format','json','--extended']);
 // A second start tests busy behavior, without changing output destination.
 if(process.argv.includes('--busy-test')) await call(['record','start',`--file=${path}`,'--no-auto-submit','--no-smart-auto-submit','--no-osd']);
 await sleep(1600);
 if(process.argv.includes('--cancel')) {
  await call(['record','cancel']);
 } else {
  await call(['record','stop','--wait','--json','--timeout','30']);
 }
 owned=false;
 await call(['status','--format','json']);
 try { console.log(JSON.stringify({path,size:(await stat(path)).size,text:await readFile(path,'utf8')})); }
 catch(e){ console.log(JSON.stringify({path,fileError:e.code})); }
} finally { if(owned) await call(['record','cancel']); }
