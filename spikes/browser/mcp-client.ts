import {spawn} from 'node:child_process';
import {createInterface} from 'node:readline';
import {resolve,dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
export async function mcpProof(endpoint:string,token:string) {
  const child=spawn(process.execPath,[resolve(dirname(fileURLToPath(import.meta.url)),'node_modules/@playwright/mcp/cli.js'),'--cdp-endpoint',endpoint,'--cdp-header',`Authorization: Bearer ${token}`,'--timeout-action','1500','--timeout-settle','0'],{cwd:resolve(dirname(fileURLToPath(import.meta.url)),'../../.spike/browser'),stdio:['pipe','pipe','pipe']});
  const waiting=new Map<number,{resolve:(value:any)=>void,reject:(reason:Error)=>void}>();
  let next=0;
  const stderr:string[]=[];
  child.stderr.on('data',data=>stderr.push(data.toString()));
  const lines=createInterface({input:child.stdout});
  lines.on('line',line=>{
    try {const message=JSON.parse(line); const entry=waiting.get(message.id); if(entry) {waiting.delete(message.id);message.error?entry.reject(new Error(JSON.stringify(message.error))):entry.resolve(message.result);}} catch {/* non-JSON diagnostic */}
  });
  const call=(method:string,params:unknown)=>new Promise<any>((resolve,reject)=>{
    const id=++next;waiting.set(id,{resolve,reject});
    child.stdin.write(JSON.stringify({jsonrpc:'2.0',id,method,params})+'\n');
    setTimeout(()=>{if(waiting.delete(id)) reject(new Error(`MCP timeout ${method}: ${stderr.join('')}`));},8000).unref();
  });
  try {
    const initialized=await call('initialize',{protocolVersion:'2024-11-05',capabilities:{},clientInfo:{name:'zuruf-spike',version:'0.0.1'}});
    child.stdin.write(JSON.stringify({jsonrpc:'2.0',method:'notifications/initialized'})+'\n');
    const tools=await call('tools/list',{});
    const evaluate=await call('tools/call',{name:'browser_evaluate',arguments:{function:'() => { const button = document.querySelector("[data-testid=counter]"); button.click(); return { title: document.title, count: button.dataset.count }; }'}});
    return {server:initialized.serverInfo,toolNames:tools.tools.map((tool:any)=>tool.name),evaluate};
  } finally {child.kill('SIGTERM');await new Promise<void>(resolve=>{if(child.exitCode!==null)resolve();else child.once('exit',()=>resolve());});lines.close();}
}
