#!/usr/bin/env python3
"""End-to-end test of the browser build in a real (headless) browser.

Serves web/www/ locally with the cross-origin isolation headers, opens it in Chromium
with Playwright, plays through the page with real keyboard and mouse events, and reads
the screen back through the page's rsEmpireScreenText() hook. It checks that:

  - the page is cross-origin isolated and the game starts;
  - the startup questions, turns and a unit's orders work;
  - edit mode and the arrow keys work;
  - saving goes to local storage and survives a page reload, and loading brings it back;
  - the mouse wheel scrolls the message log, and the screen follows the window size;
  - the menu shows the version and seed, downloads the current game, loads a
    downloaded game back in, and pre-fills a problem report;
  - quitting asks to confirm and ends the game with a notice;
  - nothing is logged as an error along the way.

Usage:
    pip install playwright && playwright install chromium
    web/build.sh
    python3 tools/web_smoke.py

Exit status 0 if every check passes.
"""
import functools
import http.server
import json
import os
import re
import urllib.parse
import sys
import threading
import time

try:
    from playwright.sync_api import sync_playwright
except ImportError:
    sys.exit("web_smoke.py needs Playwright: pip install playwright && playwright install chromium")

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "web", "www")
CARGO_TOML = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "Cargo.toml")
SEED = "1792784026"
failures = []


class Handler(http.server.SimpleHTTPRequestHandler):
    def end_headers(self):
        self.send_header("Cross-Origin-Opener-Policy", "same-origin")
        self.send_header("Cross-Origin-Embedder-Policy", "require-corp")
        self.send_header("Cache-Control", "no-store")
        super().end_headers()

    def log_message(self, *args):
        pass


def serve():
    handler = functools.partial(Handler, directory=ROOT)
    handler.extensions_map = {**http.server.SimpleHTTPRequestHandler.extensions_map,
                              ".js": "text/javascript", ".wasm": "application/wasm"}
    httpd = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    return httpd, f"http://127.0.0.1:{httpd.server_address[1]}/"


def check(cond, what, page=None):
    if cond:
        print("  ok   " + what)
    else:
        print("  FAIL " + what)
        failures.append(what)
        if page is not None:
            print("       --- screen ---")
            for line in screen(page).splitlines():
                if line.strip():
                    print("       " + line)


def screen(page):
    return page.evaluate("window.rsEmpireScreenText ? window.rsEmpireScreenText() : ''")


def wait_for(page, needle, timeout=20):
    end = time.time() + timeout
    while time.time() < end:
        if needle in screen(page):
            return True
        time.sleep(0.05)
    return False


def keys(page, *names):
    for k in names:
        page.keyboard.press(k)
        time.sleep(0.05)


def start_game(page, url):
    page.goto(url + "?seed=" + SEED)
    answer_startup(page)


def answer_startup(page):
    check(page.evaluate("self.crossOriginIsolated"), "the page is cross-origin isolated")
    check(wait_for(page, "difficulty level"), "asks for a difficulty level", page)
    keys(page, "0", "Enter")
    check(wait_for(page, "Autosave"), "asks for an autosave interval", page)
    keys(page, "0", "Enter")
    check(wait_for(page, "Build?"), "asks what the first city builds", page)
    keys(page, "a")
    check(wait_for(page, "[N]ext"), "reaches the turn prompt", page)


