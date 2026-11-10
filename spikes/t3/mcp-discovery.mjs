import {execFileSync} from 'node:child_process';
import {randomBytes,createHash} from 'node:crypto';
const previousSessions=new Set(JSON.parse(execFileSync('t3',['auth','session','list','--json'],{encoding:'utf8'})).map(s=>s.sessionId));
try {
const origin=process.env.T3_URL??'http://127.0.0.1:3773';
async function post(path,data,form=false){
 const r=await fetch(origin+path,{method:'POST',headers:{'Content-Type':form?'application/x-www-form-urlencoded':'application/json'},body:form?new URLSearchParams(data):JSON.stringify(data)});
 if(!r.ok) throw new Error(`${path}: HTTP ${r.status} (body hidden for credential safety)`);
 return r.json();
}
const redirect='http://127.0.0.1:39871/callback'; // No listener needed: decision returns redirect JSON.
const client=await post('/oauth/mcp/register',{client_name:'Zuruf feasibility',redirect_uris:[redirect],token_endpoint_auth_method:'none'});
const verifier=randomBytes(32).toString('base64url');
const authorization={response_type:'code',client_id:client.client_id,redirect_uri:redirect,code_challenge:createHash('sha256').update(verifier).digest('base64url'),code_challenge_method:'S256',resource:origin+'/mcp',state:randomBytes(16).toString('base64url')};
const pairing=JSON.parse(execFileSync('t3',['auth','pairing','create','--scope','orchestration:read','--ttl','5m','--label','zuruf-feasibility-mcp','--json'],{encoding:'utf8'}));
const code=pairing.credential??pairing.token??pairing.code??pairing.pairingToken;
if(!code) throw new Error(`Pairing fields ${Object.keys(pairing).join(',')} (values hidden)`);
const decision=await post('/oauth/mcp/decision',{authorization,decision:{_tag:'pairing-code',access:'read-only',code}});
const url=new URL(decision.redirectTo);
if(url.searchParams.get('state')!==authorization.state) throw new Error('OAuth state mismatch');
const auth=await post('/oauth/mcp/token',{grant_type:'authorization_code',client_id:client.client_id,redirect_uri:redirect,code:url.searchParams.get('code'),code_verifier:verifier,resource:origin+'/mcp'},true);
let id=0;
let session;
async function rpc(method,params){
 const r=await fetch(origin+'/mcp',{method:'POST',headers:{Authorization:`Bearer ${auth.access_token}`,'Content-Type':'application/json',Accept:'application/json, text/event-stream','mcp-protocol-version':'2025-06-18',...(session?{'mcp-session-id':session}:{})},body:JSON.stringify({jsonrpc:'2.0',id:++id,method,params})});
 session=r.headers.get("mcp-session-id")??session;
 const raw=await r.text();
 if(!r.ok) throw new Error(`MCP HTTP ${r.status}: ${raw}`);
 const result=raw.startsWith('{')?JSON.parse(raw):JSON.parse(raw.split('\n').find(l=>l.startsWith('data:')).slice(5));
 return result.result;
}
console.log(JSON.stringify({oauth:true,scope:auth.scope,expiresIn:auth.expires_in}));
console.log(JSON.stringify({initialize:await rpc('initialize',{protocolVersion:'2025-06-18',capabilities:{},clientInfo:{name:'zuruf-feasibility',version:'0'}})}));
const tools=await rpc('tools/list',{});
console.log(JSON.stringify({tools:tools.tools.filter(t=>/orchestrator_capabilities|t3_thread_(launch|send|read)|t3_project_list/.test(t.name))}));
console.log(JSON.stringify({capabilities:await rpc('tools/call',{name:'orchestrator_capabilities',arguments:{}})}));
console.log(JSON.stringify({read:await rpc('tools/call',{name:'t3_thread_read',arguments:{threadId:'mcp:ec5a0341-c7ed-432c-b1b4-53bedf73b46f',limit:4,maxCharsPerItem:100}})}));
} finally {
 for(const session of JSON.parse(execFileSync('t3',['auth','session','list','--json'],{encoding:'utf8'}))) {
  if(!previousSessions.has(session.sessionId) && session.client?.label==='Zuruf feasibility') execFileSync('t3',['auth','session','revoke',session.sessionId],{stdio:'ignore'});
 }
}
