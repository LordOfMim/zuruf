import {execFileSync} from 'node:child_process';
import {writeFile} from 'node:fs/promises';
import {fileURLToPath} from 'node:url';
import assert from 'node:assert/strict';
const run=(...args:string[])=>execFileSync('hyprctl',args,{encoding:'utf8'}).trim();
const keys='SUPER + SHIFT + CTRL + ALT + F24';
const before=JSON.parse(run('binds','-j'));
assert(!before.some((b:any)=>b.key==='F24' && b.modmask===77),'reserved test key already bound');
const result:Record<string,unknown>={legacy:run('keyword','bind','SUPER SHIFT CTRL ALT,F24,exec,/usr/bin/true')};
try {
  result.register=run('eval',`hl.bind("${keys}", hl.dsp.exec_cmd("/usr/bin/true"), {description="Zuruf Phase-0 temporary proof"})`);
  result.found=JSON.parse(run('binds','-j')).filter((b:any)=>b.description==='Zuruf Phase-0 temporary proof');
  assert((result.found as any[]).length===1);
} finally {result.unregister=run('eval',`hl.unbind("${keys}")`);}
result.remaining=JSON.parse(run('binds','-j')).filter((b:any)=>b.description==='Zuruf Phase-0 temporary proof').length;
assert.equal(result.remaining,0);
console.log(JSON.stringify(result,null,2));
await writeFile(fileURLToPath(new URL('../../.spike/browser/hyprland-results.json',import.meta.url)),JSON.stringify(result,null,2));
