#!/usr/bin/env python3
"""End-to-end smoke test of rs-empire in a real pseudo-terminal.

The Rust test suite drives the front end through a scripted `Ui` (src/tui_tests.rs).
This script covers what those tests can't: the real terminal path. It runs the
release binary in a pseudo-terminal, types into it the way a terminal does (escape
sequences for arrow keys, Ctrl-C as a byte), reads the screen back with a terminal
emulator, and checks that:

  - a new game starts, plays, saves and quits cleanly;
  - a saved game resumes with --load;
  - a bad --load reports an error and exits non-zero;
  - the screen survives the terminal being resized mid-game;
  - the terminal is put back (main screen, cursor shown) on every exit.

Usage:
    pip install pyte
    cargo build --release
    python3 tools/tui_smoke.py [path/to/rs-empire]

Exit status 0 if every check passes.
"""
import fcntl
import os
import pty
import select
import signal
import struct
import sys
import tempfile
import termios
import time

try:
    import pyte
except ImportError:
    sys.exit("tui_smoke.py needs the pyte terminal emulator: pip install pyte")

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BINARY = os.path.abspath(sys.argv[1]) if len(sys.argv) > 1 else os.path.join(ROOT, "target", "release", "rs-empire")
SEED = "1792784026"

UP, DOWN, RIGHT, LEFT = "\x1b[A", "\x1b[B", "\x1b[C", "\x1b[D"
ENTER, ESC, CTRL_C = "\r", "\x1b", "\x03"

failures = []


class Session:
    """The game running in a pseudo-terminal, with an emulated screen."""

    def __init__(self, args, cwd, cols=120, rows=40):
        self.cols, self.rows = cols, rows
        self.raw = b""
        self.pid, self.fd = pty.fork()
        if self.pid == 0:
            os.chdir(cwd)
            os.environ["TERM"] = "xterm-256color"
            os.execv(BINARY, [BINARY] + args)
        self._set_size(cols, rows)
        self.screen = pyte.Screen(cols, rows)
        self.stream = pyte.ByteStream(self.screen)
        self.status = None

    def _set_size(self, cols, rows):
        fcntl.ioctl(self.fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))

    def pump(self, seconds=0.1):
        end = time.time() + seconds
        while time.time() < end:
            r, _, _ = select.select([self.fd], [], [], 0.05)
            if r:
                try:
                    data = os.read(self.fd, 65536)
                except OSError:
                    return
                if not data:
                    return
                self.raw += data
                self.stream.feed(data)

    def text(self):
        return "\n".join(self.screen.display)

    def wait_for(self, needle, timeout=15):
        end = time.time() + timeout
        while time.time() < end:
            self.pump(0.1)
            if needle in self.text():
                return True
        return False

    def send(self, keys, gap=0.08):
        """Types one key (a string, which may be an escape sequence), or a list of keys
        one after another."""
        for k in [keys] if isinstance(keys, str) else keys:
            os.write(self.fd, k.encode())
            self.pump(gap)

    def resize(self, cols, rows):
        self.cols, self.rows = cols, rows
        self._set_size(cols, rows)
        self.screen.resize(rows, cols)
        os.kill(self.pid, signal.SIGWINCH)
        self.pump(0.5)

    def wait_exit(self, timeout=10):
        end = time.time() + timeout
        while time.time() < end:
            self.pump(0.1)
            pid, status = os.waitpid(self.pid, os.WNOHANG)
            if pid:
                self.pump(0.2)
                self.status = os.waitstatus_to_exitcode(status)
                return self.status
        os.kill(self.pid, signal.SIGKILL)
        os.waitpid(self.pid, 0)
        return None


def check(cond, what, session=None):
    if cond:
        print("  ok   " + what)
    else:
        print("  FAIL " + what)
        failures.append(what)
        if session is not None:
            print("       --- screen ---")
            for line in session.screen.display:
                if line.strip():
                    print("       " + line.rstrip())


def terminal_restored(s):
    """The program left the alternate screen and showed the cursor again."""
    tail = s.raw[-4000:]
    return b"\x1b[?1049l" in tail and b"\x1b[?25h" in tail


