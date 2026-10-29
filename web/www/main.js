// The page side of the browser front end: draws the game's screen on a canvas,
// forwards the keyboard and mouse to the game (which runs in worker.js), and keeps
// saves in local storage.
//
// Input goes to the worker through shared memory, so the game can wait for it the
// way the terminal version waits for the keyboard. Layout of `ctl` (32-bit ints):
//   0 head, 1 tail     ring buffer positions (the page writes head, the game tail)
//   2 cols, 3 rows     screen size in character cells
//   4 signal           bumped on every event or resize; the game waits on it
//   5 sleep            never changes; the game waits on it to sleep
//   16..               the ring: 256 events of 6 ints (see web.rs `decode`)
//
// Copyright (C) 2026 James T. Jordan. GPL-2.0-only; see LICENSE.

const HEAD = 0, TAIL = 1, COLS = 2, ROWS = 3, SIGNAL = 4, RING = 16, SLOT = 6, CAPACITY = 256;
const SAVE_PREFIX = "rs-empire:";
const FONT_PX = 16;
const FONT_FAMILY = '"DejaVu Sans Mono", "Menlo", "Consolas", "Liberation Mono", monospace';

// ratatui's named colours, in its order (web.rs `color_code` 1-16), Tango palette.
const PALETTE = [
  "#000000", "#cc0000", "#4e9a06", "#c4a000", "#3465a4", "#75507b", "#06989a", "#d3d7cf",
  "#555753", "#ef2929", "#8ae234", "#fce94f", "#729fcf", "#ad7fa8", "#34e2e2", "#eeeeec",
];
const DEFAULT_FG = "#d3d7cf";
const DEFAULT_BG = "#101010";
const BOLD = 1, DIM = 2, ITALIC = 4, UNDERLINED = 8, REVERSED = 64, HIDDEN = 128;

const canvas = document.getElementById("screen");
const ctx = canvas.getContext("2d");
const notice = document.getElementById("notice");

let ctl = null;
let cellW = 9, cellH = 20;
let lastFrame = null;
let dragging = null; // the mouse button held down, if any
let lastCell = null;

const SOURCE_URL = "https://github.com/tymrek/rusted-empire";
let noticeTimer = null;

// Shows a message over the game; `withSource` adds the link to the source code (the
// game is GPL software). A message with `seconds` disappears by itself.
function say(message, isError, withSource, seconds) {
  clearTimeout(noticeTimer);
  notice.replaceChildren(document.createTextNode(message));
  if (withSource) {
    const a = document.createElement("a");
    a.href = SOURCE_URL;
    a.textContent = "Source code and license";
    a.target = "_blank";
    a.rel = "noopener";
    notice.append(document.createElement("br"), a);
  }
  notice.className = isError ? "error" : "";
  notice.hidden = !message;
  if (seconds) noticeTimer = setTimeout(() => { notice.hidden = true; }, seconds * 1000);
}

function color(code, fallback) {
  if (code === 0) return fallback;
  if (code <= 16) return PALETTE[code - 1];
  const tag = code >>> 24;
  if (tag === 2) return "#" + (code & 0xffffff).toString(16).padStart(6, "0");
  if (tag === 1) {
    const i = code & 0xff;
    if (i < 16) return PALETTE[i];
    if (i >= 232) { const v = 8 + (i - 232) * 10; return `rgb(${v},${v},${v})`; }
    const n = i - 16, f = (x) => (x ? 55 + x * 40 : 0);
    return `rgb(${f(Math.floor(n / 36))},${f(Math.floor(n / 6) % 6)},${f(n % 6)})`;
  }
  return fallback;
}

// ---- screen size ---------------------------------------------------------------

function measure() {
  ctx.font = `${FONT_PX}px ${FONT_FAMILY}`;
  cellW = Math.ceil(ctx.measureText("M").width);
  cellH = Math.ceil(FONT_PX * 1.25);
}

function fit() {
  const dpr = window.devicePixelRatio || 1;
  const w = window.innerWidth, h = window.innerHeight;
  canvas.style.width = w + "px";
  canvas.style.height = h + "px";
  canvas.width = Math.floor(w * dpr);
  canvas.height = Math.floor(h * dpr);
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  measure();
  const cols = Math.max(20, Math.floor(w / cellW));
  const rows = Math.max(10, Math.floor(h / cellH));
  if (ctl) {
    Atomics.store(ctl, COLS, cols);
    Atomics.store(ctl, ROWS, rows);
    wake();
  }
  if (lastFrame) paint(lastFrame);
  return [cols, rows];
}

