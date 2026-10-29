// The worker side of the browser front end: the functions web/src/web.rs imports.
//
// worker.js puts the shared state in self.__rsEmpire before the game starts:
//   ctl   - Int32Array over a SharedArrayBuffer, written by the page (layout in main.js)
//   saves - the saves found in local storage when the page loaded, by name
//
// Copyright (C) 2026 James T. Jordan. GPL-2.0-only; see LICENSE.

const HEAD = 0, TAIL = 1, COLS = 2, ROWS = 3, SIGNAL = 4, SLEEP = 5, RING = 16, SLOT = 6, CAPACITY = 256;

function st() {
  return self.__rsEmpire;
}

function pending(ctl) {
  return Atomics.load(ctl, HEAD) !== Atomics.load(ctl, TAIL);
}

export function wait_input(timeoutMs) {
  const ctl = st().ctl;
  if (pending(ctl)) return true;
  const seen = Atomics.load(ctl, SIGNAL);
  if (pending(ctl)) return true;
  Atomics.wait(ctl, SIGNAL, seen, Math.max(0, timeoutMs));
  return pending(ctl);
}

export function pop_input(out) {
  const ctl = st().ctl;
  if (!pending(ctl)) return false;
  const tail = Atomics.load(ctl, TAIL);
  const at = RING + (tail % CAPACITY) * SLOT;
  for (let i = 0; i < SLOT; i++) out[i] = Atomics.load(ctl, at + i);
  Atomics.store(ctl, TAIL, tail + 1);
  return true;
}

export function screen_size() {
  const ctl = st().ctl;
  return ((Atomics.load(ctl, COLS) & 0xffff) << 16) | (Atomics.load(ctl, ROWS) & 0xffff);
}

export function present(cols, rows, chars, fg, bg, mods) {
  // The arrays are views into the game's memory: copy them before handing them over.
  const frame = { type: "frame", cols, rows, chars: chars.slice(), fg: fg.slice(), bg: bg.slice(), mods: mods.slice() };
  self.postMessage(frame, [frame.chars.buffer, frame.fg.buffer, frame.bg.buffer, frame.mods.buffer]);
}

export function sleep_ms(ms) {
  Atomics.wait(st().ctl, SLEEP, 0, Math.max(0, ms));
}

export function now_ms() {
  return performance.now();
}

export function save_put(name, data) {
  st().saves[name] = data;
  self.postMessage({ type: "save", name, data });
  return true;
}

export function save_get(name) {
  const data = st().saves[name];
  return data === undefined ? undefined : data;
}

export function finished(message) {
  self.postMessage({ type: "finished", message });
}
