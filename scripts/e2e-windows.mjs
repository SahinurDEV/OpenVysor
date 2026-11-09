// Drives detached app windows through WebView2's DevTools protocol.
// Launches Calculator and Clock only, and saves pictures of those windows only.
// The whole-phone window is checked by numbers alone, so no private screen is captured.
import { execFileSync } from "node:child_process";
import { writeFileSync } from "node:fs";

const PORT = 9223;
const OUT = process.argv[2];
const CALCULATOR = "com.google.android.calculator";
const CLOCK = "com.google.android.deskclock";
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const adb = (...args) => execFileSync("adb", args, { encoding: "utf8" });

const pages = async () =>
  (await (await fetch(`http://127.0.0.1:${PORT}/json`)).json()).filter((p) => p.type === "page");

async function connect(page) {
  const socket = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    socket.onopen = resolve;
    socket.onerror = reject;
  });
  let id = 0;
  const pending = new Map();
  const errors = [];
  socket.onmessage = (event) => {
    const message = JSON.parse(event.data);
    if (message.id && pending.has(message.id)) {
      pending.get(message.id)(message);
      pending.delete(message.id);
    } else if (message.method === "Runtime.exceptionThrown") {
      errors.push(message.params.exceptionDetails.text);
    } else if (message.method === "Runtime.consoleAPICalled" && message.params.type === "error") {
      errors.push(message.params.args.map((a) => a.value ?? a.description).join(" "));
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
  await send("Runtime.enable");
  await send("Page.enable");
  return { socket, send, evaluate, errors };
}

const SAMPLE = `(() => { const c=document.querySelector('[data-testid=mirror-canvas]'); if(!c||!c.width) return null; const r=c.getBoundingClientRect(); const s=document.createElement('canvas'); s.width=36; s.height=80; const g=s.getContext('2d'); g.drawImage(c,0,0,36,80); const d=g.getImageData(0,0,36,80).data; let sum=0,hash=0; for(let i=0;i<d.length;i+=4){const v=d[i]+d[i+1]+d[i+2]; sum+=v; hash=(hash*31+v)|0;} return {w:c.width,h:c.height,cssW:r.width,cssH:r.height,left:r.left,top:r.top,mean:Math.round(sum/(d.length/4)),hash,inner:[innerWidth,innerHeight],screenX,focused:document.hasFocus()}; })()`;

async function waitForWindow(known, count) {
  for (let i = 0; i < 60; i += 1) {
    const found = (await pages()).filter((p) => !known.has(p.id));
    if (found.length >= count) return found;
    await sleep(500);
  }
  throw new Error("window did not open");
}

async function waitForVideo(client) {
  for (let i = 0; i < 40; i += 1) {
    const sample = await client.evaluate(SAMPLE);
    if (sample && sample.mean > 0) return sample;
    await sleep(500);
  }
  return null;
}

const results = {};
const start = await pages();
const main = await connect(start[0]);
await sleep(3000);
const known = new Set(start.map((p) => p.id));

const clickButton = (text) =>
  main.evaluate(`[...document.querySelectorAll('button')].find(b=>b.textContent.includes(${JSON.stringify(text)})).click()`);

await clickButton("App Windows");
for (let i = 0; i < 60; i += 1) {
  if (await main.evaluate(`!!document.querySelector('[data-package="${CALCULATOR}"]')`)) break;
  await sleep(500);
}
results.appsListed = await main.evaluate(`document.querySelectorAll('[data-package]').length`);

async function openApp(appPackage) {
  await main.evaluate(`document.querySelector('[data-package="${appPackage}"]').click()`);
  const [page] = await waitForWindow(known, 1);
  known.add(page.id);
  const client = await connect(page);
  const sample = await waitForVideo(client);
  return { client, sample };
}

const calculator = await openApp(CALCULATOR);
results.calculator = calculator.sample;
const clock = await openApp(CLOCK);
results.clock = clock.sample;
await sleep(2500);

const displays = adb("shell", "dumpsys activity activities | grep -E 'Display #|mResumedActivity|ResumedActivity:' | head -n 20");
results.calculatorOnOwnDisplay = displays
  .split("\n")
  .filter((line) => line.includes(CALCULATOR) || line.includes(CLOCK) || line.includes("Display #"))
  .map((line) => line.trim().replace(/ActivityRecord\{\S+ u0 /, ""));

// Tap the middle of the Calculator's keypad and look for the picture changing.
const before = await calculator.client.evaluate(SAMPLE);
const x = before.left + before.cssW * 0.4;
const y = before.top + before.cssH * 0.72;
await calculator.client.send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
await calculator.client.send("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "left", clickCount: 1, buttons: 1 });
await sleep(80);
await calculator.client.send("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button: "left", clickCount: 1, buttons: 0 });
await sleep(1500);
const after = await calculator.client.evaluate(SAMPLE);
results.tapChangedPicture = before.hash !== after.hash;

for (const [name, entry] of [["calculator", calculator], ["clock", clock]]) {
  const reply = await entry.client.send("Page.captureScreenshot", { format: "png" });
  writeFileSync(`${OUT}/window-${name}.png`, Buffer.from(reply.result.data, "base64"));
}

await clickButton("Arrange side by side");
await sleep(800);
results.positions = {
  calculator: await calculator.client.evaluate(`[screenX, screenY, outerWidth, outerHeight]`),
  clock: await clock.client.evaluate(`[screenX, screenY, outerWidth, outerHeight]`),
};

// Alt+Right from the Calculator window should move focus to the Clock window.
await calculator.client.send("Page.bringToFront");
await sleep(300);
await calculator.client.evaluate(`window.__TAURI_INTERNALS__.invoke('focus_adjacent_window',{direction:1})`);
await sleep(800);
results.focusAfterSwitch = {
  calculator: await calculator.client.evaluate(`document.hasFocus()`),
  clock: await clock.client.evaluate(`document.hasFocus()`),
};

// Whole phone, numbers only.
await clickButton("Detach whole phone");
const [wholePage] = await waitForWindow(known, 1);
known.add(wholePage.id);
const whole = await connect(wholePage);
const wholeSample = await waitForVideo(whole);
results.wholePhone = wholeSample && { w: wholeSample.w, h: wholeSample.h, nonBlack: wholeSample.mean > 0 };
results.concurrentServers = adb("shell", "ps -ef | grep com.genymobile.scrcpy.Server | grep -v grep | grep -v CleanUp | wc -l").trim();

results.errors = {
  main: main.errors,
  calculator: calculator.client.errors,
  clock: clock.client.errors,
  whole: whole.errors,
};

for (const entry of [calculator.client, clock.client, whole]) {
  entry.evaluate(`[...document.querySelectorAll('button')].find(b=>b.title==='Close').click()`).catch(() => {});
}
await sleep(3500);
results.forwardsAfterClose = adb("forward", "--list").trim();
results.serversAfterClose = adb("shell", "ps -ef | grep com.genymobile.scrcpy | grep -v grep || true").trim();
results.windowsLeft = (await pages()).length;
console.log(JSON.stringify(results, null, 2));
process.exit(0);