def new_game(workdir):
    print("new game: start, play, edit mode, save, quit")
    s = Session(["--seed", SEED], workdir)
    check(s.wait_for("difficulty level"), "asks for a difficulty level", s)
    s.send(list("0") + [ENTER])
    check(s.wait_for("Autosave"), "asks for an autosave interval", s)
    s.send(list("0") + [ENTER])
    check(s.wait_for("Build?"), "asks what the first city builds", s)
    s.send("a")
    check(s.wait_for("[N]ext"), "reaches the turn prompt", s)
    check("Turn: 0" in s.text() or "Turn: 1" in s.text(), "the title shows the turn", s)

    # end turns until the first army needs orders
    for _ in range(12):
        if "[Army at" in s.text():
            break
        s.send("n", gap=0.3)
    check(s.wait_for("[Army at"), "the first army asks for orders", s)
    check("[T]rnsWait" in s.text().replace("\n", ""), "the whole order list is on screen", s)
    s.send("s")
    check(s.wait_for("[N]ext"), "sentry puts the army to rest", s)

    s.send("e")
    check(s.wait_for("CURSOR INFO"), "edit mode shows the cursor panel", s)
    s.send([RIGHT, DOWN, LEFT, UP])
    check("Mode: Edit" in s.text(), "arrow keys move the cursor in edit mode", s)
    s.send(ESC, gap=0.5)
    check(s.wait_for("[N]ext"), "Esc leaves edit mode", s)

    s.resize(80, 24)
    check(s.wait_for("Turn:"), "redraws after shrinking to 80x24", s)
    s.resize(160, 50)
    check(s.wait_for("Turn:"), "redraws after growing to 160x50", s)

    s.send("s")
    check(s.wait_for("Game saved to empire.sav"), "S saves the game", s)
    check(os.path.exists(os.path.join(workdir, "empire.sav")), "empire.sav was written")

    s.send(CTRL_C)
    check(s.wait_for("Really quit"), "Ctrl-C asks to confirm", s)
    s.send("n")
    check(s.wait_for("Quit cancelled"), "N keeps playing", s)
    s.send("q")
    check(s.wait_for("Really quit"), "Q asks to confirm", s)
    s.send("y")
    check(s.wait_exit() == 0, "quits with status 0", s)
    check(terminal_restored(s), "the terminal is restored on quit")


def resume(workdir):
    print("resume: --load the saved game")
    s = Session(["--load", "empire.sav"], workdir)
    check(s.wait_for("Autosave"), "a loaded game only asks about autosave", s)
    s.send(list("0") + [ENTER])
    check(s.wait_for("Game loaded from command line"), "the game is loaded", s)
    s.send(CTRL_C)
    s.wait_for("Really quit")
    s.send("y")
    check(s.wait_exit() == 0, "quits with status 0", s)
    check(terminal_restored(s), "the terminal is restored on quit")


def bad_load(workdir):
    print("bad --load: reports the problem")
    s = Session(["--load", "no-such-file.sav"], workdir)
    status = s.wait_exit()
    check(status not in (None, 0), "exits with an error status")
    check(b"failed to load save file" in s.raw, "says why")
    check(terminal_restored(s), "the terminal is restored")


def startup_ctrl_c(workdir):
    print("Ctrl-C at the first question: leaves cleanly")
    s = Session(["--seed", SEED], workdir)
    s.wait_for("difficulty level")
    s.send(CTRL_C)
    check(s.wait_exit() == 0, "exits with status 0")
    check(terminal_restored(s), "the terminal is restored")


def main():
    if not os.path.exists(BINARY):
        sys.exit("no binary at %s: run cargo build --release first" % BINARY)
    with tempfile.TemporaryDirectory() as workdir:
        new_game(workdir)
        resume(workdir)
        bad_load(workdir)
        startup_ctrl_c(workdir)
    print()
    if failures:
        print("%d check(s) failed" % len(failures))
        sys.exit(1)
    print("all checks passed")


if __name__ == "__main__":
    main()
