let socket;
function connect() {
  socket = new WebSocket('ws://127.0.0.1:__PORT__/extension?token=__TOKEN__');
  socket.onopen = () => socket.send(JSON.stringify({type:'hello', extensionId:chrome.runtime.id}));
  socket.onclose = () => setTimeout(connect, 1000);
}
connect();
chrome.sidePanel.setPanelBehavior({openPanelOnActionClick:true});
chrome.runtime.onMessage.addListener((message, sender, reply) => {
  if (message.type === 'panel') {
    chrome.sidePanel.open({windowId:sender.tab.windowId}).then(()=>reply({ok:true}),e=>reply({error:e.message}));
    return true;
  }
  if (socket.readyState === WebSocket.OPEN) socket.send(JSON.stringify({...message,tabId:sender.tab.id,frameId:sender.frameId}));
});
