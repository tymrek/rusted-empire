// A service worker that makes the page "cross-origin isolated", which the browser
// requires before it allows the shared memory the game uses (SharedArrayBuffer).
// A server would normally do this with two response headers; static hosts such as
// GitHub Pages can't set headers, so this worker adds them to every response.
//
// Copyright (C) 2026 James T. Jordan. GPL-2.0-only; see LICENSE.

self.addEventListener("install", () => self.skipWaiting());
self.addEventListener("activate", (e) => e.waitUntil(self.clients.claim()));

self.addEventListener("fetch", (e) => {
  const req = e.request;
  if (req.cache === "only-if-cached" && req.mode !== "same-origin") return;
  e.respondWith(
    fetch(req).then((res) => {
      if (res.status === 0) return res;
      const headers = new Headers(res.headers);
      headers.set("Cross-Origin-Embedder-Policy", "require-corp");
      headers.set("Cross-Origin-Opener-Policy", "same-origin");
      return new Response(res.body, { status: res.status, statusText: res.statusText, headers });
    })
  );
});
