// Runs the game (compiled to WebAssembly) off the page's main thread, where it may
// block waiting for input. See main.js for the page side and bridge.js for the calls
// the game makes.
//
// Copyright (C) 2026 James T. Jordan. GPL-2.0-only; see LICENSE.

import init, { run } from "./pkg/rs_empire_web.js";

self.onmessage = async (e) => {
  const { ctl, saves, seed, torus } = e.data;
  self.__rsEmpire = { ctl: new Int32Array(ctl), saves };
  try {
    await init();
    run(seed, torus); // returns only when the game is over
  } catch (err) {
    self.postMessage({ type: "finished", message: "The game crashed: " + err });
  }
};
