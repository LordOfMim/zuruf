import {execFileSync} from 'node:child_process';
const origin=process.env.T3_URL??'http://127.0.0.1:3773';
const auth=JSON.parse(execFileSync('t3',['auth','session','issue','--scope','orchestration:read','--scope','settings:write','--ttl','5m','--label','zuruf-feasibility-ws','--json'],{encoding:'utf8'}));
const token=auth.token??auth.accessToken??auth.access_token;
if(!token) throw new Error(`Auth fields: ${Object.keys(auth).join(',')} (values hidden)`);
try {
const headers={Authorization:`Bearer ${token}`,'x-t3-orchestration-protocol':'2'};
for(const path of ['/api/projects','/api/orchestration/threads/mcp%3Aec5a0341-c7ed-432c-b1b4-53bedf73b46f']) {
 const r=await fetch(origin+path,{headers}); const body=await r.json();
 console.log(JSON.stringify({path,status:r.status,data:path==='/api/projects'?{projects:body.projects.filter(p=>p.workspaceRoot==='/home/mika/Projects/Humaify/zuruf/main')}: {keys:Object.keys(body)}}));
}
const r=await fetch(origin+'/api/auth/websocket-ticket/rpc',{method:'POST',headers});
const ticket=await r.json();
if(!r.ok) throw new Error(`Ticket HTTP ${r.status}`);
const ws=new WebSocket(origin.replace('http','ws')+`/ws?orchestrationProtocol=2&wsTicket=${encodeURIComponent(ticket.ticket)}`);
await new Promise((resolve,reject)=>{
 const timer=setTimeout(()=>{ws.close();reject(new Error('WS timeout'));},12000);
 ws.onopen=()=>ws.send(JSON.stringify({_tag:'Request',id:'1',tag:process.env.RPC_METHOD??'server.getConfig',payload:process.env.RPC_METHOD?{threadId:'mcp:ec5a0341-c7ed-432c-b1b4-53bedf73b46f'}:{},headers:[]}));
 ws.onerror=()=>{clearTimeout(timer);reject(new Error('WS connection failed'));};
 ws.onmessage=event=>{
  const value=JSON.parse(event.data);
  // Config can contain unrelated settings; log only advertised provider metadata.
  const text=JSON.stringify(value);
  const visit=v=>{
   if(!v||typeof v!=='object')return;
   if(Array.isArray(v.providers)) console.log(JSON.stringify({providers:v.providers.map(p=>({instanceId:p.instanceId,driverKind:p.driverKind,enabled:p.enabled,models:p.models,status:p.status,capabilities:p.capabilities}))}));
   for(const [k,child] of Object.entries(v)) if(!/secret|token|key/i.test(k)) visit(child);
  };
  visit(value);
  console.log(JSON.stringify({frameTag:value._tag,keys:Object.keys(value),bytes:text.length}));
  ws.close();clearTimeout(timer);resolve();
 };
});
} finally {
 if(auth.sessionId) execFileSync('t3',['auth','session','revoke',auth.sessionId],{stdio:'ignore'});
}
