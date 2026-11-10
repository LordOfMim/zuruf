(() => {
  let review = true;
  let keys = {w:'viewport',s:'region',d:'dom',p:'panel'};
  let cursor = {x:0,y:0};
  let dragging = null;
  let regionMode = false;
  let outline;
  const send = data => chrome.runtime.sendMessage({...data,at:Date.now()});
  const guarded = event => event.composedPath().some(node => node instanceof Element && (
    node.matches('input,textarea,select,[role="textbox"],[role="combobox"],.monaco-editor,.cm-editor,.CodeMirror,[data-zuruf-ignore]') || node.isContentEditable
  ));
  function box(rect) {
    if (!outline) {
      outline=document.createElement('div');
      outline.dataset.zurufOverlay='true';
      outline.style.cssText='position:fixed;pointer-events:none;z-index:2147483647;border:2px solid #e6522c;box-sizing:border-box';
      document.documentElement.append(outline);
    }
    Object.assign(outline.style,{left:rect.x+'px',top:rect.y+'px',width:rect.width+'px',height:rect.height+'px'});
  }
  function under() {
    let element=document.elementFromPoint(cursor.x,cursor.y);
    while(element?.shadowRoot?.elementFromPoint(cursor.x,cursor.y)) {
      const child=element.shadowRoot.elementFromPoint(cursor.x,cursor.y);
      if(child===element) break;
      element=child;
    }
    return element;
  }
  function capture(element) {
    const root=element.getRootNode();
    const candidates=[];
    const add=(kind,value,stable,reason,extra={})=>candidates.push({kind,value,stable,reason,...extra});
    const testid=element.getAttribute('data-testid');
    if(testid) add('testid',testid,true,'Expliziter Testvertrag; Eindeutigkeit separat prüfen');
    const role=element.getAttribute('role') || ({BUTTON:'button',A:'link',INPUT:element.type==='password'?undefined:'textbox'})[element.tagName];
    const name=element.getAttribute('aria-label') || (element.innerText||'').trim();
    if(role && name) add('role',role,true,'Semantischer Vertrag; Name kann sich übersetzen',{name});
    const text=(element.matches('input,textarea,[contenteditable]')?'':element.innerText||'').trim().slice(0,500);
    if(text && [...root.querySelectorAll(element.tagName)].filter(e=>e.textContent.trim()===text).length===1) add('text',text,false,'Nur aktuell im Root eindeutig; Text ist veränderlich');
    let css=element.id?'#'+CSS.escape(element.id):element.localName;
    if(!element.id) {
      let child=element;
      css='';
      while(child && child!==root.documentElement) {
        const index=[...(child.parentElement?.children||[])].filter(e=>e.localName===child.localName).indexOf(child)+1;
        css=child.localName+':nth-of-type('+Math.max(index,1)+')'+(css?' > '+css:'');
        child=child.parentElement;
      }
    }
    add('css',css,false,element.id?'ID kann generiert sein':'Positionsabhängig');
    let xpath='';
    let ancestor=element;
    while(ancestor) {
      const peers=[...(ancestor.parentElement?.children||[])].filter(e=>e.localName===ancestor.localName);
      xpath='/'+ancestor.localName+'['+Math.max(1,peers.indexOf(ancestor)+1)+']'+xpath;
      ancestor=ancestor.parentElement;
    }
    add('xpath',xpath,false,'Positionsabhängiger Fallback; durchdringt keine Shadow Roots');
    const attributes={};
    for(const attribute of element.attributes) {
      if(!/value|pass|token|secret|auth|cookie|session|src|href|on|style/i.test(attribute.name) && /^(id|class|role|type|name|data-testid|aria-label|aria-describedby|aria-labelledby)$/.test(attribute.name)) attributes[attribute.name]=attribute.value.slice(0,200);
    }
    const shadowPath=[];
    let node=element;
    while(node.getRootNode() instanceof ShadowRoot) {
      node=node.getRootNode().host;
      shadowPath.unshift(node.id?'#'+CSS.escape(node.id):node.localName);
    }
    const iframePath=[];
    let frameWindow=window;
    try {
      while(frameWindow!==frameWindow.top) {
        const host=frameWindow.frameElement;
        if(!host) {iframePath.unshift({unresolved:true});break;}
        iframePath.unshift({selector:host.id?'#'+CSS.escape(host.id):'iframe',src:new URL(host.src).origin+new URL(host.src).pathname});
        frameWindow=frameWindow.parent;
      }
    } catch {iframePath.unshift({unresolved:true});}
    const rect=element.getBoundingClientRect();
    return {tag:element.localName,attributes,text,role,name:element.type==='password'?'[redacted]':name,candidates,bbox:{x:rect.x,y:rect.y,width:rect.width,height:rect.height},parent:element.parentElement?.localName,children:[...element.children].slice(0,8).map(e=>e.localName),shadowPath,iframePath,frame:{url:location.origin+location.pathname,top:window===top},scroll:{x:scrollX,y:scrollY}};
  }
  chrome.runtime.onMessage.addListener(message=>{
    if(message.type==='configure') { review=message.review; if(message.keys) keys=message.keys; }
    if(message.type==='hide') {outline?.remove();outline=null;}
  });
  window.addEventListener('pointermove',event=>{
    cursor={x:event.clientX,y:event.clientY};
    if(!review) return;
    if(dragging) box({x:Math.min(cursor.x,dragging.x),y:Math.min(cursor.y,dragging.y),width:Math.abs(cursor.x-dragging.x),height:Math.abs(cursor.y-dragging.y)});
    else if(!guarded(event)) { const element=under(); if(element) box(element.getBoundingClientRect()); }
  },true);
  window.addEventListener('pointerdown',event=>{
    if(regionMode) { dragging={x:event.clientX,y:event.clientY}; event.preventDefault(); }
  },true);
  window.addEventListener('pointerup',event=>{
    if(!dragging) return;
    const rect={x:Math.min(cursor.x,dragging.x),y:Math.min(cursor.y,dragging.y),width:Math.abs(cursor.x-dragging.x),height:Math.abs(cursor.y-dragging.y)};
    dragging=null; regionMode=false; outline?.remove(); outline=null;
    send({type:'capture',kind:'region',rect,cursor}); event.preventDefault();
  },true);
  window.addEventListener('keydown',event=>{
    if(event.key==='Escape') {regionMode=false;dragging=null;outline?.remove();outline=null;return;}
    if(!review || event.repeat || event.isComposing || event.ctrlKey || event.metaKey || event.altKey || event.shiftKey || guarded(event)) return;
    const kind=keys[event.key.toLowerCase()];
    if(!kind) return;
    event.preventDefault(); event.stopImmediatePropagation();
    if(kind==='panel') {chrome.runtime.sendMessage({type:'panel'}, result=>send({type:'panel-result',result})); return;}
    if(kind==='region') {regionMode=true;return;}
    const element=under();
    send({type:'capture',kind,cursor,dom:kind==='dom' && element?capture(element):undefined});
  },true);
})();