// ---- drawing -------------------------------------------------------------------

function paint(f) {
  lastFrame = f;
  ctx.fillStyle = DEFAULT_BG;
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  ctx.textBaseline = "top";
  for (let r = 0; r < f.rows; r++) {
    for (let c = 0; c < f.cols; c++) {
      const i = r * f.cols + c;
      const m = f.mods[i];
      let fg = color(f.fg[i], DEFAULT_FG), bg = color(f.bg[i], null);
      if (m & REVERSED) { const t = fg; fg = bg || DEFAULT_BG; bg = t; }
      const x = c * cellW, y = r * cellH;
      if (bg) { ctx.fillStyle = bg; ctx.fillRect(x, y, cellW, cellH); }
      const ch = f.chars[i];
      if (ch === 32 || ch === 0 || (m & HIDDEN)) continue;
      ctx.font = `${m & ITALIC ? "italic " : ""}${m & BOLD ? "bold " : ""}${FONT_PX}px ${FONT_FAMILY}`;
      ctx.globalAlpha = m & DIM ? 0.6 : 1;
      ctx.fillStyle = fg;
      const s = String.fromCodePoint(ch);
      if (s === "█") ctx.fillRect(x, y, cellW, cellH); // a solid block fills the cell exactly
      else if (!drawBox(ch, x, y)) ctx.fillText(s, x, y + 2);
      if (m & UNDERLINED) ctx.fillRect(x, y + cellH - 2, cellW, 1);
    }
  }
  ctx.globalAlpha = 1;
}

// Box-drawing characters (the panel borders) drawn as lines, so they join up across
// cells whatever the font. Each entry: which of up, down, left, right the line reaches.
const BOX = {
  0x2500: "lr", 0x2502: "ud", 0x250c: "dr", 0x2510: "dl", 0x2514: "ur", 0x2518: "ul",
  0x251c: "udr", 0x2524: "udl", 0x252c: "dlr", 0x2534: "ulr", 0x253c: "udlr",
};

function drawBox(ch, x, y) {
  const arms = BOX[ch];
  if (!arms) return false;
  const cx = x + Math.floor(cellW / 2), cy = y + Math.floor(cellH / 2), t = 1;
  if (arms.includes("l")) ctx.fillRect(x, cy, cx - x + t, t);
  if (arms.includes("r")) ctx.fillRect(cx, cy, x + cellW - cx, t);
  if (arms.includes("u")) ctx.fillRect(cx, y, t, cy - y + t);
  if (arms.includes("d")) ctx.fillRect(cx, cy, t, y + cellH - cy);
  return true;
}

// The screen as text, one line per row (for automated tests, and handy in the console).
window.rsEmpireScreenText = () => {
  if (!lastFrame) return "";
  const f = lastFrame, lines = [];
  for (let r = 0; r < f.rows; r++) {
    let line = "";
    for (let c = 0; c < f.cols; c++) line += String.fromCodePoint(f.chars[r * f.cols + c] || 32);
    lines.push(line.replace(/\s+$/, ""));
  }
  return lines.join("\n");
};

// ---- input ---------------------------------------------------------------------

function wake() {
  Atomics.add(ctl, SIGNAL, 1);
  Atomics.notify(ctl, SIGNAL);
}

function send(ev) {
  const head = Atomics.load(ctl, HEAD), tail = Atomics.load(ctl, TAIL);
  if (head - tail >= CAPACITY) return; // the game is far behind: drop the event
  const at = RING + (head % CAPACITY) * SLOT;
  for (let i = 0; i < SLOT; i++) Atomics.store(ctl, at + i, ev[i] || 0);
  Atomics.store(ctl, HEAD, head + 1);
  wake();
}

const NAMED_KEYS = {
  Enter: 1, Escape: 2, Backspace: 3, ArrowUp: 4, ArrowDown: 5, ArrowLeft: 6, ArrowRight: 7,
  Tab: 8, Delete: 9, Home: 10, End: 11, PageUp: 12, PageDown: 13,
};

function modifiers(e) {
  return (e.shiftKey ? 1 : 0) | (e.ctrlKey ? 2 : 0) | (e.altKey ? 4 : 0);
}

