// Drives the real packaged app through WebView2's DevTools protocol.
import { execFileSync } from "node:child_process";
import { writeFileSync } from "node:fs";

const PORT = 9223;
const OUT = process.argv[2];
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const adb = (...args) => execFileSync("adb", args, { encoding: "utf8" });

async function target() {
  for (let i = 0; i < 40; i += 1) {
    try {
      const pages = await (await fetch(`http://127.0.0.1:${PORT}/json`)).json();
      const page = pages.find((entry) => entry.type === "page");
      if (page) return page;
    } catch {}
    await sleep(500);
  }
  throw new Error("no DevTools target");
}

const page = await target();
const socket = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  socket.onopen = resolve;
  socket.onerror = reject;
});

let id = 0;
const pending = new Map();
const consoleErrors = [];
socket.onmessage = (event) => {
  const message = JSON.parse(event.data);
  if (message.id && pending.has(message.id)) {
    pending.get(message.id)(message);
    pending.delete(message.id);
  } else if (message.method === "Runtime.exceptionThrown") {
    consoleErrors.push(message.params.exceptionDetails.text);
  } else if (message.method === "Runtime.consoleAPICalled" && message.params.type === "error") {
    consoleErrors.push(message.params.args.map((a) => a.value ?? a.description).join(" "));
  }
};
const send = (method, params = {}) =>
  new Promise((resolve) => {
    id += 1;
    pending.set(id, resolve);
    socket.send(JSON.stringify({ id, method, params }));
  });
const evaluate = async (expression) => {
  const reply = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  if (reply.result?.exceptionDetails) throw new Error(JSON.stringify(reply.result.exceptionDetails));
  return reply.result.result.value;
};
const shot = async (name) => {
  const reply = await send("Page.captureScreenshot", { format: "png" });
  writeFileSync(`${OUT}/${name}.png`, Buffer.from(reply.result.data, "base64"));
};
const clickText = (text) =>
  evaluate(`(() => { const b=[...document.querySelectorAll('button')].find(x=>x.textContent.includes(${JSON.stringify(text)})); if(!b||b.disabled) return false; b.click(); return true; })()`);
const stats = () => evaluate(`document.querySelector('[data-testid=mirror-stats]')?.textContent ?? ''`);
const canvasSample = () =>
  evaluate(`(() => { const c=document.querySelector('[data-testid=mirror-canvas]'); const r=c.getBoundingClientRect(); const s=document.createElement('canvas'); s.width=48; s.height=96; const g=s.getContext('2d'); g.drawImage(c,0,0,48,96); const d=g.getImageData(0,0,48,96).data; let sum=0,max=0; for(let i=0;i<d.length;i+=4){const v=d[i]+d[i+1]+d[i+2]; sum+=v; max=Math.max(max,v);} return {w:c.width,h:c.height,cssW:r.width,cssH:r.height,left:r.left,top:r.top,mean:sum/(d.length/4),max}; })()`);

await send("Runtime.enable");
await send("Page.enable");
await sleep(3500);

const results = {};
results.summary = await evaluate(`document.querySelector('[data-testid=connection-summary]')?.textContent`);
await shot("1-dashboard");

await clickText("Devices");
await sleep(500);
await shot("2-devices");

await clickText("Mirror screen");
for (let i = 0; i < 40; i += 1) {
  await sleep(500);
  if (/\d+ FPS/.test(await stats()) && !/ 0 FPS/.test(await stats())) break;
}
await sleep(4000);
results.streamStats = await stats();
await shot("3-mirror");

const a = await canvasSample();
results.canvas = a;
results.wakefulnessBefore = adb("shell", "dumpsys power | grep mWakefulness=").trim();

// Real pointer input through the browser's input pipeline, onto the canvas.
const x = a.left + a.cssW / 2;
const y = a.top + a.cssH / 8;
await send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
await send("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "left", clickCount: 1, buttons: 1 });
await sleep(60);
await send("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button: "left", clickCount: 1, buttons: 0 });
await sleep(450);
results.motionEvents = adb("shell", "dumpsys input | grep MotionEvent | tail -n 4").trim();

// A real key press through the page. F-keys have no side effects on a locked phone.
const key = { key: "F5", code: "F5", windowsVirtualKeyCode: 116 };
await send("Input.dispatchKeyEvent", { type: "keyDown", ...key });
await send("Input.dispatchKeyEvent", { type: "keyUp", ...key });
await sleep(350);
results.keyEvents = adb("shell", "dumpsys input | grep -E 'KeyEvent' | tail -n 4").trim();
results.focusedSurface = await evaluate(`document.activeElement?.getAttribute('aria-label') ?? document.activeElement?.tagName`);

// Screenshot through the backend.
await evaluate(`[...document.querySelectorAll('button')].find(b=>b.title==='Screenshot').click()`);
await sleep(2500);
results.notice = await evaluate(`[...document.querySelectorAll('div')].map(d=>d.textContent).find(t=>t&&t.startsWith('Saved to'))??null`);
await shot("4-after-input");

// Disconnect and reconnect through the UI.
await clickText("Disconnect");
await sleep(1500);
results.afterDisconnect = await stats();
results.forwardsAfterDisconnect = adb("forward", "--list").trim();
results.serverAfterDisconnect = adb("shell", "ps -ef | grep com.genymobile.scrcpy | grep -v grep || true").trim();

await clickText("Devices");
await sleep(400);
await clickText("Mirror screen");
for (let i = 0; i < 40; i += 1) {
  await sleep(500);
  if (/\d+ FPS/.test(await stats()) && !/ 0 FPS/.test(await stats())) break;
}
await sleep(1500);
results.afterReconnect = await stats();
results.consoleErrors = consoleErrors;
console.log(JSON.stringify(results, null, 2));
socket.close();
