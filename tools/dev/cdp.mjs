// Development aid: send DevTools protocol commands to a running shell and print the replies.
//   node tools/dev/cdp.mjs <port> '<method>' '<json params>' [more method/params pairs]
// Prints events that arrive for two seconds afterwards.
const [port, ...rest] = process.argv.slice(2);
const version = await (await fetch(`http://127.0.0.1:${port}/json/version`)).json();
const socket = new WebSocket(version.webSocketDebuggerUrl);
await new Promise((resolve) => (socket.onopen = resolve));

let next = 0;
const waiting = new Map();
socket.onmessage = (message) => {
  const value = JSON.parse(message.data);
  if (value.id !== undefined && waiting.has(value.id)) {
    waiting.get(value.id)(value);
    waiting.delete(value.id);
  } else {
    console.log("event", JSON.stringify(value).slice(0, 400));
  }
};
const call = (method, params, sessionId) =>
  new Promise((resolve) => {
    const id = ++next;
    waiting.set(id, resolve);
    socket.send(JSON.stringify({ id, method, params, sessionId }));
  });

for (let index = 0; index < rest.length; index += 2) {
  const reply = await call(rest[index], JSON.parse(rest[index + 1] ?? "{}"));
  console.log(rest[index], JSON.stringify(reply.result ?? reply.error).slice(0, 1500));
}
await new Promise((resolve) => setTimeout(resolve, 2000));
socket.close();