def main():
    if not os.path.exists(os.path.join(ROOT, "pkg", "rs_empire_web_bg.wasm")):
        sys.exit("no build in web/www/pkg: run web/build.sh first")
    httpd, url = serve()
    errors = []
    with sync_playwright() as p:
        browser = p.chromium.launch()
        page = browser.new_page(viewport={"width": 1400, "height": 860})
        page.on("console", lambda m: m.type == "error" and errors.append(m.text))
        page.on("pageerror", lambda e: errors.append(str(e)))

        print("new game: start, play, orders, edit mode")
        start_game(page, url)
        for _ in range(12):
            if "[Army at" in screen(page):
                break
            keys(page, "n")
            time.sleep(0.3)
        check(wait_for(page, "[Army at"), "the first army asks for orders", page)
        check("[T]rnsWait" in screen(page).replace("\n", ""), "the whole order list is on screen", page)
        keys(page, "Numpad8")
        check(wait_for(page, "[N]ext") or wait_for(page, "[Army at"), "the keypad moves the army", page)
        if "[Army at" in screen(page):
            keys(page, "s")
        check(wait_for(page, "[N]ext"), "back at the turn prompt", page)
        keys(page, "e")
        check(wait_for(page, "CURSOR INFO"), "edit mode shows the cursor panel", page)
        before = screen(page)
        keys(page, "ArrowRight", "ArrowDown")
        time.sleep(0.3)
        check(screen(page) != before, "arrow keys move the edit cursor", page)
        keys(page, "Escape")
        check(wait_for(page, "[N]ext"), "Esc leaves edit mode", page)

        print("saving and loading")
        keys(page, "s")
        check(wait_for(page, "Game saved to empire.sav"), "S saves the game", page)
        saved = page.evaluate("localStorage.getItem('rs-empire:empire.sav')")
        check(saved is not None and saved.startswith("{"), "the save is in local storage")
        turn_line = screen(page).splitlines()[0]
        page.reload()
        start_game(page, url)
        keys(page, "l")
        check(wait_for(page, "Game loaded from empire.sav"), "after a reload, L loads the save", page)
        check(screen(page).splitlines()[0] == turn_line, "it is the same game, on the same turn", page)

        print("the playtester menu")
        version = re.search(r'^version = "([^"]+)"', open(CARGO_TOML).read(), re.M).group(1)
        page.on("dialog", lambda d: d.accept())
        page.click("#menu-button")
        check(page.is_visible("#menu"), "the menu opens")
        check(page.inner_text("#menu-version") == version, f"it shows the version ({version})")
        check(page.inner_text("#menu-seed") == SEED, "it shows the world's seed")
        check(("?seed=" + SEED) in page.get_attribute("#menu-seed-link", "href"), "with a link to play the same map")
        report = urllib.parse.unquote(page.get_attribute("#menu-report", "href"))
        check("issues/new" in report and version in report and SEED in report and "Turn:" in report,
              "the problem report is pre-filled with version, seed and turn")
        turn_before = screen(page).splitlines()[0]
        keys(page, "n")
        time.sleep(0.3)
        check(screen(page).splitlines()[0] == turn_before, "keys don't reach the game while the menu is open")
        keys(page, "Escape")
        check(not page.is_visible("#menu"), "Esc closes the menu")
        page.click("#menu-button")
        with page.expect_download() as dl:
            page.click("#menu-download")
        path = dl.value.path()
        exported = json.load(open(path))
        turn_now = re.search(r"Turn: (\d+)", screen(page)).group(1)
        check(str(exported.get("turn")) == turn_now and dl.value.suggested_filename.endswith(".json"),
              f"Download gives the current game as JSON ({dl.value.suggested_filename})")
        check("[N]ext" in screen(page) or "[Army at" in screen(page), "and the game carries on", page)
        keys(page, "n")
        time.sleep(0.3)
        page.click("#menu-button")
        with page.expect_navigation():
            page.set_input_files("#menu-file", path)
        page.wait_for_function("document.getElementById('notice').textContent.length > 0")
        notice = page.inner_text("#notice")
        check("is now the saved game" in notice, "loading a saved game file says so after the reload: " + repr(notice))
        answer_startup(page)
        keys(page, "l")
        check(wait_for(page, "Game loaded from empire.sav"), "L then loads it", page)
        check(re.search(r"Turn: (\d+)", screen(page)).group(1) == turn_now, "it is the downloaded game, on its turn", page)

        print("mouse and window size")
        page.mouse.move(400, 760)
        page.mouse.wheel(0, -120)
        check(wait_for(page, "Scroll: 1"), "the mouse wheel scrolls the message log", page)
        width_before = max(len(l) for l in screen(page).splitlines())
        page.set_viewport_size({"width": 900, "height": 600})
        time.sleep(0.6)
        width_after = max(len(l) for l in screen(page).splitlines())
        check(width_after < width_before, f"the screen follows the window ({width_before} -> {width_after} columns)", page)

        print("quitting")
        keys(page, "Control+c")
        check(wait_for(page, "Really quit"), "Ctrl-C asks to confirm", page)
        keys(page, "n")
        check(wait_for(page, "Quit cancelled"), "N keeps playing", page)
        keys(page, "q")
        check(wait_for(page, "Really quit"), "Q asks to confirm", page)
        keys(page, "y")
        time.sleep(0.5)
        notice = page.evaluate("document.getElementById('notice').hidden ? '' : document.getElementById('notice').textContent")
        check("Game over" in notice, "the game ends with a notice: " + repr(notice))

        check(not errors, "no errors in the browser console" + ("" if not errors else ": " + "; ".join(errors[:3])))
        browser.close()
    httpd.shutdown()
    print()
    if failures:
        print("%d check(s) failed" % len(failures))
        sys.exit(1)
    print("all checks passed")


if __name__ == "__main__":
    main()