window.addEventListener("keydown", (e) => {
  if (!ctl || e.isComposing || e.metaKey) return;
  // Leave the browser's own Ctrl shortcuts alone, except Ctrl-C, which the game uses to quit.
  if ((e.ctrlKey || e.altKey) && e.key.toLowerCase() !== "c") return;
  let ev = null;
  const numpad = /^Numpad([0-9])$/.exec(e.code);
  if (numpad) ev = [1, 0, numpad[1].charCodeAt(0), modifiers(e)]; // the keypad moves units even with Num Lock off
  else if (e.key in NAMED_KEYS) ev = [1, NAMED_KEYS[e.key], 0, modifiers(e)];
  else if ([...e.key].length === 1) ev = [1, 0, e.key.codePointAt(0), modifiers(e)];
  if (!ev) return;
  e.preventDefault();
  send(ev);
});

function cellAt(e) {
  const rect = canvas.getBoundingClientRect();
  return [Math.floor((e.clientX - rect.left) / cellW), Math.floor((e.clientY - rect.top) / cellH)];
}

const BUTTONS = { 0: 0, 1: 2, 2: 1 }; // DOM button -> left, middle, right (web.rs order)

canvas.addEventListener("wheel", (e) => {
  if (!ctl) return;
  e.preventDefault();
  const [c, r] = cellAt(e);
  send([2, e.deltaY < 0 ? 5 : 4, 0, c, r, modifiers(e)]);
}, { passive: false });

canvas.addEventListener("mousedown", (e) => {
  if (!ctl) return;
  const [c, r] = cellAt(e);
  dragging = BUTTONS[e.button] ?? 0;
  lastCell = c + "," + r;
  send([2, 0, dragging, c, r, modifiers(e)]);
});

window.addEventListener("mousemove", (e) => {
  if (!ctl || dragging === null) return;
  const [c, r] = cellAt(e);
  if (c + "," + r === lastCell) return;
  lastCell = c + "," + r;
  send([2, 2, dragging, c, r, modifiers(e)]);
});

window.addEventListener("mouseup", (e) => {
  if (!ctl || dragging === null) return;
  const [c, r] = cellAt(e);
  send([2, 1, dragging, c, r, modifiers(e)]);
  dragging = null;
});

canvas.addEventListener("contextmenu", (e) => e.preventDefault());

// ---- start ---------------------------------------------------------------------

function loadSaves() {
  const saves = {};
  try {
    for (let i = 0; i < localStorage.length; i++) {
      const k = localStorage.key(i);
      if (k && k.startsWith(SAVE_PREFIX)) saves[k.slice(SAVE_PREFIX.length)] = localStorage.getItem(k);
    }
  } catch (_) {
    say("This browser won't let the page use local storage, so games can't be saved.", true);
  }
  return saves;
}

function storeSave(name, data) {
  try {
    localStorage.setItem(SAVE_PREFIX + name, data);
  } catch (err) {
    say(`Couldn't save "${name}" in this browser (${err.name}). Free some site storage and save again.`, true);
  }
}

async function isolate() {
  if (self.crossOriginIsolated) return true;
  // Static hosts can't send the headers shared memory needs; a service worker adds them.
  if (!("serviceWorker" in navigator)) return false;
  if (sessionStorage.getItem("rs-empire-coi-reload")) return false; // already tried once
  await navigator.serviceWorker.register("coi-sw.js");
  await navigator.serviceWorker.ready;
  sessionStorage.setItem("rs-empire-coi-reload", "1");
  location.reload();
  return new Promise(() => {}); // the page is reloading
}

async function start() {
  if (!(await isolate())) {
    say("This browser can't run the game here: it needs shared memory (cross-origin isolation). " +
        "Try a current Firefox, Chrome, Edge or Safari, opened over https.", true);
    return;
  }
  sessionStorage.removeItem("rs-empire-coi-reload");
  ctl = new Int32Array(new SharedArrayBuffer((RING + CAPACITY * SLOT) * 4));
  fit();
  window.addEventListener("resize", fit);
  const params = new URLSearchParams(location.search);
  const worker = new Worker("worker.js", { type: "module" });
  worker.onmessage = (e) => {
    const m = e.data;
    if (m.type === "frame") paint(m);
    else if (m.type === "save") storeSave(m.name, m.data);
    else if (m.type === "finished") { say(m.message, m.message.startsWith("The game c"), true); ctl = null; }
  };
  worker.onerror = (e) => say("The game failed to load: " + (e.message || e), true);
  worker.postMessage({ ctl: ctl.buffer, saves: loadSaves(), seed: params.get("seed") || "", torus: params.has("torus") });
  canvas.focus();
  say("rusted-empire: VMS-Empire in your browser. Free software under the GNU GPL v2.", false, true, 8);
}

start();
