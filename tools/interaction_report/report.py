# /// script
# requires-python = ">=3.11"
# dependencies = ["websocket-client>=1.8", "pillow>=10", "rangehttpserver>=1.4"]
# ///
"""Interaction report and harness for www.openpower.tools.

Drives every control in data/interaction-contract.json through the
edges of its interaction machine using REAL browser input (CDP mouse
and keyboard events, pointer resting on the control), samples what
moves, captures frames, plots curves, and renders one page per control
organised by the machine: for each edge, the machine diagram with that
edge highlighted, then the frames, then the curves, then the
assertions. Every assertion rendered is one this tool enforces: the
exit status is non-zero if any fails, so the report is also the CI
gate.

The machine diagram is derived from the machine itself (op-webc's
machine_table bin), so the documentation cannot drift from the code.

Timing comes from one of two clocks. The synthetic one drives
chrome-headless-shell under begin frame control with virtual time: the
page's clock moves only when this tool draws a frame, one sixtieth of a
second at a time, so every sample sits at an exact time and two runs
agree frame for frame. The real one is Chrome's own headless on the wall
clock, which has neither facility. --clock chooses; synthetic is the
default wherever a shell is found. Two synthetic runs take the same
decisions and measure the same quantities to within one frame; a detail may
not report a measurement more finely than its check decides on it, or it
carries a transition's last bit of floating-point wobble. --checks-json
dumps the decisions, and compare_checks.py holds two runs to that rule.

That clock is also what makes the controls safe to run at the same time.
Every measurement on it is virtual time, and virtual time moves only when
this tool draws a frame, so a busy machine cannot change a result: --jobs
runs that many controls at once, each in its own worker process with its
own shell on its own debugging port and its own page, against the one
server this run starts. The real clock measures the machine it is running
on, where a second browser under load would change the timings, so there
the controls go one at a time and --jobs is ignored.

    uv run tools/interaction_report/report.py --dist dist --out reports/interactions
"""
from __future__ import annotations

import argparse
import base64
import contextlib
import glob
import io
import json
import math
import multiprocessing
import os
import re
import shutil
import socket
import subprocess
import sys
import tempfile
import threading
import time
import urllib.request
from concurrent.futures import ProcessPoolExecutor, as_completed
from dataclasses import dataclass, field
from pathlib import Path

import websocket

STORAGE_KEY = "tools.openpower.sites.www.storage.version.1.configuration.version.1.ux.theme.current"
# The chart series palette lives in styles/theme.css as --op-series-N tokens (six Okabe-Ito
# hues fitted per theme by op-colour's fit_series). Films name a series by its number; the site's
# elements colour it from the tokens. The ad hoc revision, which must not depend on the site,
# draws with the light theme's values read from the same file.
SERIES = {"palette": 1, "thumb": 2, "ghost": 3, "preview": 4, "ideal": 5}

# Films are recorded at this device scale factor: sheets and videos share it.
DPR = 1.5
# The stage video comes from the DevTools screencast, which sustains about 23
# frames a second for the full viewport as JPEG at this quality (PNG halves
# that); the sheet keeps its exact PNG screenshots at a lower cadence.
CAST_QUALITY = 90
CAST_MARGIN = 14
# Sheet frames are cut from the screencast at most this often (seconds).
SHEET_PERIOD = 0.08
# A pixel counts as changed when a channel moves by more than this, which is
# above JPEG requantisation noise between two captures of one image.
CHANGE_TOLERANCE = 24
# The browser whose screencast the films are cut from (set by main).
RECORDER = None
OKABE = {"mark": "#D55E00"}

# ---- the synthetic clock ----------------------------------------------------
# chrome-headless-shell can be told when to draw (--enable-begin-frame-control)
# and when the page's clock may run (Emulation.setVirtualTimePolicy). Under that
# pair nothing happens between our frames, so every sample sits at an exact time
# and two runs of the same script agree frame for frame. Chrome's own new
# headless has neither, so the real clock stays available.
FRAME_RATE = 60
INTERVAL_MS = 1000.0 / FRAME_RATE
# A film's recording takes every second synthetic frame: thirty frames a second.
VIDEO_EVERY = 2
# The virtual clock starts here, 2026-01-01T00:00:00Z, so the page's Date is
# fixed too (the protocol takes seconds since the epoch).
VIRTUAL_EPOCH_MS = 1767225600000
# Loading gets this much virtual time, spent only while no fetch is pending.
LOAD_BUDGET_MS = 5000
# How far ahead of the virtual clock a frame's time is stamped. It only has to
# cover the error in mapping the virtual clock onto the renderer's ticks (both
# are the machine's monotonic clock, read a moment apart), because a frame time
# behind the virtual clock is ignored and stops the page's animations.
TICK_HEADROOM_MS = 250.0
# A virtual time budget that would otherwise be starved by a busy task queue
# still expires after this many tasks.
STARVATION = 100000
CLOCK_SYNTHETIC, CLOCK_REAL = "synthetic", "real"
# Where a chrome-headless-shell may be found when neither --shell nor
# OP_HEADLESS_SHELL names one.
SHELL_GLOBS = ("~/.cache/puppeteer/chrome-headless-shell/linux-*/chrome-headless-shell-linux64/chrome-headless-shell",
               "./chrome-headless-shell/linux-*/chrome-headless-shell-linux64/chrome-headless-shell",
               "/tmp/chrome-headless-shell/linux-*/chrome-headless-shell-linux64/chrome-headless-shell")
# The clock this run drives the pages with (set by main).
CLOCK = CLOCK_REAL


def series_hex() -> dict:
    """--op-series-N of the light theme, from styles/theme.css."""
    css = (Path(__file__).resolve().parents[2] / "styles" / "theme.css").read_text()
    block = css[css.index(':root[data-theme="light"]'):]
    block = block[:block.index("}")]
    found = dict(re.findall(r"--op-series-(\d): (#[0-9A-Fa-f]{6})", block))
    if len(found) != 6:
        raise SystemExit("styles/theme.css: expected six --op-series-N tokens in the light theme")
    return {int(k): v for k, v in found.items()}


def _srgb_to_lab(rgb):
    def lin(c):
        c /= 255.0
        return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4
    r, g, b = (lin(v) for v in rgb)
    x = (0.4124564 * r + 0.3575761 * g + 0.1804375 * b) / 0.95047
    y = (0.2126729 * r + 0.7151522 * g + 0.0721750 * b)
    z = (0.0193339 * r + 0.1191920 * g + 0.9503041 * b) / 1.08883

    def f(t):
        d = 6 / 29
        return t ** (1 / 3) if t > d ** 3 else t / (3 * d * d) + 4 / 29
    fx, fy, fz = f(x), f(y), f(z)
    return (116 * fy - 16, 500 * (fx - fy), 200 * (fy - fz))


def ciede2000(lab1, lab2):
    """CIEDE2000 (Sharma, Wu and Dalal 2005), the same formula op-colour carries."""
    L1, a1, b1 = lab1
    L2, a2, b2 = lab2
    c1, c2 = math.hypot(a1, b1), math.hypot(a2, b2)
    cb = (c1 + c2) / 2
    g = 0.5 * (1 - math.sqrt(cb ** 7 / (cb ** 7 + 25 ** 7)))
    a1p, a2p = (1 + g) * a1, (1 + g) * a2
    c1p, c2p = math.hypot(a1p, b1), math.hypot(a2p, b2)

    def hp(a, b):
        return 0.0 if a == 0 and b == 0 else math.degrees(math.atan2(b, a)) % 360
    h1p, h2p = hp(a1p, b1), hp(a2p, b2)
    dLp, dCp = L2 - L1, c2p - c1p
    if c1p * c2p == 0:
        dhp = 0.0
    else:
        d = h2p - h1p
        dhp = d if abs(d) <= 180 else (d - 360 if d > 180 else d + 360)
    dHp = 2 * math.sqrt(c1p * c2p) * math.sin(math.radians(dhp / 2))
    Lbp, Cbp = (L1 + L2) / 2, (c1p + c2p) / 2
    if c1p * c2p == 0:
        hbp = h1p + h2p
    else:
        sm = h1p + h2p
        hbp = sm / 2 if abs(h1p - h2p) <= 180 else ((sm + 360) / 2 if sm < 360 else (sm - 360) / 2)
    T = (1 - 0.17 * math.cos(math.radians(hbp - 30)) + 0.24 * math.cos(math.radians(2 * hbp))
         + 0.32 * math.cos(math.radians(3 * hbp + 6)) - 0.20 * math.cos(math.radians(4 * hbp - 63)))
    dth = 30 * math.exp(-(((hbp - 275) / 25) ** 2))
    rc = 2 * math.sqrt(Cbp ** 7 / (Cbp ** 7 + 25 ** 7))
    sl = 1 + 0.015 * (Lbp - 50) ** 2 / math.sqrt(20 + (Lbp - 50) ** 2)
    sc = 1 + 0.045 * Cbp
    sh = 1 + 0.015 * Cbp * T
    rt = -math.sin(math.radians(2 * dth)) * rc
    return math.sqrt((dLp / sl) ** 2 + (dCp / sc) ** 2 + (dHp / sh) ** 2 + rt * (dCp / sc) * (dHp / sh))


# the port is checked against two of Sharma's published pairs before any run relies on it
for _p, _q, _want in (((50.0, 2.6772, -79.7751), (50.0, 0.0, -82.7485), 2.0425), ((50.0, 2.5, 0.0), (73.0, 25.0, -18.0), 27.1492)):
    if abs(ciede2000(_p, _q) - _want) > 1e-3:
        raise SystemExit(f"ciede2000 port drifted: {ciede2000(_p, _q)} vs {_want}")

# The film kind's screenshot matrix: an emulation name and how to enter it.
MATRIX = [
    ("none", {"media": [], "vision": "none"}),
    ("forced-light", {"media": [{"name": "forced-colors", "value": "active"}, {"name": "prefers-color-scheme", "value": "light"}], "vision": "none"}),
    ("forced-dark", {"media": [{"name": "forced-colors", "value": "active"}, {"name": "prefers-color-scheme", "value": "dark"}], "vision": "none"}),
    ("deuteranopia", {"media": [], "vision": "deuteranopia"}),
    ("protanopia", {"media": [], "vision": "protanopia"}),
    ("tritanopia", {"media": [], "vision": "tritanopia"}),
    ("achromatopsia", {"media": [], "vision": "achromatopsia"}),
]
MATRIX_RESET = {"media": [], "vision": "none"}  # no emulated media at all: the browser's own state
# The palette test holds CIEDE2000 8 under Machado's model, and Chrome's
# emulation, a separate implementation, renders the same floor (8.9 at its
# closest, tritanopia, on the toggle's flight film).
MIN_SERIES_SEPARATION = 8.0
FFMPEG_NOTED = False


# ----------------------------------------------------------------------------
# infrastructure
# ----------------------------------------------------------------------------
def free_port() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def find_chrome(explicit: str | None) -> str:
    for c in [explicit, "google-chrome", "google-chrome-stable", "chromium", "chromium-browser"]:
        if c and shutil.which(c):
            return shutil.which(c)
    sys.exit("no Chrome/Chromium binary found")


def find_shell(explicit: str | None) -> str | None:
    """The chrome-headless-shell the synthetic clock needs: the flag, the
    environment, then the usual download sites, or None."""
    for c in (explicit, os.environ.get("OP_HEADLESS_SHELL")):
        if c:
            return c
    for pattern in SHELL_GLOBS:
        hits = sorted(glob.glob(os.path.expanduser(pattern)))
        if hits:
            return hits[0]
    return None


def clock_note(clock: str) -> str:
    return f"Clock: synthetic, {FRAME_RATE} frames per second" if clock == CLOCK_SYNTHETIC else "Clock: real"


def binary_version(binary: str) -> str:
    try:
        return subprocess.run([binary, "--version"], capture_output=True, text=True, timeout=30).stdout.strip()
    except Exception:
        return "version unknown"


# A headless browser has no input device, so by default it answers (pointer: none)
# and (hover: none), and an element that gates a hover on those queries could never
# run its hovering branch here. These are Blink's own settings for what the device
# is rather than a CDP emulation, and they give every page a fine hovering pointer,
# the answers a mouse gives, in the shell and in Chrome's own headless alike.
# Emulation.setTouchEmulationEnabled still makes the device coarse on top of them,
# so the coarse path is driven the same way it always was.
POINTER = "--blink-settings=primaryPointerType=4,availablePointerTypes=4,primaryHoverType=2,availableHoverTypes=2"


class Browser:
    def __init__(self, chrome: str, workdir: Path, synthetic: bool = False):
        self.synthetic = synthetic
        self.port = free_port()
        self.profile = workdir / "profile"
        self.profile.mkdir(parents=True, exist_ok=True)
        log = (workdir / "chrome.log").open("wb")
        # the synthetic clock needs a shell that draws only when asked and
        # schedules nothing off a real clock; the threaded compositor paths are
        # off for the same reason
        mode = (["--headless", "--enable-begin-frame-control", "--deterministic-mode",
                 "--run-all-compositor-stages-before-draw", "--disable-new-content-rendering-timeout",
                 "--disable-threaded-animation", "--disable-threaded-scrolling",
                 "--disable-checker-imaging", "--disable-image-animation-resync"] if synthetic else
                ["--headless=new", "--disable-dev-shm-usage", "--no-first-run", "--no-default-browser-check"])
        self.proc = subprocess.Popen(
            [chrome, *mode, POINTER, "--disable-gpu", "--no-sandbox", "--hide-scrollbars",
             "--window-size=1280,900", f"--force-device-scale-factor={DPR}",
             f"--remote-debugging-port={self.port}", "--remote-allow-origins=*",
             f"--user-data-dir={self.profile}", "about:blank"],
            stdout=log, stderr=log)
        # CI runners can take well over ten seconds to bring Chrome up. Starting a
        # process is not page timing: this wait is on the real clock in both modes.
        deadline = time.time() + 90
        while True:
            try:
                if synthetic:
                    # the shell's own page target: /json/new is not offered here
                    targets = json.load(urllib.request.urlopen(f"http://127.0.0.1:{self.port}/json/list", timeout=2))
                    target = next(t for t in targets if t["type"] == "page")
                else:
                    urllib.request.urlopen(f"http://127.0.0.1:{self.port}/json/version", timeout=2).read()
                    target = json.load(urllib.request.urlopen(urllib.request.Request(
                        f"http://127.0.0.1:{self.port}/json/new?url=about:blank", method="PUT")))
                break
            except Exception:
                if self.proc.poll() is not None:
                    sys.exit(f"Chrome exited early (code {self.proc.returncode}); see {workdir / 'chrome.log'}")
                if time.time() > deadline:
                    sys.exit(f"Chrome did not open its DevTools port within 90s; see {workdir / 'chrome.log'}")
                time.sleep(0.3)
        self.ws = websocket.create_connection(target["webSocketDebuggerUrl"], timeout=60)
        self.mid = 0
        self.events: list[dict] = []
        # One thread owns the socket's receive side so screencast frames are
        # acknowledged the moment they arrive (Chrome sends the next frame only
        # after the acknowledgement) while the main thread samples the page.
        self.lock = threading.Lock()
        self.cond = threading.Condition(self.lock)
        self.replies: dict[int, dict] = {}
        self.closing = False
        self.cast_frames: list[tuple[float, bytes]] = []
        self.casting = False
        self.cast_frame0 = 0  # the frame a recording began at, which sets its cadence
        self.cast_rect = None
        self.cast_size = (1280, 900)  # the viewport in CSS px, as screencast metadata gives it
        # the synthetic clock's state: frames drawn, where the frame grid stands
        # on the virtual clock, the real ticks that clock started from, and the
        # last frame time sent
        self.frames = 0
        self.grid = 0.0
        self.tick0 = 0.0
        self.tick = 0.0
        self.reader = threading.Thread(target=self._read_loop, name="cdp-reader", daemon=True)
        self.reader.start()
        self.call("Page.enable")
        self.call("Runtime.enable")
        # Chrome's headless grants clipboard writes and the shell denies them, so
        # a control that copies would raise on one clock and not the other. The
        # page under test meets one permission state, stated here.
        self.call("Browser.grantPermissions", permissions=["clipboardReadWrite", "clipboardSanitizedWrite"])
        if self.synthetic:
            self.call("HeadlessExperimental.enable")
            self.call("Emulation.setVirtualTimePolicy", policy="pause", initialVirtualTime=VIRTUAL_EPOCH_MS / 1000)
            # the virtual clock now stands at the epoch and the renderer's ticks
            # stand here, so this is the base every frame time is measured from
            self.tick0 = time.monotonic() * 1000.0 + TICK_HEADROOM_MS
        self.clip_uses_page_coords = None
        self.seeded = None  # the theme seeding script, while a control has one installed

    def _read_loop(self):
        while not self.closing:
            try:
                raw = self.ws.recv()
            except websocket.WebSocketTimeoutException:
                continue
            except Exception:
                if self.closing:
                    return
                raise
            if not raw:
                if self.closing:
                    return
                continue
            try:
                msg = json.loads(raw)
            except ValueError:
                continue  # a closing handshake or a truncated frame at shutdown
            if "id" in msg:
                with self.cond:
                    self.replies[msg["id"]] = msg
                    self.cond.notify_all()
            elif msg.get("method") == "Page.screencastFrame":
                p = msg["params"]
                with self.lock:
                    if self.casting:
                        md = p["metadata"]
                        self.cast_frames.append((md["timestamp"], base64.b64decode(p["data"]), md["deviceWidth"], md["deviceHeight"]))
                    self.mid += 1
                    self.ws.send(json.dumps({"id": self.mid, "method": "Page.screencastFrameAck",
                                             "params": {"sessionId": p["sessionId"]}}))
            else:
                with self.cond:
                    self.events.append(msg)
                    self.cond.notify_all()

    # ---- the clock: real time, or the synthetic frame clock ----------------
    def now(self) -> float:
        """Seconds. Real: the wall clock. Synthetic: the page's own clock, which
        is the frame count times the interval and so exact by construction."""
        return self.frames * INTERVAL_MS / 1000.0 if self.synthetic else time.time()

    def wait(self, seconds: float):
        """Let `seconds` of page time pass: a real sleep, or whole frames, rounded up."""
        if not self.synthetic:
            time.sleep(seconds)
            return
        for _ in range(math.ceil(seconds * FRAME_RATE - 1e-9)):
            self.frame()

    def wait_event(self, method: str, timeout: float = 120):
        """The next event of this method, waited for on the real clock (it is the
        browser's answer, not the page's time)."""
        deadline = time.time() + timeout
        with self.cond:
            while True:
                for i, e in enumerate(self.events):
                    if e.get("method") == method:
                        del self.events[i]
                        return e
                remaining = deadline - time.time()
                if remaining <= 0:
                    raise RuntimeError(f"{method}: no event within {timeout}s")
                self.cond.wait(remaining)

    def advance(self, budget_ms: float, policy: str = "advance"):
        """Run the page's virtual clock for `budget_ms` and wait for it to expire."""
        with self.lock:
            self.events = [e for e in self.events if e.get("method") != "Emulation.virtualTimeBudgetExpired"]
        self.call("Emulation.setVirtualTimePolicy", policy=policy, budget=budget_ms,
                  maxVirtualTimeTaskStarvationCount=STARVATION)
        self.wait_event("Emulation.virtualTimeBudgetExpired")

    def virtual(self) -> float:
        """The page's virtual clock in milliseconds since the epoch it started
        at, read as a wall clock because that is one clock for the whole
        browser; performance.now() restarts at every document."""
        return self.js("performance.timeOrigin + performance.now()") - VIRTUAL_EPOCH_MS

    def frame(self, screenshot: dict | None = None):
        """One synthetic frame: the page's clock advances by the interval, then
        the frame is drawn and composited before this returns. With `screenshot`
        the result carries screenshotData. On the real clock the browser draws
        on its own and this does nothing.

        Two clocks meet here. The page's virtual clock is asked for exactly the
        time this frame is short of the grid, because a budget expires when the
        task that crosses it ends and so overshoots by a fraction of a
        millisecond: asking for the shortfall each time keeps the page on the
        frame's own clock instead of drifting a little further off it every
        frame, which is what makes two runs settle on the same frame.

        The frame's time is that grid, a little ahead of the virtual clock, in
        the renderer's own ticks. It has to lead: Blink pushes its animation
        clock forward to "now" whenever a script asks for a time-dependent style
        outside a frame (a computed style on an animated element, the document
        timeline), and it never lets that clock go back, so a frame stamped
        behind the virtual clock is ignored and every animation, transition and
        animation frame on the page stops. Stamped ahead of it, these frames are
        the only thing that moves the animation clock, and they move it by
        exactly one sixtieth of a second."""
        if not self.synthetic:
            return None
        self.grid += INTERVAL_MS
        short = self.grid - self.virtual()
        if short > 0:
            self.advance(short)
        self.tick = self.tick0 + self.grid + INTERVAL_MS
        want = screenshot
        # counted from the film's own first frame rather than from the browser's,
        # so which of two neighbouring frames a recording holds is the film's
        # business and not a consequence of whatever ran before it
        casting = self.casting and (self.frames + 1 - self.cast_frame0) % VIDEO_EVERY == 0
        if want is None and casting:
            want = {"format": "jpeg", "quality": CAST_QUALITY, "optimizeForSpeed": True}
        params = {"frameTimeTicks": self.tick, "interval": INTERVAL_MS}
        if want is not None:
            params["screenshot"] = want
        r = self.call("HeadlessExperimental.beginFrame", **params)
        self.frames += 1
        if screenshot is None and casting and r.get("screenshotData"):
            with self.lock:
                if self.casting:
                    self.cast_frames.append((self.now(), base64.b64decode(r["screenshotData"]), *self.cast_size))
        return r

    def cast_start(self, rect):
        """Start recording the viewport for a film; frames accumulate until cast_take.
        The moment it starts is the film's clock origin."""
        with self.lock:
            if self.casting:
                return
            self.casting = True
            self.cast_rect = rect
            self.cast_frames = []
            self.film_t0 = self.now()
            self.cast_frame0 = self.frames
        if self.synthetic:
            # the recording is taken frame by frame in frame(); its pictures are
            # the whole viewport at the device scale, as the screencast's are
            self.cast_size = tuple(self.js("[window.innerWidth, window.innerHeight]"))
            return
        self.call("Page.startScreencast", format="jpeg", quality=CAST_QUALITY,
                  maxWidth=int(1280 * DPR), maxHeight=int(900 * DPR), everyNthFrame=1)

    def film_start(self, rect) -> list:
        """Begin a film: start the recording and return its frame list, opened
        with a frame at time zero (the state before any input)."""
        self.cast_start(rect)
        return [(0.0, None)]

    def cast_take(self):
        """Stop recording and return (rect, [(timestamp, jpeg, deviceWidth, deviceHeight)]) for the film just made."""
        with self.lock:
            if not self.casting:
                return None, []
            self.casting = False
        if not self.synthetic:
            self.call("Page.stopScreencast")
        with self.lock:
            frames, rect = self.cast_frames, self.cast_rect
            self.cast_frames = []
        if frames and not getattr(self, "cast_noted", False):
            from PIL import Image
            import io
            im = Image.open(io.BytesIO(frames[0][1]))
            span = frames[-1][0] - frames[0][0]
            print(f"   (screencast {im.width}x{im.height} for a {frames[0][2]}x{frames[0][3]} viewport; {len(frames)} frames in {span:.1f}s)")
            self.cast_noted = True
        return rect, frames

    def close(self):
        self.closing = True
        try:
            self.ws.close()
        finally:
            self.proc.kill()
            try:
                # reaped here, so a worker that has run several controls leaves
                # no shell of its own behind
                self.proc.wait(timeout=30)
            except subprocess.TimeoutExpired:
                pass

    def call(self, method: str, **params):
        with self.cond:
            self.mid += 1
            mid = self.mid
            self.ws.send(json.dumps({"id": mid, "method": method, "params": params}))
        if self.synthetic and method.startswith("Input."):
            # An input costs the frame it is delivered in: nothing runs in the
            # page while its clock is paused, so that frame is the smallest unit
            # of page time there is and the one every check that reads "at once"
            # now means. The frame runs while the answer is still in flight: the
            # renderer handles the event in the frame's time and only answers
            # once it has, and it answers promptly once the frame is drawn.
            self.frame()
        msg = self.reply(mid, method)
        if "error" in msg:
            raise RuntimeError(f"{method}: {msg['error']}")
        return msg.get("result", {})

    def reply(self, mid: int, method: str) -> dict:
        with self.cond:
            deadline = time.time() + 120
            while mid not in self.replies:
                remaining = deadline - time.time()
                if remaining <= 0:
                    raise RuntimeError(f"{method}: no reply within 120s")
                self.cond.wait(remaining)
            return self.replies.pop(mid)

    def js(self, expr: str):
        r = self.call("Runtime.evaluate", expression=expr, returnByValue=True, awaitPromise=True)
        if "exceptionDetails" in r:
            ex = r["exceptionDetails"]
            raise RuntimeError(f"JS: {ex.get('exception', {}).get('description', ex.get('text'))} in {expr[:80]}")
        return r.get("result", {}).get("value")

    def seed_theme(self, mode: str | None):
        script = f"try{{localStorage.setItem({STORAGE_KEY!r},{mode!r})}}catch(e){{}}" if mode else \
                 f"try{{localStorage.removeItem({STORAGE_KEY!r})}}catch(e){{}}"
        self.seeded = self.call("Page.addScriptToEvaluateOnNewDocument", source=script)["identifier"]

    def unseed_theme(self):
        """Put the theme back as the browser found it: the seeding script off, and
        the choice the page stored gone. A seed belongs to the control that asked
        for it; left in place it would decide the theme of every control run after
        it on this browser, and so make their frames depend on the run's order."""
        if self.seeded is not None:
            self.call("Page.removeScriptToEvaluateOnNewDocument", identifier=self.seeded)
            self.seeded = None
        try:
            self.js(f"try{{localStorage.removeItem({STORAGE_KEY!r})}}catch(e){{}}; true")
        except RuntimeError:
            pass

    def reduced_motion(self, on: bool):
        self.call("Emulation.setEmulatedMedia",
                  features=[{"name": "prefers-reduced-motion", "value": "reduce" if on else "no-preference"}])
        self.frame()  # an emulation change reaches the page in the next frame

    def goto(self, url: str, ready: str, timeout: float = 25):
        # Mark the outgoing document so a same-URL navigation cannot pass
        # the readiness check on the old page (and install helpers there).
        try:
            self.js("window.__op_stale = true")
        except RuntimeError:
            pass
        self.call("Page.navigate", url=url)
        if self.synthetic:
            # loading spends virtual time only while nothing is being fetched,
            # and it is spent in one go: the frame grid resumes at the next whole
            # frame after that, so it stays a multiple of the interval and the
            # page's clock reads the same numbers in every run
            self.advance(LOAD_BUDGET_MS, policy="pauseIfNetworkFetchesPending")
            self.grid = (math.floor(self.virtual() / INTERVAL_MS + 0.5) + 1) * INTERVAL_MS
            for _ in range(int(timeout * FRAME_RATE)):
                self.frame()
                try:
                    if self.js(f"!window.__op_stale && ({ready})"):
                        self.js(HELPERS)
                        return
                except RuntimeError:
                    pass
            sys.exit(f"page never became ready: {url}")
        deadline = time.time() + timeout
        while time.time() < deadline:
            time.sleep(0.25)
            try:
                if self.js(f"!window.__op_stale && ({ready})"):
                    self.js(HELPERS)
                    return
            except RuntimeError:
                pass
        sys.exit(f"page never became ready: {url}")

    def hover(self, x, y):
        self.call("Input.dispatchMouseEvent", type="mouseMoved", x=x, y=y)

    def click(self, x, y):
        self.call("Input.dispatchMouseEvent", type="mousePressed", x=x, y=y, button="left", clickCount=1)
        self.call("Input.dispatchMouseEvent", type="mouseReleased", x=x, y=y, button="left", clickCount=1)

    def errors(self) -> list:
        with self.lock:
            return [e["params"] for e in self.events if e.get("method") == "Runtime.exceptionThrown"]

    def calibrate_clip(self):
        """Whether captureScreenshot clips are page- or viewport-relative here.
        Only the real clock needs it: under begin frame control the pictures come
        from the frames themselves, which are always viewport-relative."""
        self.js("""(() => { document.documentElement.style.minHeight='3000px'; window.scrollTo(0, 600);
          const m = document.createElement('div'); m.id='__op_mark';
          m.style.cssText='position:absolute;left:40px;top:640px;width:30px;height:30px;background:rgb(255,0,255)';
          document.body.appendChild(m); return true; })()""")
        time.sleep(0.15)
        from PIL import Image
        import io
        shot = self.call("Page.captureScreenshot", format="png",
                         clip={"x": 40, "y": 40, "width": 30, "height": 30, "scale": 1})
        im = Image.open(io.BytesIO(base64.b64decode(shot["data"]))).convert("RGB")
        px = im.getpixel((15, 15))
        self.clip_uses_page_coords = not (px[0] > 200 and px[1] < 60 and px[2] > 200)
        self.js("(() => { document.getElementById('__op_mark').remove(); document.documentElement.style.minHeight=''; window.scrollTo(0,0); return true; })()")

    def frame_bytes(self, rect, margin: float = 14, scale: float = 2) -> bytes:
        x, y, w, h = rect
        if self.synthetic:
            # captureScreenshot can hang under begin frame control, so the frame
            # carries the picture: one frame for whatever state change is pending,
            # then one that is drawn and read, cropped from the whole viewport
            from PIL import Image
            import io
            self.frame()
            shot = self.frame({"format": "png"})
            full = Image.open(io.BytesIO(base64.b64decode(shot["screenshotData"]))).convert("RGB")
            left = min(int(max(0.0, x - margin) * DPR), full.width - 1)
            top = min(int(max(0.0, y - margin) * DPR), full.height - 1)
            bw, bh = int(round((w + 2 * margin) * DPR)), int(round((h + 2 * margin) * DPR))
            crop = full.crop((left, top, max(left + 1, min(full.width, left + bw)), max(top + 1, min(full.height, top + bh))))
            # the box keeps its asked-for size where the viewport cut it short, padded
            # with the edge colour, so every frame of one film is the same shape
            im = Image.new("RGB", (max(1, bw), max(1, bh)), full.getpixel((min(full.width - 1, left), min(full.height - 1, top))))
            im.paste(crop, (0, 0))
            if abs(scale - DPR) > 1e-6:
                im = im.resize((max(1, round(im.width * scale / DPR)), max(1, round(im.height * scale / DPR))), Image.LANCZOS)
            buf = io.BytesIO()
            im.save(buf, format="PNG", optimize=True)
            return buf.getvalue()
        if self.clip_uses_page_coords:
            sx, sy = self.js("[window.scrollX, window.scrollY]")
            x, y = x + sx, y + sy
        shot = self.call("Page.captureScreenshot", format="png",
                         clip={"x": max(0, x - margin), "y": max(0, y - margin),
                               "width": w + 2 * margin, "height": h + 2 * margin, "scale": scale / DPR})
        return base64.b64decode(shot["data"])

    def save_frame(self, path: Path, rect, margin: float = 14, scale: float = 2):
        path.write_bytes(self.frame_bytes(rect, margin, scale))


HELPERS = r"""
window.__op = {
  find(tag, target) {
    const host = document.querySelector(tag); if (!host) return null;
    this.tag = tag; this.target = target;
    let el = null;
    if (target.host) el = host;
    else if (target.shadow) el = host.shadowRoot && host.shadowRoot.querySelector(target.shadow);
    else if (target.light) el = document.querySelector(target.light);
    else if (target.any) el = (host.shadowRoot && host.shadowRoot.querySelector(target.any)) || host.querySelector(target.any);
    if (!el) return null;
    el.scrollIntoView({block: 'center'});
    this.host = host; this.el = el;
    const r = el.getBoundingClientRect(), hr = host.getBoundingClientRect();
    return {x: r.x + r.width/2, y: r.y + r.height/2, rect: [r.x, r.y, r.width, r.height], host: [hr.x, hr.y, hr.width, hr.height]};
  },
  sig() {
    const roots = [this.host.shadowRoot, this.host].filter(Boolean); const out = [];
    for (const root of roots) for (const el of root.querySelectorAll('*')) {
      const s = getComputedStyle(el), r = el.getBoundingClientRect();
      out.push([el.tagName, s.display, s.visibility, s.opacity, s.backgroundColor, s.color, s.outlineWidth, s.outlineColor,
                s.borderColor, s.textDecorationLine, s.transform, s.boxShadow, Math.round(r.width), Math.round(r.height)].join(','));
    }
    const s = getComputedStyle(this.el);
    out.push(['SELF', s.backgroundColor, s.color, s.outlineWidth, s.outlineColor, s.borderColor, s.boxShadow, s.textDecorationLine].join(','));
    return out.join(';');
  },
  refind() { if (!this.el.isConnected && this.tag) { const l = this.find(this.tag, this.target); if (!l) return false; } return true; },
  attr(n) { this.refind(); return this.el.getAttribute(n); },
  hostQuery(sel) { const r = this.host.shadowRoot || this.host; const e = r.querySelector(sel); return e ? getComputedStyle(e).opacity + '|' + getComputedStyle(e).visibility : null; },
  detailsOpen() { this.refind(); const d = this.el.closest('details'); return d ? d.open : null; },
  checked() { this.refind(); return this.el.checked; },
  focusVisible() { this.el.focus({focusVisible: true}); const r = this.host.shadowRoot; return document.activeElement === this.host || document.activeElement === this.el || (r && r.activeElement === this.el); },
  blur() { this.el.blur(); document.body.focus(); return true; },
  holdLink() {
    // A real link would navigate away from the page under test; keep the
    // click (and its styling consequences) but not the navigation.
    const a = this.el.closest('a[href]');
    if (!a) return false;
    a.addEventListener('click', e => e.preventDefault(), { once: true });
    return true;
  },
  holdPopup() {
    // A select's list is a browser window rather than part of the page, and
    // while it is open the page draws no frames at all: on the synthetic clock,
    // where the frames are ours to draw, that stops everything. Keep the click
    // and its styling consequences, hold the popup.
    const s = this.el.closest('select');
    if (!s) return false;
    s.addEventListener('mousedown', e => e.preventDefault(), { once: true });
    return true;
  },
  bgProbe() {
    let d = document.getElementById('__op_bg');
    if (!d) { d = document.createElement('div'); d.id = '__op_bg'; d.style.cssText = 'position:fixed;left:-10px;top:-10px;width:4px;height:4px;background:var(--op-bg)'; document.body.appendChild(d); }
    return getComputedStyle(d).backgroundColor;
  },
  tstate() {
    const h = this.host, sr = h.shadowRoot, b = sr.querySelector('button'), bb = b.getBoundingClientRect();
    const st = n => h.matches(':state(' + n + ')'); const part = n => sr.querySelector('[part=' + n + ']');
    const px = el => el.getBoundingClientRect().x - bb.x;
    return { dark: st('dark'), attention: st('attention'), flight: st('flight'),
      thumb: px(part('thumb')), ghost: px(part('progress')), preview: px(part('preview')),
      preview_op: parseFloat(getComputedStyle(part('preview')).opacity), ghost_op: parseFloat(getComputedStyle(part('progress')).opacity),
      preview_anim: getComputedStyle(part('preview')).animationName,
      w: bb.width, bg: this.bgProbe(), title: b.getAttribute('title'), checked: b.getAttribute('aria-checked') };
  },
  sstate() {
    const i = this.el, b = getComputedStyle(i, '::before'), a = getComputedStyle(i, '::after'), r = i.getBoundingClientRect();
    return { checked: i.checked, thumb: parseFloat(b.left), preview: parseFloat(a.left), preview_op: parseFloat(a.opacity), anim: a.animationName, w: r.width };
  }
};
true"""


# ----------------------------------------------------------------------------
# report model
# ----------------------------------------------------------------------------
@dataclass
class Check:
    name: str
    ok: bool
    detail: str = ""


@dataclass
class Edge:
    key: tuple  # (from, input, to)
    title: str
    narrative: str
    frames: list = field(default_factory=list)  # (caption, relpath)
    curves: list = field(default_factory=list)  # (caption, relpath)
    checks: list = field(default_factory=list)
    note: str = ""
    matrix: list = field(default_factory=list)  # (emulation, relpath) screenshots of the chart
    matrix_title: str = "The chart under every emulation"
    film: dict | None = None  # {sheet, times, w, h, keys: [(caption, relpath)]}


@dataclass
class ControlReport:
    tag: str
    kind: str
    page: str
    clock: str = CLOCK_REAL
    edges: list = field(default_factory=list)
    machine_edges: list = field(default_factory=list)  # folded (from,input,to)
    nodes: list = field(default_factory=list)

    @property
    def checks(self):
        return [c for e in self.edges for c in e.checks]


def secs(t: float | None) -> str:
    """A measured time for a check's detail, at a hundredth of a second: the
    resolution every window in this file is written at, and coarse enough that
    one frame of difference between two runs does not change the wording."""
    return "never" if t is None else f"{t:.2f}s"


def green(bg: str) -> int:
    return int(bg.split("(")[1].split(",")[1])


# ----------------------------------------------------------------------------
# machine diagram
# ----------------------------------------------------------------------------
def machine_svg(nodes: list[str], edges: list[tuple], highlight: tuple | None) -> str:
    n = len(nodes)
    width = 240 * n + 60 if n > 1 else 360
    pos = {name: (150 + i * 240, 120) for i, name in enumerate(nodes)}
    if n == 1:
        pos[nodes[0]] = (180, 120)
    R = 36
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} 250" width="{width}" height="250" role="img" font-family="system-ui, sans-serif" font-size="13">',
           '<defs><marker id="a" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto-start-reverse"><path d="M0 0L10 5L0 10z" fill="#888"/></marker>'
           '<marker id="ah" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto-start-reverse"><path d="M0 0L10 5L0 10z" fill="#D55E00"/></marker></defs>']
    # group edges by (from,to) to stack self-loop / parallel labels
    loops: dict[str, list[str]] = {}
    for f, i, t in edges:
        if f == t:
            loops.setdefault(f, []).append(i)
    drawn_pairs: dict[tuple, int] = {}
    for f, i, t in edges:
        hl = highlight == (f, i, t)
        stroke = OKABE["mark"] if hl else "#888"
        sw = 3 if hl else 1.3
        marker = "ah" if hl else "a"
        if f == t:
            continue
        (x1, y1), (x2, y2) = pos[f], pos[t]
        k = drawn_pairs.get((f, t), 0)
        drawn_pairs[(f, t)] = k + 1
        forward = pos[t][0] > pos[f][0]
        adjacent = abs(nodes.index(f) - nodes.index(t)) == 1
        if forward and adjacent:
            path = f"M{x1 + R} {y1} L{x2 - R} {y2}"
            lx, ly = (x1 + x2) / 2, y1 - 10
        else:
            depth = 70 if adjacent else 105
            cy = y1 + depth
            sx, ex = (x1 - 8, x2 + 8) if not forward else (x1 + 8, x2 - 8)
            path = f"M{sx} {y1 + R} Q{(x1 + x2) / 2} {cy + 40} {ex} {y2 + R}"
            # label at the quadratic curve's midpoint (t = 0.5)
            lx = (sx + 2 * (x1 + x2) / 2 + ex) / 4
            ly = (y1 + R + 2 * (cy + 40) + y2 + R) / 4 + 4
        out.append(f'<path d="{path}" fill="none" stroke="{stroke}" stroke-width="{sw}" marker-end="url(#{marker})"/>')
        out.append(f'<text x="{lx}" y="{ly}" text-anchor="middle" fill="{stroke}" font-weight="{"bold" if hl else "normal"}" paint-order="stroke" stroke="#fafafa" stroke-width="5">{i}</text>')
    for name, (x, y) in pos.items():
        involved = highlight and name in (highlight[0], highlight[2])
        fill = "#FDE9DC" if involved else "#fff"
        stroke = OKABE["mark"] if involved else "#555"
        out.append(f'<circle cx="{x}" cy="{y}" r="{R}" fill="{fill}" stroke="{stroke}" stroke-width="{2 if involved else 1.3}"/>')
        out.append(f'<text x="{x}" y="{y + 5}" text-anchor="middle" font-weight="600">{name}</text>')
        if name in loops:
            hl_loop = highlight and highlight[0] == name and highlight[2] == name
            hl_in = highlight[1] if hl_loop else None
            stroke = OKABE["mark"] if hl_loop else "#888"
            out.append(f'<path d="M{x - 14} {y - R + 2} C{x - 40} {y - 90} {x + 40} {y - 90} {x + 14} {y - R + 2}" fill="none" stroke="{stroke}" stroke-width="{3 if hl_loop else 1.3}" marker-end="url(#{"ah" if hl_loop else "a"})"/>')
            labels = " / ".join(f"<tspan font-weight=\"bold\" fill=\"{OKABE['mark']}\">{i}</tspan>" if i == hl_in else i for i in loops[name])
            out.append(f'<text x="{x}" y="{y - 82}" text-anchor="middle" fill="#555">{labels}</text>')
    out.append("</svg>")
    return "\n".join(out)


# ----------------------------------------------------------------------------
# charts: inline SVG with a playhead, sharing the film's clock
# ----------------------------------------------------------------------------
CHART_W, CHART_H, ML, MR, MT, MB = 900, 268, 46, 14, 16, 48


def chart_svg(series: list[dict], t_max: float, marks=(), ylabel: str = "progress %", ymin=-4, ymax=106) -> str:
    """Series: {label, color, t: [..], y: [..], dash?: bool}. The SVG
    carries a head line the player moves; data-x0/x1/t1 map time to px."""
    x_of = lambda t: ML + (t / t_max) * (CHART_W - ML - MR)  # noqa: E731
    y_of = lambda v: MT + (ymax - v) / (ymax - ymin) * (CHART_H - MT - MB)  # noqa: E731
    out = [f'<svg class="chart" viewBox="0 0 {CHART_W} {CHART_H}" width="{CHART_W}" height="{CHART_H}" data-x0="{ML}" data-x1="{CHART_W - MR}" data-t1="{t_max:.3f}" font-family="system-ui, sans-serif" font-size="11" role="img">']
    for v in (0, 25, 50, 75, 100):
        y = y_of(v)
        out.append(f'<line x1="{ML}" x2="{CHART_W - MR}" y1="{y:.1f}" y2="{y:.1f}" stroke="#ddd" stroke-width="{1 if v in (0, 100) else 0.5}"/>')
        out.append(f'<text x="{ML - 6}" y="{y + 4:.1f}" text-anchor="end" fill="#666">{v}</text>')
    step = 0.5 if t_max <= 5 else 1.0
    t = 0.0
    while t <= t_max + 1e-9:
        x = x_of(t)
        out.append(f'<line x1="{x:.1f}" x2="{x:.1f}" y1="{CHART_H - MB}" y2="{CHART_H - MB + 4}" stroke="#888"/>')
        out.append(f'<text x="{x:.1f}" y="{CHART_H - MB + 16}" text-anchor="middle" fill="#666">{t:g}s</text>')
        t += step
    out.append(f'<text x="14" y="{(CHART_H - MB + MT) / 2:.1f}" transform="rotate(-90 14 {(CHART_H - MB + MT) / 2:.1f})" text-anchor="middle" fill="#666">{ylabel}</text>')
    for tm, label in marks:
        x = x_of(tm)
        out.append(f'<line x1="{x:.1f}" x2="{x:.1f}" y1="{MT}" y2="{CHART_H - MB}" stroke="{OKABE["mark"]}" stroke-dasharray="3 3"/>')
        out.append(f'<text x="{x + 4:.1f}" y="{MT + 10}" fill="{OKABE["mark"]}">{label}</text>')
    for sr in series:
        pts = " ".join(f"{x_of(t):.1f},{y_of(max(ymin, min(ymax, v))):.1f}" for t, v in zip(sr["t"], sr["y"]))
        dash = ' stroke-dasharray="5 4"' if sr.get("dash") else ""
        colour = series_hex()[sr["series"]]
        out.append(f'<polyline points="{pts}" fill="none" stroke="{colour}" stroke-width="{sr.get("lw", 1.8)}"{dash} stroke-linejoin="round"/>')
        if sr.get("label") and sr["t"]:
            i = min(len(sr["t"]) - 1, int(len(sr["t"]) * sr.get("at", 0.85)))
            out.append(f'<text x="{x_of(sr["t"][i]) + 4:.1f}" y="{y_of(sr["y"][i]) - 5:.1f}" fill="{colour}" font-weight="700" paint-order="stroke" stroke="#fff" stroke-width="4">{sr["label"]}</text>')
    # a YouTube-style bar under the axis: played portion, chapter dividers
    # (the machine's marks), and a band for the hovered chapter
    by = CHART_H - 10
    out.append(f'<rect class="band" x="{ML}" y="{MT}" width="0" height="{CHART_H - MB - MT}" fill="{OKABE["mark"]}" opacity="0.07"/>')
    out.append(f'<rect class="bar-bg" x="{ML}" y="{by}" width="{CHART_W - ML - MR}" height="4" rx="2" fill="#ddd"/>')
    out.append(f'<rect class="bar-played" x="{ML}" y="{by}" width="0" height="4" rx="2" fill="{OKABE["mark"]}"/>')
    for tm, _label in marks:
        x = x_of(tm)
        out.append(f'<rect class="chapter" x="{x - 1:.1f}" y="{by - 3}" width="2" height="10" fill="#fff" stroke="#999" stroke-width="0.6"/>')
    out.append(f'<line class="peek-line" x1="{ML}" x2="{ML}" y1="{MT}" y2="{CHART_H - MB}" stroke="#555" stroke-width="1" stroke-dasharray="3 3" visibility="hidden"/>')
    out.append(f'<line class="head" x1="{ML}" x2="{ML}" y1="{MT}" y2="{by + 4}" stroke="{OKABE["mark"]}" stroke-width="1.5"/>')
    out.append(f'<circle class="head-dot" cx="{ML}" cy="{by + 2}" r="5" fill="{OKABE["mark"]}"/>')
    out.append(f'<text class="head-t" x="{ML + 4}" y="{CHART_H - MB - 6}" fill="{OKABE["mark"]}" font-weight="700" paint-order="stroke" stroke="#fff" stroke-width="4">0.00s</text>')
    out.append("</svg>")
    return "\n".join(out)


# ----------------------------------------------------------------------------
# kinds
# ----------------------------------------------------------------------------
# A window's last frame lands exactly on its bound (0.6 s is thirty-six frames),
# and the page's clock is a count of frames read as seconds, so the two ends of
# the window carry the rounding of however many frames the browser had already
# drawn: without this guard the same window holds thirty-six frames or
# thirty-seven depending on what ran before it. The guard is a millionth of a
# frame, far below anything a window means to include.
EDGE = 1e-9


def sample(b: Browser, expr: str, seconds: float, period: float = 0.05, until=None, t0: float | None = None,
           film: list | None = None, rect=None, every: int = 1, scale: float = 2):
    """Samples `expr` for `seconds`; with `film`, also captures a clipped
    frame every `every`-th sample so frames and curves share one clock. On the
    synthetic clock every frame is one sample, at an exact time, and `period`
    does not apply: rows, sheet frames and the recording share that clock."""
    start = b.now()
    t0 = t0 if t0 is not None else start
    if film is not None and rect is not None:
        b.cast_start(rect)  # rows, sheet frames and the recording share one origin
    rows = []
    i = 0
    last_frame = -1.0
    while b.now() - start < seconds - EDGE:
        s = b.js(expr)
        t = round(b.now() - t0, 3)
        rows.append((t, s))
        # a sheet frame wanted at this time; cut from the recording later, so
        # the loop never waits on a screenshot and the curves stay dense
        if film is not None and i % every == 0 and t - last_frame >= SHEET_PERIOD:
            film.append((t, None))
            last_frame = t
        i += 1
        if until and until(s):
            break
        if b.synthetic:
            b.frame()
        else:
            time.sleep(period)
    return rows


def burst(b: Browser, rect, seconds: float, fps: float = 15, t0: float | None = None, scale: float = 1.5) -> list:
    """A short frame sequence with no sampling in between. On the synthetic clock
    it steps frame by frame and keeps `fps` as the sheet's cadence, so a burst
    holds the same frames on either clock."""
    start = b.now()
    t0 = t0 if t0 is not None else start
    b.cast_start(rect)
    film = []
    last = -1.0
    while b.now() - start < seconds - EDGE:
        t = round(b.now() - t0, 3)
        if t - last >= 1 / fps - EDGE:
            film.append((t, None))
            last = t
        if b.synthetic:
            b.frame()
        else:
            time.sleep(1 / fps)
    return film


def crop_frame(img, rect, dev_w: float, dev_h: float, cell: tuple, margin: float = CAST_MARGIN):
    """The element's region of a screencast frame, framed like the sheet: the
    same margin, scaled from the frame's own size, padded with the edge colour
    where the viewport clipped it, and resized to the sheet cell."""
    from PIL import Image
    x, y, w, h = rect
    sx, sy = img.width / dev_w, img.height / dev_h
    left, top = (x - margin) * sx, (y - margin) * sy
    right, bottom = (x + w + margin) * sx, (y + h + margin) * sy
    bw, bh = max(2, int(round(right - left))), max(2, int(round(bottom - top)))
    edge = img.getpixel((min(img.width - 1, max(0, int(left))), min(img.height - 1, max(0, int(top)))))
    canvas = Image.new("RGB", (bw, bh), edge)
    cl, ct = max(0, int(left)), max(0, int(top))
    cr, cb = min(img.width, int(right)), min(img.height, int(bottom))
    if cr > cl and cb > ct:
        canvas.paste(img.crop((cl, ct, cr, cb)), (cl - int(left), ct - int(top)))
    return canvas.resize(cell, Image.LANCZOS)


def sheet_cell(rect) -> tuple:
    """The sheet cell for an element: its box plus the margin, at the device scale, even-sized."""
    _, _, w, h = rect
    cw, ch = int(round((w + 2 * CAST_MARGIN) * DPR)), int(round((h + 2 * CAST_MARGIN) * DPR))
    return (cw - cw % 2, ch - ch % 2)


def nearest_cast_frame(cast: list, origin: float, t: float):
    """The screencast frame closest to film time `t`, or None when none is within reach."""
    best, best_d = None, 0.35
    for entry in cast:
        d = abs(entry[0] - origin - t)
        if d < best_d:
            best, best_d = entry, d
    return best


def changed_fraction_images(a, b_) -> float:
    """The share of pixels whose colour moved by more than the tolerance."""
    from PIL import ImageChops
    diff = ImageChops.difference(a.convert("RGB"), b_.convert("RGB")).convert("L").point(lambda v: 255 if v > CHANGE_TOLERANCE else 0)
    changed = sum(diff.histogram()[255:])
    return round(changed / (a.width * a.height), 4)


def make_film(edge: Edge, frames: list, d: Path, name: str, keys: int = 8, series=None, marks=(), ylabel="progress %", title="", chapter0="start", trace=(), t0: float | None = None):
    """Stitches (t, png) frames into a horizontal sprite sheet with a
    timestamp table and, per frame, the fraction of pixels that changed
    since the previous frame - shown as the reel's captions, so a frame
    that did not change says so instead of merely looking repeated."""
    from PIL import Image
    import io
    if not frames:
        return
    rect, cast = RECORDER.cast_take() if RECORDER is not None else (None, [])
    cell = sheet_cell(rect) if rect is not None else None
    # frames captured as screenshots come with their bytes; frames wanted at a
    # time are cut from the screencast, nearest frame to that time
    origin = t0 if t0 is not None else (RECORDER.film_t0 if (RECORDER is not None and cast) else 0.0)
    images = []
    times = []
    for t, png in frames:
        if png is not None:
            im = Image.open(io.BytesIO(png)).convert("RGB")
        else:
            entry = nearest_cast_frame(cast, origin, t)
            if entry is None:
                continue
            ts, jpeg, dev_w, dev_h = entry
            im = crop_frame(Image.open(io.BytesIO(jpeg)).convert("RGB"), rect, dev_w, dev_h, cell)
        images.append(im)
        times.append(t)
    if not images:
        print("   (film skipped: no frames within reach of the screencast)")
        return
    w, h = images[0].size
    images = [im if im.size == (w, h) else im.resize((w, h)) for im in images]
    sheet = Image.new("RGB", (w * len(images), h), "#ffffff")
    for i, im in enumerate(images):
        sheet.paste(im, (i * w, 0))
    sheet.save(d / f"{name}-film.png", optimize=True)
    deltas = [0.0] + [changed_fraction_images(images[i - 1], images[i]) for i in range(1, len(images))]
    film = {"sheet": f"{name}-film.png", "times": times, "deltas": deltas, "w": w, "h": h, "title": title,
            "chapters": [[0.0, chapter0]] + [[float(t), label] for t, label in marks],
            "series": [{k: v for k, v in sr.items() if k in ("label", "series", "t", "y", "dash", "lw", "at")} for sr in (series or [])],
            "ylabel": ylabel,
            "trace": [[round(float(t), 3), a, i, b] for t, a, i, b in trace]}
    if series:
        t_max = max(max(times), max(max(sr["t"]) for sr in series if sr["t"]))
        film["chart"] = chart_svg(series, t_max, marks, ylabel)
    if cast:
        video = write_video(cast, rect, origin, times, d, name, (w, h))
        if video:
            film["video"] = video
    edge.film = film


def write_video(cast: list, rect, origin: float, times: list, d: Path, name: str, cell: tuple):
    """Cut the screencast to the film's element, framed exactly like the sheet
    (same margin, resized to the sheet's cell), and encode it as VP9 WebM at
    the frames' own timestamps (variable frame rate), aligned to the sheet's
    clock. Returns the file name, or None when there is nothing to encode."""
    if not shutil.which("ffmpeg"):
        global FFMPEG_NOTED
        if not FFMPEG_NOTED:
            print("   (no ffmpeg on PATH: films keep their sheets but carry no recording)")
            FFMPEG_NOTED = True
        return None
    if len(cast) < 2 or rect is None:
        return None
    from PIL import Image
    import io
    cell_w, cell_h = cell
    even = (cell_w - cell_w % 2, cell_h - cell_h % 2)
    end = times[-1] + 0.25
    with tempfile.TemporaryDirectory(prefix="op-film-") as tmp:
        tmp = Path(tmp)
        kept = []
        for i, (ts, jpeg, dev_w, dev_h) in enumerate(cast):
            t = ts - origin
            if t < -0.05 or t > end:
                continue
            im = crop_frame(Image.open(io.BytesIO(jpeg)).convert("RGB"), rect, dev_w, dev_h, even)
            path = tmp / f"f{i:05d}.png"
            im.save(path)
            kept.append((max(0.0, t), path))
        if len(kept) < 2:
            return None
        lines = []
        for k, (t, path) in enumerate(kept):
            nxt = kept[k + 1][0] if k + 1 < len(kept) else end
            lines.append(f"file '{path}'\nduration {max(0.001, nxt - t):.4f}")
        lines.append(f"file '{kept[-1][1]}'")
        (tmp / "list.txt").write_text("\n".join(lines) + "\n")
        out = d / f"{name}-film.webm"
        cmd = ["ffmpeg", "-y", "-loglevel", "error", "-f", "concat", "-safe", "0", "-i", str(tmp / "list.txt"),
               "-vsync", "vfr", "-pix_fmt", "yuv420p", "-c:v", "libvpx-vp9", "-crf", "30", "-b:v", "0",
               "-row-mt", "1", "-cpu-used", "4", str(out)]
        r = subprocess.run(cmd, capture_output=True, text=True)
        if r.returncode != 0:
            print(f"   (video skipped: ffmpeg failed: {r.stderr.strip()[:200]})")
            return None
    return out.name


def run_toggle(b: Browser, base: str, ctrl: dict, out: Path, machine: list) -> ControlReport:
    tag = ctrl["tag"]
    rep = ControlReport(tag=tag, kind="toggle", page=ctrl["page"],
                        nodes=["Idle", "Toward", "Back"],
                        machine_edges=sorted({(r["from"]["flight"], r["input"], r["to"]["flight"]) for r in machine}))
    d = out / tag
    d.mkdir(parents=True, exist_ok=True)
    b.reduced_motion(False)
    b.seed_theme(ctrl.get("seed_theme"))
    ready = f"!!document.querySelector('{tag}')?.shadowRoot?.querySelector('button') && !!document.querySelector('{tag}').matches(':state(dark)')"
    b.goto(base + ctrl["page"], ready)
    loc = b.js(f"window.__op.find({tag!r}, {json.dumps(ctrl['target'])})")
    rect = loc["rect"]
    T = "window.__op.tstate()"
    ST = lambda: b.js(T)  # noqa: E731
    x, y = loc["x"], loc["y"]
    ideal_t = [i / 100 * 3 for i in range(101)]
    ideal = [(2 ** (6 * t / 3) - 1) / 63 * 100 for t in ideal_t]

    s0 = ST()
    thumb_on, g_on = s0["thumb"], green(s0["bg"])

    # ---- E1: Idle --Attend--> Idle : preview loop ----
    e1 = Edge(("Idle", "Attend", "Idle"), "Attend: the pointer arrives",
              "Attention becomes a custom state on the host; while the machine is idle the preview ghost loops toward the destination.")
    film = []
    b.hover(x, y)
    rows = sample(b, T, 2.0, 0.04, film=film, rect=rect)
    ts = [t for t, _ in rows]
    make_film(e1, film, d, "attend", keys=6, ylabel="% (opacity, left)", chapter0="preview loop", trace=[(0.0, "Idle", "Attend", "Idle")],
              series=[{"label": "preview opacity", "series": SERIES["preview"], "t": ts, "y": [s["preview_op"] * 100 for _, s in rows], "lw": 2.4},
                      {"label": "preview left (% of track)", "series": SERIES["ghost"], "t": ts, "y": [s["preview"] / s["w"] * 100 for _, s in rows], "at": 0.5}])
    at = next((t for t, s in rows if s["attention"]), None)
    peak = max(s["preview_op"] for _, s in rows)
    e1.checks += [Check("attention custom state set", at is not None and at < 0.3, f"at {secs(at)}"),
                  Check("preview reaches legible opacity", peak >= 0.8, f"peak {peak}"),
                  Check("no flight while merely attended", not any(s["flight"] for _, s in rows))]
    rep.edges.append(e1)

    # ---- E2: Idle --Activate--> Toward : flight ----
    e2 = Edge(("Idle", "Activate", "Toward"), "Activate: a hovered click starts the flight",
              "The setting flips at once (solid thumb, aria-checked, stored choice); the palette blend and the progress ghost run on the blend clock; the preview is gated off.")
    film = []
    b.click(x, y)
    t0 = b.now()
    first = ST()
    rows = sample(b, T, 3.7, 0.05, t0=t0, film=film, rect=rect)
    settled = next((t for t, s in rows if not s["flight"]), None)
    thumb_off = rows[-1][1]["thumb"]
    span = abs(thumb_on - thumb_off)
    g_off = green(rows[-1][1]["bg"])
    ts = [t for t, _ in rows]
    ghost = [abs(s["ghost"] - thumb_on) / span * 100 for _, s in rows]
    pal = [(green(s["bg"]) - g_on) / (g_off - g_on) * 100 for _, s in rows]
    thumb = [abs(s["thumb"] - thumb_on) / span * 100 for _, s in rows]
    gap = max(abs(a - p) for a, p in zip(ghost, pal))
    make_film(e2, film, d, "flight", keys=8, title=f"max ghost-palette gap {gap:.1f} pts", chapter0="flight",
              trace=[(0.0, "Idle", "Activate", "Toward")] + ([(settled, "Toward", "Finished", "Idle")] if settled else []),
              series=[{"label": "ideal exponential", "series": SERIES["ideal"], "t": ideal_t, "y": ideal, "lw": 1, "dash": True, "at": 0.68},
                      {"label": "solid thumb", "series": SERIES["thumb"], "t": ts, "y": thumb, "at": 0.06},
                      {"label": "palette", "series": SERIES["palette"], "t": ts, "y": pal, "lw": 2.6, "at": 0.8},
                      {"label": "progress ghost", "series": SERIES["ghost"], "t": ts, "y": ghost, "at": 0.9},
                      {"label": "preview opacity", "series": SERIES["preview"], "t": ts, "y": [s["preview_op"] * 100 for _, s in rows], "lw": 1, "dash": True, "at": 0.3}])
    early = [s["preview_op"] for t, s in rows if t < 2.0]
    e2.checks += [Check("flight custom state armed on click", first["flight"]),
                  Check("setting flipped at once", first["dark"] is False and first["checked"] == "false"),
                  Check("description names the switch and the way back", "switching" in (first["title"] or "").lower()),
                  Check("preview gated off during flight", max(early) < 0.05, "max opacity in the first 2s"),
                  Check("ghost tracks the palette", gap < 5, f"max gap {gap:.2f} pts"),
                  Check("solid thumb settles within the snap clock", next((v for t, v in zip(ts, thumb) if t >= 0.3), thumb[-1]) > 85, "position at 0.3s")]
    rep.edges.append(e2)

    # ---- E3: Toward --Finished--> Idle ----
    e3 = Edge(("Toward", "Finished", "Idle"), "Finished: the blend's own completion settles the machine",
              "No timer: the palette transitions' finished promises on <html> resolve, the flight state clears, and the preview resumes because the pointer is still there.")
    film = []
    after = sample(b, T, 1.9, 0.05, film=film, rect=rect)
    ts = [t for t, _ in after]
    make_film(e3, film, d, "settle", keys=5, ylabel="% (opacity)", trace=[],
              series=[{"label": "preview opacity (resuming)", "series": SERIES["preview"], "t": ts, "y": [s["preview_op"] * 100 for _, s in after], "lw": 2.4, "at": 0.5}])
    resume = max(s["preview_op"] for _, s in after)
    e3.checks += [Check("settled when the blend ended (2.8-3.4s)", settled is not None and 2.8 <= settled <= 3.4, f"flight cleared at {secs(settled)}"),
                  Check("palette arrived", green(rows[-1][1]["bg"]) == g_off and abs(g_off - g_on) > 100),
                  Check("preview resumes under the resting pointer", resume >= 0.8, f"peak {resume} within 1.9s")]
    rep.edges.append(e3)

    # ---- fly back so the abort below starts from dark, as the page began ----
    b.click(x, y)
    sample(b, T, 3.6, 0.1, until=lambda s_: not s_["flight"] and s_["dark"])
    b.wait(0.3)

    # ---- E4/E5: Toward --Activate--> Back --Finished--> Idle : abort ----
    e4 = Edge(("Toward", "Activate", "Back"), "Activate mid-flight: abort",
              "The setting returns at once and the armed clocks reverse; CSS shortens the reversal in proportion to how far it had got.")
    before = ST()
    film = []
    b.click(x, y)
    t0 = b.now()
    rows = sample(b, T, 1.2, 0.05, t0=t0, film=film, rect=rect)
    b.click(x, y)
    t_abort = b.now() - t0
    rows += sample(b, T, 2.4, 0.05, t0=t0, film=film, rect=rect)
    cleared = next((t for t, s in rows if t > t_abort and not s["flight"]), None)
    origin_dark = before["dark"]
    ts = [t for t, _ in rows]
    g_from = green(rows[0][1]["bg"])
    g_to = g_off if origin_dark else g_on
    pal = [(green(s["bg"]) - g_from) / (g_to - g_from) * 100 for _, s in rows]
    origin_x = thumb_on if origin_dark else thumb_off
    ghost = [abs(s["ghost"] - origin_x) / span * 100 for _, s in rows]
    thumb = [abs(s["thumb"] - origin_x) / span * 100 for _, s in rows]
    make_film(e4, film, d, "abort", keys=8, marks=[(t_abort, "abort")], chapter0="flight",
              trace=[(0.0, "Idle", "Activate", "Toward"), (t_abort, "Toward", "Activate", "Back")] + ([(cleared, "Back", "Finished", "Idle")] if cleared else []),
              series=[{"label": "solid thumb", "series": SERIES["thumb"], "t": ts, "y": thumb, "at": 0.2},
                      {"label": "palette", "series": SERIES["palette"], "t": ts, "y": pal, "lw": 2.6, "at": 0.3},
                      {"label": "progress ghost", "series": SERIES["ghost"], "t": ts, "y": ghost, "at": 0.25}])
    first_after = next((s for t, s in rows if t > t_abort), None)
    e4.checks += [Check("setting restored immediately on abort", first_after is not None and first_after["dark"] == origin_dark),
                  Check("flight stays armed for the reversal", bool(first_after and first_after["flight"]), "first sample after the abort click")]
    rep.edges.append(e4)
    e5 = Edge(("Back", "Finished", "Idle"), "Finished after the reversal", "The reversed transitions complete early; their finished promises settle the machine within a fraction of the full blend.")
    e5.checks += [Check("settled soon after the abort", cleared is not None and cleared - t_abort < 1.0, f"cleared {cleared - t_abort:.2f}s after the abort" if cleared else "never cleared"),
                  Check("palette back at the origin", abs(green(rows[-1][1]["bg"]) - g_from) < 6)]
    rep.edges.append(e5)

    # ---- E6: Back --Activate--> Toward : re-fly ----
    e6 = Edge(("Back", "Activate", "Toward"), "Activate during the reversal: fly again",
              "A third click starts a fresh flight from wherever the reversal had got to; the machine goes Toward again with the opposite setting.")
    before = ST()
    film = []
    b.click(x, y); t0 = b.now()
    rows = sample(b, T, 0.8, 0.05, t0=t0, film=film, rect=rect)
    b.click(x, y); t_ab = b.now() - t0; b.wait(0.12)
    b.click(x, y); t_re = b.now() - t0
    s_re = ST()
    rows += sample(b, T, 3.8, 0.05, t0=t0, film=film, rect=rect)
    settled2 = next((t for t, s in rows if t > t_re and not s["flight"]), None)
    ts = [t for t, _ in rows]
    origin_x = thumb_on if before["dark"] else thumb_off
    ghost = [abs(s["ghost"] - origin_x) / span * 100 for _, s in rows]
    g_from = g_on if before["dark"] else g_off
    g_to = g_off if before["dark"] else g_on
    pal = [(green(s["bg"]) - g_from) / (g_to - g_from) * 100 for _, s in rows]
    make_film(e6, film, d, "refly", keys=8, marks=[(t_ab, "abort"), (t_re, "fly again")], chapter0="flight",
              trace=[(0.0, "Idle", "Activate", "Toward"), (t_ab, "Toward", "Activate", "Back"), (t_re, "Back", "Activate", "Toward")] + ([(settled2, "Toward", "Finished", "Idle")] if settled2 else []),
              series=[{"label": "palette", "series": SERIES["palette"], "t": ts, "y": pal, "lw": 2.6, "at": 0.8},
                      {"label": "progress ghost", "series": SERIES["ghost"], "t": ts, "y": ghost, "at": 0.9}])
    e6.checks += [Check("third click re-arms toward the opposite setting", s_re["flight"] and s_re["dark"] != before["dark"]),
                  Check("the new flight settles on its own clock", settled2 is not None and settled2 - t_re < 3.6, f"{settled2 - t_re:.2f}s after the third click" if settled2 else "never")]
    rep.edges.append(e6)

    # ---- E7: Neglect ----
    e7 = Edge(("Idle", "Neglect", "Idle"), "Neglect: the pointer leaves", "Attention clears; the preview stops.")
    film = b.film_start(rect)
    t0 = b.now()
    b.hover(2, 2)
    rows = sample(b, T, 0.8, 0.05, t0=t0, film=film, rect=rect)
    ts = [t for t, _ in rows]
    make_film(e7, film, d, "neglect", keys=4, ylabel="% (opacity)", trace=[(0.0, "Idle", "Neglect", "Idle")],
              series=[{"label": "preview opacity", "series": SERIES["preview"], "t": ts, "y": [s["preview_op"] * 100 for _, s in rows], "lw": 2.4, "at": 0.5}])
    gone = next((t for t, s in rows if not s["attention"]), None)
    e7.checks += [Check("attention custom state cleared", gone is not None and gone < 0.3, f"at {secs(gone)}"),
                  Check("preview hidden once unattended", rows[-1][1]["preview_op"] < 0.05, f"opacity {rows[-1][1]['preview_op']}")]
    rep.edges.append(e7)

    # ---- E8: reduced motion (same machine, different representation) ----
    e8 = Edge(("Idle", "Attend", "Idle"), "Reduced motion: the same edges, static representation",
              "prefers-reduced-motion collapses the snap token and disables the preview loop; the preview appears statically at the destination, and a click still blends the palette (a colour fade is not motion) while the ghost snaps.")
    b.reduced_motion(True)
    b.wait(0.2)
    film = b.film_start(rect)
    t0 = b.now()
    b.hover(x, y); b.wait(0.4)
    s_rm = ST()
    film.append((round(b.now() - t0, 3), None))
    t_click = b.now() - t0
    b.click(x, y)
    rows = sample(b, T, 3.6, 0.05, t0=t0, film=film, rect=rect)
    ts = [t for t, _ in rows]
    g_from = green(rows[0][1]["bg"]); g_to = green(rows[-1][1]["bg"])
    settle_rm = next((t for t, s_ in rows if not s_["flight"]), None)
    make_film(e8, film, d, "reduced", keys=6, marks=[(t_click, "click")], chapter0="hover",
              trace=[(0.0, "Idle", "Attend", "Idle"), (t_click, "Idle", "Activate", "Toward")] + ([(settle_rm, "Toward", "Finished", "Idle")] if settle_rm else []),
              series=[{"label": "palette (still fades)", "series": SERIES["palette"], "t": ts, "y": [(green(s["bg"]) - g_from) / (g_to - g_from or 1) * 100 for _, s in rows], "lw": 2.6, "at": 0.7},
                      {"label": "ghost (snaps)", "series": SERIES["ghost"], "t": ts, "y": [abs(s["ghost"] - rows[0][1]["ghost"]) / span * 100 for _, s in rows], "at": 0.3}])
    mid = green(rows[len(rows) // 2][1]["bg"])
    e8.checks += [Check("static preview shown while attended", s_rm["preview_anim"] == "none" and s_rm["preview_op"] >= 0.8, f"animation {s_rm['preview_anim']}, opacity {s_rm['preview_op']}"),
                  Check("ghost snaps to the destination", abs(rows[1][1]["ghost"] - rows[-1][1]["ghost"]) < 1.0),
                  Check("palette still fades", 20 < mid < 220, f"mid-flight green {mid}")]
    b.reduced_motion(False)
    b.hover(2, 2)
    rep.edges.append(e8)
    b.unseed_theme()
    return rep


def run_switch(b: Browser, base: str, ctrl: dict, out: Path) -> ControlReport:
    tag = ctrl["tag"]
    rep = ControlReport(tag=tag, kind="switch", page=ctrl["page"], nodes=["Idle"],
                        machine_edges=[("Idle", "Attend", "Idle"), ("Idle", "Neglect", "Idle"), ("Idle", "Activate", "Idle")])
    d = out / tag
    d.mkdir(parents=True, exist_ok=True)
    b.reduced_motion(False)
    b.goto(base + ctrl["page"], f"!!document.querySelector('{ctrl['target']['light']}') && !!document.getElementById('opt-switch-parts')")
    loc = b.js(f"window.__op.find({tag!r}, {json.dumps(ctrl['target'])})")
    rect, x, y = loc["rect"], loc["x"], loc["y"]
    S = "window.__op.sstate()"
    b.hover(2, 2); b.wait(0.2)
    s0 = b.js(S)
    # E1 attend
    e1 = Edge(("Idle", "Attend", "Idle"), "Attend: hover plays the preview",
              "The native input's ::after is the preview ghost, generated from the same parts as the theme toggle: same contrast, same plateau, same clock.")
    film = []
    b.hover(x, y)
    rows = sample(b, S, 2.0, 0.04, film=film, rect=rect)
    ts = [t for t, _ in rows]
    make_film(e1, film, d, "attend", keys=6, ylabel="% (opacity, left)", trace=[(0.0, "Idle", "Attend", "Idle")],
              series=[{"label": "preview opacity", "series": SERIES["preview"], "t": ts, "y": [s["preview_op"] * 100 for _, s in rows], "lw": 2.4},
                      {"label": "preview left (% of track)", "series": SERIES["ghost"], "t": ts, "y": [s["preview"] / s["w"] * 100 for _, s in rows], "at": 0.5}])
    peak = max(s["preview_op"] for _, s in rows)
    e1.checks += [Check("preview animation plays", any(s["anim"].startswith("opt-switch-preview") for _, s in rows)),
                  Check("preview reaches legible opacity", peak >= 0.8, f"peak {peak}")]
    rep.edges.append(e1)
    # E2 activate
    e2 = Edge(("Idle", "Activate", "Idle"), "Activate: the thumb snaps on the snap clock", "A native checkbox toggle; the thumb transitions over --op-motion-snap.")
    film = b.film_start(rect)
    t0 = b.now()
    b.click(x, y)
    rows = sample(b, S, 0.5, 0.02, t0=t0, film=film, rect=rect)
    lefts = [s["thumb"] for _, s in rows]
    ts = [t for t, _ in rows]
    travel = [abs(l - lefts[0]) / max(1e-6, abs(lefts[-1] - lefts[0])) * 100 for l in lefts]
    make_film(e2, film, d, "activate", keys=6, trace=[(0.0, "Idle", "Activate", "Idle")],
              series=[{"label": "thumb travel", "series": SERIES["thumb"], "t": ts, "y": travel, "lw": 2.4, "at": 0.5}])
    moved_by = next((t for t, s in rows if abs(s["thumb"] - lefts[-1]) < 0.5), None)
    e2.checks += [Check("checked state toggled", rows[-1][1]["checked"] != s0["checked"]),
                  # the detail prints the positions the check itself decides on, whole
                  # pixels: a finer figure would report a transition's last bit of
                  # floating-point wobble and differ between two identical runs
                  Check("thumb transitions (not a jump)", len({round(l) for l in lefts[:6]}) > 2, f"first positions {[round(l) for l in lefts[:6]]}"),
                  # the snap is 160 ms; arrival is polled from before the click, and the recording's
                  # encoder competes with the compositor on a two-core runner, so the window is three
                  # snaps: a jump still fails the check above, and a stalled transition still fails here
                  Check("thumb arrives within the snap clock", moved_by is not None and moved_by <= 0.5, f"arrived at {secs(moved_by)}")]
    rep.edges.append(e2)
    # E3 neglect
    e3 = Edge(("Idle", "Neglect", "Idle"), "Neglect", "The preview stops when the pointer leaves.")
    b.hover(2, 2); rows = sample(b, S, 0.6, 0.05)
    e3.checks += [Check("preview hidden once unattended", rows[-1][1]["preview_op"] < 0.05)]
    rep.edges.append(e3)
    # E4 reduced motion
    e4 = Edge(("Idle", "Attend", "Idle"), "Reduced motion: static preview", "The loop is off; the preview appears at the destination while attended.")
    b.reduced_motion(True); b.wait(0.2)
    film = b.film_start(rect)
    t0 = b.now()
    b.hover(x, y)
    film += burst(b, rect, 0.6, t0=t0, scale=2)
    s_rm = b.js(S)
    make_film(e4, film, d, "reduced", keys=3, trace=[(0.0, "Idle", "Attend", "Idle")])
    e4.checks += [Check("static preview shown", s_rm["anim"] == "none" and s_rm["preview_op"] >= 0.8, f"animation {s_rm['anim']}, opacity {s_rm['preview_op']}")]
    b.reduced_motion(False); b.hover(2, 2)
    rep.edges.append(e4)
    return rep


def run_attention(b: Browser, base: str, ctrl: dict, out: Path) -> ControlReport:
    tag = ctrl["tag"]
    rep = ControlReport(tag=tag, kind="attention", page=ctrl["page"], nodes=["Idle"],
                        machine_edges=[("Idle", "Attend", "Idle"), ("Idle", "Focus", "Idle"), ("Idle", "Activate", "Idle"), ("Idle", "Neglect", "Idle")])
    d = out / tag
    d.mkdir(parents=True, exist_ok=True)
    b.reduced_motion(False)
    b.goto(base + ctrl["page"], f"!!document.querySelector('{tag}')")
    loc = b.js(f"window.__op.find({tag!r}, {json.dumps(ctrl['target'])})")
    if not loc:
        e = Edge(("Idle", "Attend", "Idle"), "Target not found", "")
        e.checks.append(Check("target element exists on the page", False, json.dumps(ctrl["target"])))
        rep.edges.append(e)
        return rep
    rect = loc["host"]
    x, y = loc["x"], loc["y"]
    b.hover(2, 2); b.js("window.__op.blur()"); b.wait(0.2)
    base_sig = b.js("window.__op.sig()")
    rest_png = b.frame_bytes(rect, scale=1.5)
    # attend
    e1 = Edge(("Idle", "Attend", "Idle"), "Attend: hover", "A real pointer over the control must change something visible.")
    film = [(0.0, rest_png)]
    t0 = b.now()
    b.hover(x, y)
    film += burst(b, rect, 0.5, t0=t0)
    hov = b.js("window.__op.sig()")
    make_film(e1, film, d, "hover", keys=4, trace=[(0.0, "Idle", "Attend", "Idle")])
    if ctrl.get("hover", True):
        e1.checks.append(Check("visible hover affordance", hov != base_sig))
    else:
        e1.note = "hover affordance not required for this control"
    if ctrl.get("reveals"):
        rv = b.js(f"window.__op.hostQuery({ctrl['reveals']!r})")
        e1.checks.append(Check("reveals its annotation", rv is not None and rv.startswith("1|visible"), str(rv)))
    rep.edges.append(e1)
    # focus-visible
    e2 = Edge(("Idle", "Focus", "Idle"), "Attend: visible focus", "Keyboard-style focus must show a focus ring or equivalent.")
    b.hover(2, 2); b.wait(0.2)
    film = b.film_start(rect)
    t0 = b.now()
    landed = b.js("window.__op.focusVisible()")
    film += burst(b, rect, 0.4, t0=t0)
    foc = b.js("window.__op.sig()")
    make_film(e2, film, d, "focus", keys=3, trace=[(0.0, "Idle", "Focus", "Idle")])
    e2.checks += [Check("focusable", bool(landed)), Check("visible focus affordance", foc != base_sig)]
    b.js("window.__op.blur()")
    rep.edges.append(e2)
    # activate
    kind = ctrl.get("activate", "none")
    e3 = Edge(("Idle", "Activate", "Idle"), "Activate: click", f"Expected consequence: {kind}.")
    expr = {"aria-pressed": "window.__op.attr('aria-pressed')", "aria-selected": "window.__op.attr('aria-selected')",
            "details-open": "window.__op.detailsOpen()", "checked": "window.__op.checked()"}.get(kind)
    before = b.js(expr) if expr else None
    if b.js("window.__op.holdLink()"):
        e3.note = "a navigating link: clicked with navigation suppressed"
    if b.synthetic and b.js("window.__op.holdPopup()"):
        e3.note = "a select: clicked with its popup held, since a popup is a browser window and holds every frame of the page while it is open"
    b.hover(x, y); b.wait(0.15)
    film = b.film_start(rect)
    t0 = b.now()
    b.click(x, y)
    film += burst(b, rect, 0.6, t0=t0)
    after = b.js(expr) if expr else None
    make_film(e3, film, d, "activate", keys=4, trace=[(0.0, "Idle", "Activate", "Idle")])
    if expr:
        e3.checks.append(Check(f"activation changes {kind}", before != after, f"{before} -> {after}"))
    else:
        e3.checks.append(Check("activation raises no error", not b.errors()))
    rep.edges.append(e3)
    # neglect
    e4 = Edge(("Idle", "Neglect", "Idle"), "Neglect", "Leaving returns the control to rest.")
    b.hover(2, 2); b.js("window.__op.blur()"); b.wait(0.4)
    e4.checks.append(Check("returns toward the rest signature", b.js("window.__op.sig()") != hov or hov == base_sig))
    rep.edges.append(e4)
    return rep


# ----------------------------------------------------------------------------
# rendering
# ----------------------------------------------------------------------------
CSS = """
body{font:15px/1.55 system-ui,sans-serif;margin:2rem auto;max-width:1500px;color:#222;background:#fafafa;padding:0 1rem}
h1{font-size:1.4rem}h2{font-size:1.15rem;margin-top:2.6rem;border-top:1px solid #ddd;padding-top:1rem}h3{font-size:.95rem;color:#555;margin:1rem 0 .4rem}
.strip{display:flex;gap:10px;flex-wrap:wrap}figure{margin:0}figcaption{font-size:.78rem;color:#555;text-align:center;font-variant-numeric:tabular-nums}
img.frame{max-width:340px;display:block;border:1px solid #ddd;background:#fff}img.curve{max-width:100%}
table{border-collapse:collapse;font-size:.9rem}td,th{padding:.25rem .6rem;border-bottom:1px solid #e5e5e5;text-align:left;vertical-align:top}
.ok{color:#1b7f3b;font-weight:600}.fail{color:#b3261e;font-weight:700}
p.note{color:#444;max-width:70ch}.machine{margin:.4rem 0 1rem}.summary{padding:.6rem .9rem;background:#fff;border:1px solid #ddd;display:inline-block}
a{color:#0b57d0}
.film{margin:.6rem 0 1rem;display:block;border:1px solid #ddd;background:#fff;padding:.5rem;max-width:930px;user-select:none;-webkit-user-select:none;outline:2px solid transparent;outline-offset:2px}
.film:focus-visible,.film svg.chart:focus-visible{outline-color:#D55E00}
.film .stagebox{display:flex;flex-direction:column;align-items:center;padding:4px 0 8px}
.matrix{display:grid;grid-template-columns:repeat(auto-fill,minmax(220px,1fr));gap:.6rem;margin:.4rem 0 1rem}
.matrix figure{margin:0}.matrix img{width:100%;height:auto;border:1px solid #e3e3e3}.matrix figcaption{font-size:.78rem;color:#666}
.film .stagewrap{position:relative;display:inline-block}
.film .stage{background-repeat:no-repeat;border:1px solid #e3e3e3;box-shadow:0 1px 4px rgba(0,0,0,.08)}
.film .stagevideo{position:absolute;inset:1px;width:calc(100% - 2px);height:calc(100% - 2px);display:none;object-fit:fill}
.film .stagevideo.ready{display:block}
.film .stage.pending{outline:2px dashed #D55E00;outline-offset:2px}
.film .stagelabel{font-size:.78rem;color:#666;min-height:1.2em;margin-top:.3rem;font-variant-numeric:tabular-nums}
.film .reelbox{position:relative;overflow:hidden;width:100%;padding:6px 0;border-top:1px solid #eee;touch-action:none}
.film .gate{position:absolute;left:0;top:0;bottom:0;width:0;margin-left:-1px;border-left:2px solid #D55E00;opacity:.75;pointer-events:none;z-index:2}
.film .reel{display:flex;gap:8px;align-items:flex-start;will-change:transform}
.film .fr{margin:0;flex:none;cursor:pointer;text-align:center}
.film .fr .cell{background-repeat:no-repeat;border:1px solid #e3e3e3;box-sizing:content-box}
.film .fr:hover .cell{border-color:#999}
.film .fr.current .cell{outline:2px solid #D55E00;outline-offset:1px}
.film .fr.pending .cell{outline:2px dashed #D55E00;outline-offset:1px}
.film .fr figcaption{font-size:.72rem;color:#666;font-variant-numeric:tabular-nums;white-space:nowrap}
.film .bar{display:flex;gap:.6rem;align-items:center;margin-top:.4rem;font-size:.85rem;flex-wrap:wrap}
.film input[type=range]{flex:1;min-width:220px}.film .t{font-variant-numeric:tabular-nums;min-width:4.5rem}.film .n{color:#777}
.film .keys{font-size:.8rem;color:#555;margin:.3rem 0 0}.film .keys summary{cursor:pointer}
.film .keys dl{display:grid;grid-template-columns:max-content 1fr;gap:.15rem .8rem;margin:.4rem 0}.film .keys dt{font-family:ui-monospace,monospace}.film .keys dd{margin:0}
.film .chartbox{margin-top:.6rem;position:relative}.film svg.chart{max-width:100%;height:auto;cursor:ew-resize;display:block;touch-action:none}
.film .peek{position:absolute;bottom:56px;transform:translateX(-50%);pointer-events:none;background:#fff;border:1px solid #bbb;border-radius:3px;padding:3px;box-shadow:0 2px 6px rgba(0,0,0,.15);z-index:3}
.film .peek .pframe{background-repeat:no-repeat}.film .peek .ptime{font-size:.75rem;text-align:center;color:#333;font-variant-numeric:tabular-nums;white-space:nowrap}
.film .sr{position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0 0 0 0);white-space:nowrap}
"""

PLAYER_JS = """<script>
document.querySelectorAll('.film').forEach(f => {
  const times = JSON.parse(f.dataset.times), n = times.length, w = +f.dataset.w, h = +f.dataset.h;
  const chapters = JSON.parse(f.dataset.chapters || '[[0,"start"]]');
  const reelbox = f.querySelector('.reelbox'), reel = f.querySelector('.reel'), gate = f.querySelector('.gate'), frs = [...f.querySelectorAll('.fr')];
  const slider = f.querySelector('input[type=range]'), label = f.querySelector('.t'), btn = f.querySelector('button.play'), rate = f.querySelector('select');
  const stage = f.querySelector('.stage'), stageLabel = f.querySelector('.stagelabel'), live = f.querySelector('.sr');
  const video = f.querySelector('.stagevideo');
  if (video) { video.addEventListener('loadeddata', () => video.classList.add('ready')); }
  const syncVideo = (t, isPlaying, r) => { if (!video) return; video.playbackRate = r;
    if (isPlaying) { if (video.paused) video.play().catch(() => {}); } else if (!video.paused) video.pause();
    if (Math.abs(video.currentTime - t) > (isPlaying ? 0.12 : 0.02)) video.currentTime = t; };
  const chart = f.querySelector('svg.chart'), head = chart && chart.querySelector('.head'), headDot = chart && chart.querySelector('.head-dot'), headT = chart && chart.querySelector('.head-t');
  const played = chart && chart.querySelector('.bar-played'), band = chart && chart.querySelector('.band'), peekLine = chart && chart.querySelector('.peek-line');
  const peek = f.querySelector('.peek'), pframe = peek && peek.querySelector('.pframe'), ptime = peek && peek.querySelector('.ptime');
  const x0 = chart ? +chart.dataset.x0 : 0, x1 = chart ? +chart.dataset.x1 : 1, t1 = chart ? +chart.dataset.t1 : times[n - 1];
  const tEnd = Math.max(times[n - 1], t1);
  const scale = w < 220 ? 1.5 : Math.min(1, 320 / w);
  const cw = Math.round(w * scale), ch = Math.round(h * scale);
  const paint = (el, sw, sh) => { el.style.width = sw + 'px'; el.style.height = sh + 'px'; el.style.backgroundImage = 'url(' + f.dataset.sheet + ')'; el.style.backgroundSize = (sw * n) + 'px ' + sh + 'px'; };
  frs.forEach((fr, k) => { const c = fr.querySelector('.cell'); paint(c, cw, ch); c.style.backgroundPosition = (-k * cw) + 'px 0'; });
  const ss = Math.min(900 / w, w < 220 ? 3 : 1.25), sw = Math.round(w * ss), sh = Math.round(h * ss);
  paint(stage, sw, sh);
  if (pframe) paint(pframe, cw, ch);
  let tc = 0, playing = false, raf = 0, last = null, pending = null;
  const frameAt = t => { let k = 0; for (let j = 0; j < n; j++) if (times[j] <= t + 1e-6) k = j; return k; };
  const chapterAt = t => { let c = chapters[0]; for (const ch_ of chapters) if (ch_[0] <= t + 1e-6) c = ch_; return c; };
  const xOf = t => x0 + Math.min(1, Math.max(0, t / t1)) * (x1 - x0);
  const fmt = t => t.toFixed(2) + 's';
  const announce = (() => { let timer = 0; return msg => { clearTimeout(timer); timer = setTimeout(() => { live.textContent = msg; }, 250); }; })();
  const showStage = (k, tag) => { stage.style.backgroundPosition = (-k * sw) + 'px 0'; stageLabel.textContent = fmt(times[k]) + ' \\u00b7 frame ' + (k + 1) + ' of ' + n + (tag ? ' \\u00b7 ' + tag : ''); };
  const render = () => {
    syncVideo(tc, playing, parseFloat(rate.value) || 1);
    const k = frameAt(tc);
    const next = Math.min(n - 1, k + 1), span = times[next] - times[k];
    const frac = span > 0 ? Math.min(1, Math.max(0, (tc - times[k]) / span)) : 0;
    const mid = j => frs[j].offsetLeft + frs[j].offsetWidth / 2;
    const pitch = n > 1 ? frs[1].offsetLeft - frs[0].offsetLeft : frs[0].offsetWidth;
    const perPage = Math.max(1, Math.floor(reelbox.clientWidth / pitch));
    const page = Math.floor(k / perPage), pageLeft = frs[page * perPage].offsetLeft;
    reel.style.transform = 'translateX(' + (-pageLeft) + 'px)';
    const samePage = Math.floor(next / perPage) === page;
    gate.style.left = (mid(k) + (samePage ? frac * (mid(next) - mid(k)) : 0) - pageLeft) + 'px';
    frs.forEach((fr, j) => { fr.classList.toggle('current', j === k); fr.classList.toggle('pending', j === pending); });
    if (pending === null) { showStage(k, chapterAt(tc)[1]); stage.classList.remove('pending'); }
    slider.value = k; label.textContent = fmt(tc);
    if (chart) {
      const x = xOf(tc);
      head.setAttribute('x1', x); head.setAttribute('x2', x); headDot.setAttribute('cx', x); headT.setAttribute('x', x + 4); headT.textContent = fmt(tc);
      played.setAttribute('width', Math.max(0, x - x0));
      chart.setAttribute('aria-valuenow', tc.toFixed(2)); chart.setAttribute('aria-valuemax', tEnd.toFixed(2));
      chart.setAttribute('aria-valuetext', tc.toFixed(2) + ' seconds, frame ' + (k + 1) + ' of ' + n + ', ' + chapterAt(tc)[1]);
    }
  };
  const seekTo = t => { tc = Math.min(tEnd, Math.max(0, t)); render(); announce(fmt(tc) + ', frame ' + (frameAt(tc) + 1) + ' of ' + n); };
  const pause = () => { playing = false; btn.textContent = 'Play'; cancelAnimationFrame(raf); last = null; };
  const play = () => { if (tc >= tEnd) tc = 0; playing = true; btn.textContent = 'Pause'; last = null; raf = requestAnimationFrame(tick); };
  const tick = now => {
    if (!playing) return;
    if (last !== null) { tc += (now - last) / 1000 * +rate.value; if (tc > tEnd + 0.6) tc = 0; }
    last = now; render(); raf = requestAnimationFrame(tick);
  };
  btn.addEventListener('click', () => playing ? pause() : play());
  slider.addEventListener('input', () => { pause(); seekTo(times[+slider.value]); });
  // --- peek: look without moving the playhead ---
  const showPeek = (t, anchorX) => {
    if (!chart) return;
    const k = frameAt(t), x = xOf(t);
    peekLine.setAttribute('x1', x); peekLine.setAttribute('x2', x); peekLine.setAttribute('visibility', 'visible');
    const c = chapterAt(t); const nextC = chapters.find(ch_ => ch_[0] > c[0]);
    band.setAttribute('x', xOf(c[0])); band.setAttribute('width', Math.max(0, xOf(nextC ? nextC[0] : tEnd) - xOf(c[0])));
    pframe.style.backgroundPosition = (-k * cw) + 'px 0';
    ptime.textContent = fmt(times[k]) + ' \\u00b7 ' + c[1];
    peek.hidden = false;
    const r = chart.getBoundingClientRect(), vb = chart.viewBox.baseVal;
    peek.style.left = (anchorX !== undefined ? anchorX : x * (r.width / vb.width)) + 'px';
  };
  const hidePeek = () => { if (!chart) return; peek.hidden = true; peekLine.setAttribute('visibility', 'hidden'); band.setAttribute('width', 0); };
  const tAtPointer = e => { const r = chart.getBoundingClientRect(), vb = chart.viewBox.baseVal; const px = (e.clientX - r.left) * (vb.width / r.width); return Math.min(tEnd, Math.max(0, (px - x0) / (x1 - x0) * t1)); };
  if (chart) {
    chart.addEventListener('pointermove', e => { if (e.buttons & 1) { seekTo(tAtPointer(e)); hidePeek(); } else { const r = chart.getBoundingClientRect(); showPeek(tAtPointer(e), e.clientX - r.left); } });
    chart.addEventListener('pointerleave', hidePeek);
    chart.addEventListener('pointerdown', e => { e.preventDefault(); pause(); hidePeek(); chart.setPointerCapture(e.pointerId); seekTo(tAtPointer(e)); });
    chart.addEventListener('pointerup', e => { chart.releasePointerCapture(e.pointerId); });
  }
  // --- strip: hover peeks; press-drag chooses a pending frame, release seeks, Esc cancels ---
  const frameUnder = e => { const el = document.elementFromPoint(e.clientX, e.clientY); const fr = el && el.closest('.fr'); return fr && frs.includes(fr) ? +fr.dataset.k : null; };
  frs.forEach((fr, k) => {
    fr.addEventListener('pointerenter', () => { if (pending === null) showPeek(times[k]); });
    fr.addEventListener('pointerleave', () => { if (pending === null) hidePeek(); });
  });
  const setPending = k => { pending = k; render(); showStage(k, 'pending \\u2014 release to seek, Esc to cancel'); stage.classList.add('pending'); showPeek(times[k]); };
  reelbox.addEventListener('pointerdown', e => {
    const k = frameUnder(e); if (k === null) return;
    e.preventDefault(); pause(); reelbox.setPointerCapture(e.pointerId); setPending(k);
  });
  reelbox.addEventListener('pointermove', e => { if (pending === null) return; const k = frameUnder(e); if (k !== null && k !== pending) setPending(k); });
  const commit = () => { if (pending === null) return; const k = pending; pending = null; hidePeek(); seekTo(times[k]); };
  const cancel = () => { if (pending === null) return; pending = null; hidePeek(); render(); announce('seek cancelled'); };
  reelbox.addEventListener('pointerup', commit);
  reelbox.addEventListener('pointercancel', cancel);
  // --- keys, YouTube's model, while the player (or its chart) has focus ---
  const step = dk => { pause(); seekTo(times[Math.min(n - 1, Math.max(0, frameAt(tc) + dk))]); };
  const rates = ['0.25', '0.5', '1'];
  f.addEventListener('keydown', e => {
    if (e.target.tagName === 'SELECT' || e.target.tagName === 'SUMMARY') return;
    const key = e.key;
    let handled = true;
    if (key === ' ' || key === 'k' || key === 'K') { playing ? pause() : play(); }
    else if (key === '.') step(1);
    else if (key === ',') step(-1);
    else if (key === 'ArrowRight') step(5);
    else if (key === 'ArrowLeft') step(-5);
    else if (key === 'l' || key === 'L') step(10);
    else if (key === 'j' || key === 'J') step(-10);
    else if (key === 'Home') step(-n);
    else if (key === 'End') step(n);
    else if (/^[0-9]$/.test(key)) { pause(); seekTo(tEnd * (+key / 10)); }
    else if (key === '>' ) { rate.value = rates[Math.min(rates.length - 1, rates.indexOf(rate.value) + 1)]; announce('speed ' + rate.value + 'x'); }
    else if (key === '<') { rate.value = rates[Math.max(0, rates.indexOf(rate.value) - 1)]; announce('speed ' + rate.value + 'x'); }
    else if (key === 'Escape') { cancel(); }
    else handled = false;
    if (handled) e.preventDefault();
  });
  window.addEventListener('resize', render);
  render();
});
</script>"""


def render_control(rep: ControlReport, out: Path):
    d = out / rep.tag
    d.mkdir(parents=True, exist_ok=True)  # a control that failed before its runner made it still gets a page
    total = len(rep.checks)
    passed = sum(1 for c in rep.checks if c.ok)
    parts = [f"<!doctype html><html lang='en'><head><meta charset='utf-8'><title>{rep.tag} — interaction report</title><style>{CSS}</style></head><body>",
             f"<p><a href='../index.html'>All controls</a></p><h1>&lt;{rep.tag}&gt; — interaction report</h1>",
             f"<p class='note'>Kind: <code>{rep.kind}</code>. Page: <code>{rep.page}</code>. {clock_note(rep.clock)}. Every frame and sample below comes from real pointer and keyboard events in headless Chromium against the built site.</p>",
             f"<p class='summary'><span class='{'ok' if passed == total else 'fail'}'>{passed} of {total} checks pass</span></p>",
             "<h2>The machine</h2><div class='machine'>" + machine_svg(rep.nodes, rep.machine_edges, None) + "</div>",
             "<p class='note'>Nodes are flight states; loops are inputs that leave the flight alone. Below, each behaviour highlights the edge it exercises.</p>"]
    for i, e in enumerate(rep.edges, 1):
        parts.append(f"<h2>{i}. {e.title}</h2>")
        parts.append(f"<h3>Machine annotated for {e.key[0]} —{e.key[1]}→ {e.key[2]}</h3><div class='machine'>{machine_svg(rep.nodes, rep.machine_edges, e.key)}</div>")
        if e.narrative:
            parts.append(f"<p class='note'>{e.narrative}</p>")
        if e.note:
            parts.append(f"<p class='note'><em>{e.note}</em></p>")
        if e.film:
            f = e.film
            title = f" <span class='n'>{f['title']}</span>" if f.get("title") else ""
            cells = "".join(
                f"<figure class='fr' data-k='{k}'><div class='cell'></div>"
                f"<figcaption>{t:.2f}s{'' if k == 0 else (' &middot; ' + (f'{dl * 100:.0f}%' if dl > 0.001 else 'same'))}</figcaption></figure>"
                for k, (t, dl) in enumerate(zip(f["times"], f["deltas"])))
            chart = f.get("chart", "")
            if chart:
                chart = chart.replace("<svg class=\"chart\"", "<svg class=\"chart\" tabindex=\"0\" role=\"slider\" aria-label=\"playhead\" aria-valuemin=\"0\" aria-valuenow=\"0\" aria-valuetext=\"0.00 seconds\"", 1)
            parts.append(
                f"<h3>Playback{title}</h3>"
                f"<div class='film' tabindex='0' role='group' aria-label='Playback: {e.title}' data-sheet='{f['sheet']}' data-w='{f['w']}' data-h='{f['h']}' "
                f"data-times='{json.dumps([round(t, 3) for t in f['times']])}' data-chapters='{json.dumps(f['chapters'])}'>"
                f"<div class='stagebox'><div class='stagewrap'><div class='stage'></div>{('<video class=' + chr(39) + 'stagevideo' + chr(39) + ' muted playsinline preload=' + chr(39) + 'metadata' + chr(39) + ' src=' + chr(39) + f['video'] + chr(39) + '></video>') if f.get('video') else ''}</div><div class='stagelabel'></div></div>"
                f"<div class='reelbox'><div class='gate'></div><div class='reel'>{cells}</div></div>"
                f"<div class='bar'><button type='button' class='play'>Play</button>"
                f"<select aria-label='speed'><option value='1'>1x</option><option value='0.5'>0.5x</option><option value='0.25'>0.25x</option></select>"
                f"<input type='range' min='0' max='{len(f['times']) - 1}' value='0' aria-label='frame'><span class='t'></span>"
                f"<span class='n'>{len(f['times'])} frames; captions give the share of pixels changed since the previous frame</span></div>"
                f"<details class='keys'><summary>Keys</summary><dl>"
                f"<dt>Space, K</dt><dd>play / pause</dd><dt>, .</dt><dd>previous / next frame</dd><dt>&larr; &rarr;</dt><dd>five frames back / forward</dd>"
                f"<dt>J L</dt><dd>ten frames back / forward</dd><dt>0-9</dt><dd>seek to 0-90 %</dd><dt>Home End</dt><dd>first / last frame</dd>"
                f"<dt>&lt; &gt;</dt><dd>slower / faster</dd><dt>Esc</dt><dd>cancel a pending seek on the strip</dd></dl>"
                f"<p>Hover the chart or the strip to peek without moving the playhead; press and drag across the strip to choose a frame and release to seek.</p></details>"
                + (f"<div class='chartbox'>{chart}<div class='peek' hidden><div class='pframe'></div><div class='ptime'></div></div></div>" if chart else "")
                + "<span class='sr' aria-live='polite'></span></div>")
        if e.frames:
            parts.append("<h3>Frames</h3><div class='strip'>" + "".join(
                f"<figure><img class='frame' src='{p}'><figcaption>{c}</figcaption></figure>" for c, p in e.frames) + "</div>")
        for c, p in e.curves:
            parts.append(f"<h3>{c}</h3><img class='curve' src='{p}' alt='{c}'>")
        parts.append("<h3>Checks</h3><table>" + "".join(
            f"<tr><td class='{'ok' if c.ok else 'fail'}'>{'pass' if c.ok else 'FAIL'}</td><td>{c.name}</td><td>{c.detail}</td></tr>" for c in e.checks) + "</table>")
        if e.matrix:
            parts.append(f"<h4>{e.matrix_title}</h4><div class='matrix'>" + "".join(
                f"<figure><img src='{fn}' alt='the chart under {mode}' loading='lazy'><figcaption>{mode}</figcaption></figure>" for mode, fn in e.matrix) + "</div>")
    parts.append(PLAYER_JS)
    parts.append("</body></html>")
    (d / "index.html").write_text("\n".join(parts))


def render_index(reports: list[ControlReport], statics: list[str], out: Path):
    rows = []
    for r in reports:
        total, passed = len(r.checks), sum(1 for c in r.checks if c.ok)
        rows.append(f"<tr><td><a href='{r.tag}/index.html'>{r.tag}</a></td><td>{r.kind}</td><td class='{'ok' if passed == total else 'fail'}'>{passed}/{total}</td></tr>")
    html = [f"<!doctype html><html lang='en'><head><meta charset='utf-8'><title>Interaction report</title><style>{CSS}</style></head><body>",
            "<h1>Interaction report — every control, every machine edge, real input</h1>",
            f"<p class='note'>Generated by tools/interaction_report/report.py. {clock_note(CLOCK)}. The machine diagrams come from the code's own transition table; the frames and curves from CDP mouse and keyboard events with the pointer resting on the control. A failing check here fails the build.</p>",
            "<table><tr><th>control</th><th>kind</th><th>checks</th></tr>" + "".join(rows) + "</table>",
            "<h2>Declared static (no interaction)</h2><p class='note'>" + ", ".join(statics) + "</p>",
            "</body></html>"]
    (out / "index.html").write_text("\n".join(html))



# ----------------------------------------------------------------------------
# the integrated revision: the same evidence rendered with the site's own
# elements (opt-film, opt-machine, opt-term, opt-table, opt-kpi ...), so
# the report exercises the controls it is made of. Its pages take the
# built site's shared <head> and navigation, exactly as op-pages does.
# ----------------------------------------------------------------------------
def site_chrome(base: str) -> tuple[str, str]:
    """(shared head inner HTML without page-specific tags, nav markup)."""
    html = urllib.request.urlopen(base + "/").read().decode()
    head = html[html.index("<head>") + 6:html.index("</head>")]
    out, rest = "", head
    while "<title>" in rest:
        a = rest.index("<title>"); b = rest.index("</title>") + 8
        out += rest[:a]; rest = rest[b:]
    head = out + rest
    head = "".join(chunk for chunk in head.split(">") if "name=\"description\"" not in chunk and "rel=\"canonical\"" not in chunk) if False else \
        ">".join(chunk for chunk in head.split(">") if "name=\"description\"" not in chunk and "rel=\"canonical\"" not in chunk)
    nav_start = html.index("<opt-site-nav>"); nav_end = html.index("</opt-site-nav>") + len("</opt-site-nav>")
    return head, html[nav_start:nav_end]


def esc(t: str) -> str:
    return t.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;").replace("\"", "&quot;")


def integrated_page(rep: ControlReport, head: str, nav: str, prefix: str) -> str:
    total = len(rep.checks); passed = sum(1 for c in rep.checks if c.ok)
    outcome = "pass" if passed == total else "fail"
    machine = {"nodes": rep.nodes, "edges": [list(e) for e in rep.machine_edges], "highlight": None}
    parts = [f"<!doctype html>\n<html lang=\"en\">\n<head>\n<title>{esc(rep.tag)} interaction report: openpower.tools</title>\n"
             f"<meta name=\"description\" content=\"Every machine edge of {esc(rep.tag)} driven with real input: frames, curves, checks.\" />\n"
             f"<link rel=\"canonical\" href=\"https://www.openpower.tools{prefix}{rep.tag}/\" />{head}</head>\n<body>\n<opt-theme-toggle></opt-theme-toggle>\n{nav}\n"
             f"<opt-site-header heading=\"{esc(rep.tag)}\" tagline=\"Interaction report: every edge of its machine, driven with real input.\"></opt-site-header>\n<main>\n"
             f"<p><a href=\"../\">All controls</a></p>\n"
             f"<opt-kpi label=\"checks pass\" value=\"{passed} of {total}\"><opt-term scheme=\"outcome\" value=\"{outcome}\"></opt-term></opt-kpi>\n"
             f"<p>Kind: <code>{rep.kind}</code>. Page under test: <code>{esc(rep.page)}</code>. {clock_note(rep.clock)}. Every frame and sample comes from real pointer and keyboard events in headless Chromium against the built site; this page is rendered with the site's own elements.</p>\n"
             f"<h2>The machine</h2>\n<opt-machine><script type=\"application/json\">{json.dumps(machine)}</script></opt-machine>\n"
             f"<p>Nodes are flight states; loops are inputs that leave the flight alone. Each behaviour below lights the edge it exercises; during playback the machine's own playhead rests on the settled node and travels the edge at each recorded transition.</p>\n"]
    for i, e in enumerate(rep.edges, 1):
        film_id = f"film-{i}"
        m = {"nodes": rep.nodes, "edges": [list(x) for x in rep.machine_edges], "highlight": list(e.key), "trace": e.film["trace"] if e.film else []}
        parts.append(f"<h2>{i}. {esc(e.title)}</h2>\n<h3>Machine annotated for {esc(e.key[0])} —{esc(e.key[1])}→ {esc(e.key[2])}</h3>\n"
                     f"<opt-machine for=\"{film_id}\"><script type=\"application/json\">{json.dumps(m)}</script></opt-machine>\n")
        if e.narrative:
            parts.append(f"<p>{esc(e.narrative)}</p>\n")
        if e.note:
            parts.append(f"<p><em>{esc(e.note)}</em></p>\n")
        if e.film:
            f = e.film
            data = {"w": f["w"], "h": f["h"], "times": [round(t, 3) for t in f["times"]], "deltas": [round(x, 4) for x in f["deltas"]],
                    "chapters": f["chapters"], "series": [{**sr, "t": [round(t, 3) for t in sr["t"]], "y": [round(v, 2) for v in sr["y"]]} for sr in f["series"]],
                    "ylabel": f["ylabel"]}
            parts.append(f"<h3>Playback{(' <small>' + esc(f['title']) + '</small>') if f.get('title') else ''}</h3>\n"
                         f"<opt-film id=\"{film_id}\" sheet=\"{f['sheet']}\"{(' video=' + chr(34) + f['video'] + chr(34)) if f.get('video') else ''} title=\"{esc(e.title)}\"><script type=\"application/json\">{json.dumps(data)}</script></opt-film>\n")
        if e.matrix:
            cells = "".join(f"<figure style=\"margin:0\"><img src=\"{fn}\" alt=\"the chart under {esc(mode)}\" loading=\"lazy\" style=\"width:100%;height:auto;border:1px solid var(--op-border)\"><figcaption>{esc(mode)}</figcaption></figure>" for mode, fn in e.matrix)
            parts.append(f"<h3>{esc(e.matrix_title)}</h3>\n<div style=\"display:grid;grid-template-columns:repeat(auto-fill,minmax(220px,1fr));gap:0.6rem\">{cells}</div>\n")
        rows = "".join(f"<tr><td><opt-term scheme=\"outcome\" value=\"{'pass' if c.ok else 'fail'}\"></opt-term></td><td>{esc(c.name)}</td><td>{esc(c.detail)}</td></tr>" for c in e.checks)
        parts.append(f"<h3>Checks</h3>\n<opt-table lined=\"\"><table><thead><tr><th>outcome</th><th>check</th><th>detail</th></tr></thead><tbody>{rows}</tbody></table></opt-table>\n")
    parts.append("</main>\n<opt-site-footer></opt-site-footer>\n<noscript><p>This report is rendered by WebAssembly; it needs JavaScript enabled.</p></noscript>\n</body>\n</html>\n")
    return "".join(parts)


def integrated_index(reports: list[ControlReport], statics: list[str], head: str, nav: str, prefix: str) -> str:
    rows = []
    for r in reports:
        total, passed = len(r.checks), sum(1 for c in r.checks if c.ok)
        rows.append(f"<tr><td><a href=\"{r.tag}/\">{esc(r.tag)}</a></td><td>{r.kind}</td><td>{passed} of {total}</td>"
                    f"<td><opt-term scheme=\"outcome\" value=\"{'pass' if passed == total else 'fail'}\"></opt-term></td></tr>")
    all_ok = all(all(c.ok for c in r.checks) for r in reports)
    return (f"<!doctype html>\n<html lang=\"en\">\n<head>\n<title>Interaction reports: openpower.tools</title>\n"
            f"<meta name=\"description\" content=\"Every control driven through its interaction machine with real input, rendered with the site's own elements.\" />\n"
            f"<link rel=\"canonical\" href=\"https://www.openpower.tools{prefix}\" />{head}</head>\n<body>\n<opt-theme-toggle></opt-theme-toggle>\n{nav}\n"
            f"<opt-site-header heading=\"Interaction reports\" tagline=\"Every control, every machine edge, real input - rendered with the controls themselves.\"></opt-site-header>\n<main>\n"
            f"<opt-kpi label=\"controls\" value=\"{len(reports)}\"><opt-term scheme=\"outcome\" value=\"{'pass' if all_ok else 'fail'}\"></opt-term></opt-kpi>\n"
            f"<p>Generated by tools/interaction_report/report.py from the code's own machine table. {clock_note(CLOCK)}. The ad hoc revision of the same evidence, which depends on none of these elements, is kept as a build artifact so gross failures in the controls can still be read.</p>\n"
            f"<opt-table lined=\"\"><table><thead><tr><th>control</th><th>kind</th><th>checks</th><th>outcome</th></tr></thead><tbody>{''.join(rows)}</tbody></table></opt-table>\n"
            f"<h2>Declared static</h2><p>{esc(', '.join(statics))}</p>\n</main>\n<opt-site-footer></opt-site-footer>\n</body>\n</html>\n")


def render_integrated(reports: list[ControlReport], statics: list[str], adhoc_out: Path, out: Path, base: str, prefix: str):
    head, nav = site_chrome(base)
    out.mkdir(parents=True, exist_ok=True)
    for rep in reports:
        d = out / rep.tag
        d.mkdir(parents=True, exist_ok=True)
        for e in rep.edges:
            if e.film:
                shutil.copy(adhoc_out / rep.tag / e.film["sheet"], d / e.film["sheet"])
                if e.film.get("video"):
                    shutil.copy(adhoc_out / rep.tag / e.film["video"], d / e.film["video"])
            for _, fn in e.matrix:
                shutil.copy(adhoc_out / rep.tag / fn, d / fn)
        (d / "index.html").write_text(integrated_page(rep, head, nav, prefix))
    (out / "index.html").write_text(integrated_index(reports, statics, head, nav, prefix))


# ----------------------------------------------------------------------------
# the film kind: opt-film exercised on an integrated page it renders. This
# is the one level of recursion; the checks are written in plain JS and
# the frames go to the ad hoc report only.
# ----------------------------------------------------------------------------
def run_film(b: Browser, base: str, ctrl: dict, out: Path) -> ControlReport:
    tag = ctrl["tag"]
    rep = ControlReport(tag=tag, kind="film", page=ctrl["page"], nodes=["Paused", "Playing"],
                        machine_edges=[("Paused", "Play", "Playing"), ("Playing", "Pause", "Paused"), ("Paused", "Seek", "Paused"), ("Paused", "Peek", "Paused"), ("Paused", "Press", "Paused")])
    d = out / tag
    d.mkdir(parents=True, exist_ok=True)
    b.reduced_motion(False)
    b.goto(base + ctrl["page"], "document.querySelectorAll('opt-film').length > 1 && !!document.querySelectorAll('opt-film')[1].shadowRoot?.querySelector('.stage')")
    F = "document.querySelectorAll('opt-film')[1]"
    M = "document.querySelector('opt-machine[for=\"film-2\"]')"
    S = f"""(() => {{ const f = {F}, sr = f.shadowRoot; const cur = sr.querySelector('.fr.current'), pend = sr.querySelector('.fr.pending');
      const st = n => f.matches(':state(' + n + ')'); const tok = {M} && {M}.shadowRoot.querySelector('.token');
      return {{k: cur ? +cur.dataset.k : -1, pending: pend ? +pend.dataset.k : null, playing: st('playing'), pend_state: st('pending'), peeking: st('peeking'),
        t: sr.querySelector('.t').textContent, token: tok ? [+tok.getAttribute('cx'), +tok.getAttribute('cy')] : null,
        other_k: +document.querySelectorAll('opt-film')[0].shadowRoot.querySelector('.fr.current').dataset.k, role: sr.querySelector('.chart').getAttribute('role')}}; }})()"""
    loc = b.js(f"(() => {{ const el = {F}; el.scrollIntoView({{block: 'center'}}); const r = el.getBoundingClientRect(); return [r.x, r.y, r.width, r.height]; }})()")
    rect = loc
    s0 = b.js(S)
    # E1 play/pause: frames, custom state, the linked machine's token, and the other film untouched
    e1 = Edge(("Paused", "Play", "Playing"), "Play: the clock runs", "Frames advance on real time, the host exposes :state(playing), the machine linked by for= moves its token, and a sibling film is unaffected.")
    film = b.film_start(rect)
    t0 = b.now()
    b.js(f"{F}.shadowRoot.querySelector('button.play').click(); 'ok'")
    film += burst(b, rect, 1.6, fps=8, t0=t0, scale=1)
    s1 = b.js(S)
    b.js(f"{F}.shadowRoot.querySelector('button.play').click(); 'ok'")
    s1b = b.js(S)
    make_film(e1, film, d, "play", keys=5, trace=[(0.0, "Paused", "Play", "Playing")])
    e1.checks += [Check("frames advance while playing", s1["k"] > s0["k"], f"{s0['k']} -> {s1['k']}"),
                  Check("playing exposed as a custom state", s1["playing"] and not s1b["playing"]),
                  Check("the linked machine's playhead moved", s0["token"] != s1["token"], f"{s0['token']} -> {s1['token']}"),
                  Check("a sibling film is unaffected", s1["other_k"] == s0["other_k"]),
                  Check("chart is a slider", s0["role"] == "slider")]
    rep.edges.append(e1)
    # E2 keys
    e2 = Edge(("Paused", "Seek", "Paused"), "Seek by keys",
              "Home, then . . , -> : YouTube's model, frame by frame. Shift plus an arrow seeks one second of film time, "
              "PageDown and PageUp walk the chapters, and Control plus an arrow and Alt plus an arrow alias the chapter keys.")

    def key(name: str, code: str | None = None, mods: int = 0):
        """One keydown/keyup. mods is the CDP bitmask: Alt 1, Control 2, Meta 4, Shift 8."""
        p = {"key": name, **({"code": code} if code else {}), **({"modifiers": mods} if mods else {})}
        b.call("Input.dispatchKeyEvent", type="keyDown", **p)
        b.call("Input.dispatchKeyEvent", type="keyUp", **p)

    def facts(sel: str) -> str:
        """Where a film's playhead is, plus where its own JSON says each seek must land."""
        return f"""(() => {{ const f = {sel}, s = f.querySelector('script[type="application/json"]'), d = JSON.parse(s ? s.textContent : '{{}}');
      const times = d.times || [0], n = times.length, ch = d.chapters || [[0, 'start']];
      const tm = (d.series || []).flatMap(sr => sr.t || []).reduce((m, v) => Math.max(m, v), times[n - 1]);
      const end = Math.max(tm > 0 ? tm : 1, times[n - 1]);
      const frameAt = x => {{ let k = 0; for (let j = 0; j < n; j++) if (times[j] <= x + 1e-6) k = j; return k; }};
      const fwd = Math.min(times[0] + 1, end), back = Math.max(0, fwd - 1);
      const cur = f.shadowRoot && f.shadowRoot.querySelector('.fr.current'), k = cur ? +cur.dataset.k : -1;
      const t = f.shadowRoot.querySelector('.t').textContent, tc = parseFloat(t);
      // the chapter the playhead is in, read from the time readout so a seek that lands between
      // frames counts as the chapter it seeked to; the readout is rounded, hence the 5 ms fuzz
      let ci = 0; for (let i = 0; i < ch.length; i++) if (ch[i][0] <= tc + 5e-3) ci = i;
      return {{k, t, n, nch: ch.length, end,
        sec: frameAt(fwd), sec_t: fwd.toFixed(2) + 's', back: frameAt(back), back_t: back.toFixed(2) + 's',
        next: frameAt(ch.length > 1 ? ch[1][0] : end), ci, start: frameAt(ch[ci][0]), prev: frameAt(ch[Math.max(0, ci - 1)][0])}}; }})()"""

    b.js(f"{F}.focus(); 'ok'")
    ks = []
    for key_, code in (("Home", "Home"), (".", None), (".", None), (",", None), ("ArrowRight", "ArrowRight")):
        key(key_, code)
        ks.append(b.js(S)["k"])
    e2.checks.append(Check("keys step frames as documented", ks == [0, 1, 2, 1, 6], f"{ks}"))
    # the playhead is one group moved by a transform; its line keeps its own coordinates at the origin
    PH = f"""(() => {{ const sr = {F}.shadowRoot, g = sr.querySelector('svg.chart > g.playhead'), h = sr.querySelector('.head'), rt = sr.querySelector('.head-t');
      const ty = +sr.querySelector('.chart text.axis[text-anchor="middle"]').getAttribute('y');
      return {{transform: g && g.getAttribute('transform'), head: h && [h.getAttribute('x1'), h.getAttribute('x2')], readout_y: rt && +rt.getAttribute('y'), tick_y: ty}}; }})()"""
    key("Home", "Home")
    ph0 = b.js(PH)
    key("End", "End")
    ph1 = b.js(PH)
    e2.checks += [Check("the playhead group moves by one transform on seek",
                        bool(ph0["transform"]) and ph0["transform"] != ph1["transform"] and str(ph1["transform"]).startswith("translate("),
                        f"{ph0['transform']} -> {ph1['transform']}"),
                  Check("the playhead line keeps its own coordinates at the origin", ph1["head"] == ["0", "0"], f"x1, x2 = {ph1['head']}"),
                  Check("the readout sits in the axis band below the tick labels", ph1["readout_y"] is not None and ph1["readout_y"] > ph1["tick_y"], f"readout y {ph1['readout_y']}, tick labels y {ph1['tick_y']}")]
    key("Home", "Home")
    # Shift plus an arrow seeks a second of film time, not a count of frames
    fd = b.js(facts(F))
    key("Home", "Home"); key("ArrowRight", "ArrowRight", 8)
    sec = b.js(facts(F))
    key("ArrowLeft", "ArrowLeft", 8)
    secb = b.js(facts(F))
    e2.checks += [Check("Shift+ArrowRight seeks one second of film time",
                        sec["k"] == fd["sec"] and sec["t"] == fd["sec_t"],
                        f"frame {sec['k']} at {sec['t']}, expected frame {fd['sec']} at {fd['sec_t']} (film ends at {fd['end']:.2f}s)"),
                  Check("Shift+ArrowLeft seeks a second back",
                        secb["k"] == fd["back"] and secb["t"] == fd["back_t"],
                        f"frame {secb['k']} at {secb['t']}, expected frame {fd['back']} at {fd['back_t']}")]
    # the chapter keys want a film with chapters: the first one on this page that has a second
    cidx = b.js("""(() => { const fs = [...document.querySelectorAll('opt-film')];
      for (let i = 0; i < fs.length; i++) { const s = fs[i].querySelector('script[type="application/json"]');
        if (s && (JSON.parse(s.textContent).chapters || []).length > 1) return i; } return -1; })()""")
    F2 = f"document.querySelectorAll('opt-film')[{cidx if cidx >= 0 else 1}]"
    b.js(f"{F2}.focus(); 'ok'")
    fd2 = b.js(facts(F2))
    e2.note = ((f"Chapter keys run on opt-film[{cidx}], the first film on this page with more than one chapter "
                f"({fd2['nch']} chapters, {fd2['n']} frames)." if cidx >= 0 else
                "No film on this page has a second chapter, so the chapter keys run on opt-film[1] "
                f"({fd2['nch']} chapter, {fd2['n']} frames): with no next chapter PageDown must land on the end.")
               + " Alt+ArrowLeft is not driven because Chrome may take it as history back.")

    def back_to(f: dict) -> list:
        """Where PageUp must land from f: the current chapter's start once the playhead is past
        its first frame, else the previous chapter's start."""
        # the element's rule is exact: past the chapter's first frame goes back to the chapter start
        return [f["start"]] if f["k"] > f["start"] else [f["prev"]]

    def where(f: dict, landed: dict) -> str:
        return (f"from frame {f['k']}, {f['k'] - f['start']} frames past chapter {f['ci']}, to frame {landed['k']}, "
                f"expected {' or '.join(str(x) for x in back_to(f))}")

    key("Home", "Home"); key("PageDown", "PageDown")
    pd = b.js(facts(F2))
    key("PageUp", "PageUp")
    pu = b.js(facts(F2))
    key("PageDown", "PageDown"); key("."); key(".")  # two frames past the chapter start
    mid = b.js(facts(F2))
    key("PageUp", "PageUp")
    pu2 = b.js(facts(F2))
    key("Home", "Home"); key("ArrowRight", "ArrowRight", 2)
    ctl_r = b.js(facts(F2))
    key("ArrowLeft", "ArrowLeft", 2)
    ctl_l = b.js(facts(F2))
    key("Home", "Home"); key("ArrowRight", "ArrowRight", 1)
    alt_r = b.js(facts(F2))
    e2.checks += [
        Check("PageDown seeks to the next chapter" if fd2["nch"] > 1 else "PageDown with no next chapter seeks to the end",
              pd["k"] == fd2["next"],
              f"frame {pd['k']} at {pd['t']}, expected frame {fd2['next']} of {fd2['n']} ({fd2['nch']} chapters)"),
        Check("PageUp on a chapter start seeks to the previous chapter", pu["k"] in back_to(pd), where(pd, pu)),
        Check("PageUp past a chapter start seeks to that chapter's start", pu2["k"] in back_to(mid), where(mid, pu2)),
        Check("Control+ArrowRight and Alt+ArrowRight alias PageDown", ctl_r["k"] == fd2["next"] and alt_r["k"] == fd2["next"],
              f"Control {ctl_r['k']}, Alt {alt_r['k']}, expected {fd2['next']}"),
        Check("Control+ArrowLeft aliases PageUp", ctl_l["k"] in back_to(ctl_r), where(ctl_r, ctl_l))]
    b.js(f"{F}.focus(); 'ok'")  # the chapter film may be elsewhere on the page: put the film under test back in view
    rep.edges.append(e2)
    # E3 peek
    e3 = Edge(("Paused", "Peek", "Paused"), "Peek: hover without seeking", "The pointer over the chart shows a thumbnail and exposes :state(peeking); the playhead stays.")
    cr = b.js(f"(() => {{ const r = {F}.shadowRoot.querySelector('.chart').getBoundingClientRect(); return [r.x, r.y, r.width, r.height]; }})()")
    before = b.js(S)
    b.hover(cr[0] + cr[2] * 0.6, cr[1] + cr[3] * 0.4); b.wait(0.15)
    peek = b.js(S)
    b.hover(2, 2); b.wait(0.1)
    after = b.js(S)
    e3.checks += [Check("peeking exposed while hovering the chart", peek["peeking"] and not after["peeking"]),
                  Check("peek leaves the playhead alone", peek["t"] == before["t"])]
    rep.edges.append(e3)
    # E4 pending seek on the strip
    e4 = Edge(("Paused", "Press", "Paused"), "Press and drag on the strip", "A pending frame with :state(pending); release seeks, Escape cancels.")
    b.js(f"{F}.focus(); 'ok'")
    b.call("Input.dispatchKeyEvent", type="keyDown", key="Home", code="Home"); b.call("Input.dispatchKeyEvent", type="keyUp", key="Home", code="Home")
    # frames 0..2 share the strip's first page; a frame on another page is clipped and cannot be hit
    cells = b.js(f"(() => [...{F}.shadowRoot.querySelectorAll('.fr .cell')].slice(0, 3).map(c => {{ const r = c.getBoundingClientRect(); return [r.x + r.width/2, r.y + r.height/2]; }}))()")
    b.call("Input.dispatchMouseEvent", type="mousePressed", x=cells[1][0], y=cells[1][1], button="left", clickCount=1); b.wait(0.1)
    p1 = b.js(S)
    b.call("Input.dispatchMouseEvent", type="mouseMoved", x=cells[2][0], y=cells[2][1], button="left", buttons=1); b.wait(0.1)
    p2 = b.js(S)
    b.call("Input.dispatchMouseEvent", type="mouseReleased", x=cells[2][0], y=cells[2][1], button="left", clickCount=1); b.wait(0.1)
    p3 = b.js(S)
    b.call("Input.dispatchMouseEvent", type="mousePressed", x=cells[0][0], y=cells[0][1], button="left", clickCount=1); b.wait(0.1)
    b.call("Input.dispatchKeyEvent", type="keyDown", key="Escape", code="Escape"); b.call("Input.dispatchKeyEvent", type="keyUp", key="Escape", code="Escape")
    b.call("Input.dispatchMouseEvent", type="mouseReleased", x=cells[0][0], y=cells[0][1], button="left", clickCount=1); b.wait(0.1)
    p4 = b.js(S)
    e4.checks += [Check("press marks a pending frame with the custom state", p1["pending"] == 1 and p1["pend_state"], f"pending {p1['pending']}"),
                  Check("dragging moves the pending frame, not the playhead", p2["pending"] == 2 and p2["k"] == 0, f"pending {p2['pending']}, playhead {p2['k']}"),
                  Check("release seeks and clears pending", p3["k"] == 2 and p3["pending"] is None and not p3["pend_state"], f"playhead {p3['k']}"),
                  Check("Escape cancels a press", p4["k"] == 2 and p4["pending"] is None, f"playhead {p4['k']}")]
    rep.edges.append(e4)
    # E5 the chart under forced colours, print and high contrast: identity must
    # survive without the palette. Every paint is read, not a sample of them: the
    # shared blocks were once interpolated halfway up the chart stylesheet, so
    # everything written after them - the playhead, its dot and its readout, the
    # track, the played bar, the chapter tick and the peek rule - put its token
    # straight back on a forced palette and in print, at equal specificity and in
    # silence. The checks that were here read the series, the markers, the end
    # label and the grid, all of them written before the blocks, so they passed
    # throughout. The paints written after them are read below.
    e5 = Edge(("Paused", "Peek", "Paused"), "Forced colours, print and high contrast",
              "With forced-colors active every paint maps to a system colour, markers appear on every series and dashes carry identity; "
              "in print emulation every paint maps to a print black or grey and the peek rule goes; with prefers-contrast more the grid "
              "reaches the strong border and strokes thicken. Each paint is read off the rendered node, not off the stylesheet.")
    CH = f"""(() => {{ const sr = {F}.shadowRoot; const cs = e => e && getComputedStyle(e);
      const p1 = sr.querySelector('path[class^=series].series-1'), p2 = sr.querySelector('path[class^=series].series-2') || p1, m = sr.querySelector('.marker'), g = sr.querySelector('.grid'), lab = sr.querySelector('.endlabel');
      const head = sr.querySelector('.head'), dot = sr.querySelector('.head-dot'), ht = sr.querySelector('.head-t');
      const bg = sr.querySelector('.bar-bg'), played = sr.querySelector('.bar-played'), ch = sr.querySelector('.chapter'), peek = sr.querySelector('.peek-line');
      const token = n => getComputedStyle({F}).getPropertyValue(n).trim();
      return {{s1: cs(p1).stroke, s2: cs(p2).stroke, dash2: cs(p2).strokeDasharray, w1: cs(p1).strokeWidth, marker: m ? cs(m).display : null, grid: cs(g).stroke, label: lab ? cs(lab).fill : null,
        head: head ? cs(head).stroke : null, dot: dot ? cs(dot).fill : null, readout: ht ? cs(ht).fill : null,
        track: bg ? cs(bg).fill : null, played: played ? cs(played).fill : null,
        chapter: ch ? cs(ch).stroke : null, peek: peek ? cs(peek).stroke : null, peek_shown: peek ? cs(peek).display : null,
        token1: token('--op-series-1'), strong: token('--op-border-strong'), accent: token('--op-accent'),
        border: token('--op-border'), peek_token: token('--op-peek')}}; }})()"""
    normal = b.js(CH)
    # the peek rule's token, witnessed by moving it: --op-peek is declared per
    # theme, registered and contrast-tested, and it carried the same value as
    # --op-muted, so a rule painted with the wrong one of the two looked right.
    # Nothing but changing one of them can tell them apart in a browser.
    b.js(f"{F}.style.setProperty('--op-peek', 'rgb(1, 2, 3)'); 'ok'")
    b.wait(0.2)
    tinted = b.js(CH)
    b.js(f"{F}.style.removeProperty('--op-peek'); 'ok'")
    b.wait(0.2)
    untinted = b.js(CH)
    b.call("Emulation.setEmulatedMedia", features=[{"name": "forced-colors", "value": "active"}])
    b.wait(0.2)
    forced = b.js(CH)
    b.call("Emulation.setEmulatedMedia", features=[{"name": "forced-colors", "value": "none"}, {"name": "prefers-contrast", "value": "more"}])
    b.wait(0.2)
    more = b.js(CH)
    b.call("Emulation.setEmulatedMedia", features=[{"name": "forced-colors", "value": "none"}, {"name": "prefers-contrast", "value": "no-preference"}])
    b.wait(0.2)
    # the print mapping is the same question with a different answer, asked the
    # way the browser asks it of a page it is about to print
    b.call("Emulation.setEmulatedMedia", media="print")
    b.wait(0.2)
    printed = b.js(CH)
    b.call("Emulation.setEmulatedMedia", media="")
    b.wait(0.2)
    back = b.js(CH)

    def rgb(value):
        """A token's computed value as the browser reports paints: registered <color> properties
        already come back as rgb(...); a bare hex is converted."""
        v = value.strip()
        if v.startswith("#") and len(v) == 7:
            return "rgb(%d, %d, %d)" % tuple(int(v[i:i + 2], 16) for i in (1, 3, 5))
        return v

    # the print blocks the stylesheet names, as the browser reports them
    PRINT_BLACK, PRINT_TRACK, PRINT_PLAYED = "rgb(0, 0, 0)", "rgb(221, 221, 221)", "rgb(85, 85, 85)"
    e5.checks += [Check("series stroke resolves to its palette token", normal["s1"] == rgb(normal["token1"]), f"{normal['s1']} vs token {normal['token1']}"),
                  Check("the second series carries a dash pattern", normal["dash2"] not in (None, "", "none"), f"dasharray {normal['dash2']}"),
                  Check("markers are hidden for labelled series in normal mode", normal["marker"] == "none", f"display {normal['marker']}"),
                  Check("the peek rule is painted with the peek token and nothing else is",
                        tinted["peek"] == "rgb(1, 2, 3)" and tinted["s1"] == normal["s1"] and tinted["grid"] == normal["grid"]
                        and untinted["peek"] == normal["peek"],
                        f"moving --op-peek to rgb(1, 2, 3) moved the peek rule {normal['peek']} -> {tinted['peek']} and put it back "
                        f"at {untinted['peek']}; the token reads {normal['peek_token']} and the series and grid did not move"),
                  Check("forced colours replace the palette stroke with a system colour", forced["s1"] != normal["s1"] and forced["s1"] == forced["s2"], f"{normal['s1']} -> {forced['s1']} / {forced['s2']}"),
                  Check("forced colours keep the dash pattern as the identity cue", forced["dash2"] == normal["dash2"], f"{forced['dash2']}"),
                  Check("forced colours show the markers", forced["marker"] == "inline", f"display {forced['marker']}"),
                  Check("forced colours put the playhead, its dot and the played bar on one system colour",
                        forced["head"] == forced["dot"] == forced["played"] and forced["head"] != rgb(normal["accent"]),
                        f"playhead {normal['head']} -> {forced['head']}, its dot {forced['dot']}, the played bar "
                        f"{normal['played']} -> {forced['played']}, none of them the accent token {normal['accent']}"),
                  Check("forced colours put the track on the same subordinate colour as the grid",
                        forced["track"] == forced["grid"] and forced["track"] != rgb(normal["border"]),
                        f"track {normal['track']} -> {forced['track']}, grid {forced['grid']}, border token {normal['border']}"),
                  Check("forced colours put the peek rule, the readout and the chapter tick on the text colour",
                        forced["peek"] == forced["s1"] and forced["readout"] == forced["s1"] and forced["chapter"] in (None, forced["s1"]),
                        f"peek rule {normal['peek']} -> {forced['peek']}, readout {normal['readout']} -> {forced['readout']}, chapter tick "
                        f"{forced['chapter'] or 'not drawn: this film has one chapter, so the tick is left to the Rust ordering test'}, "
                        f"series {forced['s1']}"),
                  Check("print puts the playhead, its dot, its readout and the chapter tick on black",
                        {printed["head"], printed["dot"], printed["readout"]} == {PRINT_BLACK} and printed["chapter"] in (None, PRINT_BLACK),
                        f"playhead {normal['head']} -> {printed['head']}, its dot {printed['dot']}, its readout {printed['readout']}, "
                        f"chapter tick {printed['chapter'] or 'not drawn by this film'}"),
                  Check("print puts the track and the played bar on its own greys",
                        (printed["track"], printed["played"]) == (PRINT_TRACK, PRINT_PLAYED),
                        f"track {normal['track']} -> {printed['track']}, played bar {normal['played']} -> {printed['played']}"),
                  Check("print drops the peek rule altogether", printed["peek_shown"] == "none",
                        f"the peek rule's display is {printed['peek_shown']} in print and {normal['peek_shown']} on screen"),
                  Check("high contrast raises the grid to the strong border", more["grid"] == rgb(more["strong"]) and more["grid"] != normal["grid"], f"{normal['grid']} -> {more['grid']} (strong {more['strong']})"),
                  Check("high contrast thickens the strokes", float(more["w1"].rstrip("px")) > float(normal["w1"].rstrip("px")), f"{normal['w1']} -> {more['w1']}"),
                  Check("the emulation is reset afterwards",
                        back["s1"] == normal["s1"] and back["marker"] == normal["marker"] and back["head"] == normal["head"]
                        and back["peek_shown"] == normal["peek_shown"],
                        f"series {back['s1']}, markers {back['marker']}, playhead {back['head']}, peek rule display {back['peek_shown']}")]
    rep.edges.append(e5)
    # E6 the recorded video on the stage follows the film's clock
    e6 = Edge(("Paused", "Play", "Playing"), "The stage plays the recording",
              "A film with a video plays it on its own clock: the video's time tracks the readout while playing, lands exactly on a seek, and pauses with the film.")
    VS = f"""(() => {{ const f = {F}, sr = f.shadowRoot, v = sr.querySelector('video.stagevideo');
      const s = f.querySelector('script[type="application/json"]'), times = ((s ? JSON.parse(s.textContent).times : null) || [0]);
      return {{present: !!v, ready: v ? v.readyState : -1, shown: v ? getComputedStyle(v).display : null, vt: v ? v.currentTime : null, paused: v ? v.paused : null,
        t: parseFloat(sr.querySelector('.t').textContent), src: v ? v.currentSrc : null, w: v ? v.videoWidth : 0,
        muted: v ? v.muted : null, dur: v ? v.duration : null, last: times[times.length - 1]}}; }})()"""
    b.js(f"{F}.focus(); 'ok'")
    key("Home", "Home")
    v0 = b.js(VS)
    if v0["present"]:
        # a media element decodes on the real clock whichever clock the page runs
        # on, so this wait is a real one; the synthetic run waits for the terminal
        # readyState, which a fully fetched local file reaches every time
        want_ready = 4 if b.synthetic else 2
        for _ in range(40):
            if b.js(VS)["ready"] >= want_ready:
                break
            time.sleep(0.1)
            b.frame()
        v0 = b.js(VS)
        b.js(f"{F}.shadowRoot.querySelector('button.play').click(); 'ok'")
        b.wait(0.9)
        v1 = b.js(VS)
        b.js(f"{F}.shadowRoot.querySelector('button.play').click(); 'ok'")
        b.wait(0.15)
        v2 = b.js(VS)
        key("End", "End")
        b.wait(0.3)
        v3 = b.js(VS)
        dur = v0["dur"] if isinstance(v0["dur"], (int, float)) else None
        last = v0["last"] if isinstance(v0["last"], (int, float)) else 0.0
        e6.checks.append(Check("the recording loads and is shown over the stage", v0["ready"] >= 2 and v0["shown"] == "block" and v0["w"] > 0, f"readyState {v0['ready']}, display {v0['shown']}, {v0['w']}px wide"))
        if b.synthetic:
            # the video plays on the real clock while the film plays on the synthetic
            # one, so the drift between them while playing is not a property of the
            # page and is not measured here; where the film puts the video when it
            # stops is, and so are the recording's own facts
            e6.note = ("Under the synthetic clock the media element still runs on real time, so the check that the "
                       "video tracks the film's clock while playing is not emitted: it would measure the machine, not "
                       "the page. What the film owns is measured instead: where it leaves the video when it stops, and "
                       "the recording it was given.")
            e6.checks += [Check("pausing the film pauses the video", v2["paused"] is True, f"paused {v2['paused']}"),
                          Check("a paused film puts the video on its own time", v2["vt"] is not None and abs(v2["vt"] - v2["t"]) < 0.15, f"video {v2['vt']:.2f}s vs readout {v2['t']:.2f}s"),
                          Check("the recording is muted and as long as the film", v0["muted"] is True and dur is not None and last <= dur <= last + 1.0,
                                f"muted {v0['muted']}, duration {'none' if dur is None else format(dur, '.2f')}s for a film ending at {last:.2f}s"),
                          Check("a seek lands the video on the film's time", abs(v3["vt"] - v3["t"]) < 0.05 and v3["paused"] is True, f"video {v3['vt']:.2f}s vs readout {v3['t']:.2f}s")]
        else:
            e6.checks += [Check("while playing the video's time tracks the film's clock", v1["paused"] is False and v1["vt"] > 0.3 and abs(v1["vt"] - v1["t"]) < 0.2, f"video {v1['vt']:.2f}s vs readout {v1['t']:.2f}s"),
                          Check("pausing the film pauses the video", v2["paused"] is True, f"paused {v2['paused']}"),
                          Check("a seek lands the video on the film's time", abs(v3["vt"] - v3["t"]) < 0.05 and v3["paused"] is True, f"video {v3['vt']:.2f}s vs readout {v3['t']:.2f}s")]
    else:
        e6.checks.append(Check("this film carries a recording", False, "no video element on the stage: the run that produced this page had no screencast or no ffmpeg"))
    rep.edges.append(e6)
    # E7 the chart under every emulation: a screenshot each, and the series must stay apart in all of them
    e7 = Edge(("Paused", "Peek", "Paused"), "Colour vision and forced palettes",
              "The chart captured under forced colours with both system palettes and under the four vision deficiencies; in each, the rendered series stay pairwise apart (CIEDE2000 at least 8), or, where every stroke is one system colour, their dash patterns differ.")
    from PIL import Image
    import io
    GEOM = f"""(() => {{ const sr = {F}.shadowRoot, svg = sr.querySelector('svg.chart'); const r = svg.getBoundingClientRect(); const vb = svg.viewBox.baseVal;
      const series = [...sr.querySelectorAll('path[class^=series]')].map(p => {{ const cls = p.getAttribute('class');
        const sw = sr.querySelector('line.swatch.' + cls);
        // the end label's swatch is a solid stroke in the series colour that label spreading keeps clear of every other series;
        // without one (an unlabelled series) fall back to the path's own points, the numbers between its M and L commands
        const own = (p.getAttribute('d') || '').split(/[ML\\s]+/).filter(s => s.length).map(Number);
        const pts = sw ? [[(+sw.getAttribute('x1') + +sw.getAttribute('x2')) / 2, +sw.getAttribute('y1')]] : own.filter((_, i) => i % 2 === 0).map((x, i) => [x, own[2 * i + 1]]);
        const paint = getComputedStyle(sw || p).stroke;
        return {{cls, pts, paint}}; }});
      return {{rect: [r.x, r.y, r.width, r.height], vb: [vb.width, vb.height], series, scroll: [window.scrollX, window.scrollY]}}; }})()"""
    # the dash patterns are read under whichever emulation is in force
    DASHES = f"""(() => Object.fromEntries([...{F}.shadowRoot.querySelectorAll('path[class^=series]')].map(p => [p.getAttribute('class'), getComputedStyle(p).strokeDasharray])))()"""
    STROKE1 = f"""getComputedStyle({F}.shadowRoot.querySelector('path[class^=series]')).stroke"""
    b.js(f"{F}.scrollIntoView({{block: 'center'}}); 'ok'")
    # the pointer's last position left the film peeking; the thumbnail would cover the chart's edge
    b.hover(2, 2)
    b.wait(0.3)
    geom = b.js(GEOM)
    baseline_stroke = b.js(STROKE1)
    rx, ry, rw, rh = geom["rect"]
    sx, sy = rw / geom["vb"][0], rh / geom["vb"][1]
    x0 = rx + (geom["scroll"][0] if b.clip_uses_page_coords else 0)
    y0 = ry + (geom["scroll"][1] if b.clip_uses_page_coords else 0)

    def rgb_of(css):
        return tuple(int(v) for v in re.findall(r"\d+", css)[:3]) if css and css.startswith("rgb") else None

    def emulate(how):
        b.call("Emulation.setEmulatedMedia", features=how["media"])
        b.call("Emulation.setEmulatedVisionDeficiency", type=how["vision"])
        b.wait(0.25)

    def capture(mode):
        if b.synthetic:
            # from the frame itself, at the device scale, exactly the chart's box
            png = b.frame_bytes((rx, ry, rw, rh), margin=0, scale=DPR)
        else:
            shot = b.call("Page.captureScreenshot", format="png", clip={"x": x0, "y": y0, "width": rw, "height": rh, "scale": 1.0})
            png = base64.b64decode(shot["data"])
        (d / f"matrix-{mode}.png").write_bytes(png)
        return Image.open(io.BytesIO(png)).convert("RGB")

    files = []
    probes = {}  # series class -> the pixel proven to be its own stroke in the unemulated capture
    try:
        for mode, how in MATRIX:
            emulate(how)
            img = capture(mode)
            files.append((mode, f"matrix-{mode}.png"))
            ksx, ksy = img.width / rw, img.height / rh
            if mode == "none":
                # a pixel near each series' swatch that carries that series' own paint (a stroke
                # edge is anti-aliased, so a small distance is accepted; a neighbour is far beyond it)
                for sr in geom["series"]:
                    want = rgb_of(sr["paint"])
                    best, best_d = None, 8.0
                    for (px, py) in sr["pts"]:
                        ix, iy = int(px * sx * ksx), int(py * sy * ksy)
                        for dx in range(-2, 3):
                            for dy in range(-2, 3):
                                if want and 0 <= ix + dx < img.width and 0 <= iy + dy < img.height:
                                    c = img.getpixel((ix + dx, iy + dy))
                                    dist = ciede2000(_srgb_to_lab(c), _srgb_to_lab(want))
                                    if dist < best_d:
                                        best, best_d = (ix + dx, iy + dy), dist
                    if best:
                        probes[sr["cls"]] = best
            colours = {cls: img.getpixel(px) for cls, px in probes.items()}
            dashes = b.js(DASHES)
            names = sorted(colours)
            worst, worst_pair = float("inf"), ""
            for i in range(len(names)):
                for j in range(i + 1, len(names)):
                    dist = ciede2000(_srgb_to_lab(colours[names[i]]), _srgb_to_lab(colours[names[j]]))
                    if dist < worst:
                        worst, worst_pair = dist, f"{names[i]} vs {names[j]}"
            if mode.startswith("forced") or mode == "achromatopsia":
                # one system colour, or no hue at all: identity rests on the dash table
                distinct = len(set(dashes.values())) == len(dashes)
                e7.checks.append(Check(f"{mode}: dashes tell the series apart", distinct and len(dashes) >= 2,
                                       f"{len(dashes)} series, dashes {sorted(set(dashes.values()))}"))
            else:
                all_probed = len(names) == len(geom["series"]) and len(names) >= 2
                e7.checks.append(Check(f"{mode}: rendered series stay pairwise apart", all_probed and worst >= MIN_SERIES_SEPARATION,
                                       f"{len(names)} of {len(geom['series'])} series probed; closest pair {worst_pair} at dE00 {worst:.1f}" if names else "no series probed"))
    finally:
        emulate(MATRIX_RESET)
    e7.checks.append(Check("the emulations are reset afterwards", b.js(STROKE1) == baseline_stroke, f"first series stroke {b.js(STROKE1)} vs {baseline_stroke} before"))
    e7.matrix = files
    rep.edges.append(e7)
    return rep

# ----------------------------------------------------------------------------
def fnv1a64_hex(text: str) -> str:
    """The build's content hash of a chart's data block, recomputed here as an
    independent check: FNV-1a 64 over the block with ASCII whitespace trimmed
    at both ends, sixteen lowercase hex digits (op_chart::data::hash_hex)."""
    h = 0xCBF29CE484222325
    for byte in text.strip(" \t\n\x0c\r").encode("utf-8"):
        h = ((h ^ byte) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"{h:016x}"


assert fnv1a64_hex("") == "cbf29ce484222325" and fnv1a64_hex("a") == "af63dc4c8601ec8c"


def run_chart(b: Browser, base: str, ctrl: dict, out: Path) -> ControlReport:
    """The chart kind: a pre-rendered <opt-chart> that follows a film's clock.
    Static first (scripts off, two widths), then upgrade, a full play and resize,
    then the gestures: a tap, a drag, Escape mid-drag, a hover, the key table, the
    focus rule those keys obey and a coarse long press, each read off the film's
    own clock."""
    tag = ctrl["tag"]
    rep = ControlReport(tag=tag, kind="chart", page=ctrl["page"], nodes=["Static", "Following"],
                        machine_edges=[("Static", "Upgrade", "Following"), ("Following", "Tick", "Following"),
                                       ("Following", "Resize", "Following"), ("Static", "Resize", "Static"),
                                       # the intents a gesture or a key carries: the chart asks, the film answers
                                       ("Following", "Seek", "Following"), ("Following", "Peek", "Following"),
                                       ("Following", "Cancel", "Following"), ("Following", "Toggle", "Following")])
    d = out / tag
    d.mkdir(parents=True, exist_ok=True)
    b.reduced_motion(False)
    C = "document.querySelector('opt-chart')"
    F = "document.getElementById('chart-film')"
    M = "document.querySelector('opt-machine[for=\"chart-film\"]')"
    # everything the checks read from the chart, in one evaluation
    PROBE = f"""(() => {{ const c = {C}; const sr = c && c.shadowRoot; if (!sr) return {{root: false}};
      const st = n => c.matches(':state(' + n + ')');
      const svgs = [...sr.querySelectorAll('svg.chart')];
      const vis = svgs.find(s => s.getBoundingClientRect().width > 0) || svgs[0];
      const vb = vis ? vis.viewBox.baseVal : null, r = vis ? vis.getBoundingClientRect() : null;
      const scale = vis && vb.width ? r.width / vb.width : 0;
      const px = sel => [...vis.querySelectorAll(sel)].filter(t => getComputedStyle(t).display !== 'none').map(t => parseFloat(getComputedStyle(t).fontSize));
      const eff = sel => px(sel).map(v => +(v * scale).toFixed(2));
      const head = vis && vis.querySelector('g.playhead'), ht = vis && vis.querySelector('.head-t'), bar = vis && vis.querySelector('.bar-bg'), played = vis && vis.querySelector('.bar-played');
      const tx = head ? (head.getAttribute('transform') || '').match(/translate\\(([-\\d.]+)/) : null;
      const summary = sr.querySelector('figcaption .summary'), table = sr.querySelector('details.data table');
      const film = {F}, ft = film && film.shadowRoot && film.shadowRoot.querySelector('.t');
      const mach = {M}, tok = mach && mach.shadowRoot && mach.shadowRoot.querySelector('.token');
      return {{root: true, defined: !!customElements.get('opt-chart'), hydrated: st('hydrated'), following: st('following'),
        svgs: svgs.length, by: vis ? vis.dataset.renderedBy : null, vbw: vb ? vb.width : null, cssw: r ? +r.width.toFixed(1) : null,
        ticks: vis ? eff('.tick-label') : [], ends: vis ? eff('.endlabel') : [], tick_px: vis ? px('.tick-label') : [],
        summary: summary ? summary.textContent.trim().length : 0, rows: table ? table.querySelectorAll('tbody tr').length : 0,
        hash: c.getAttribute('data-hash'), block: (c.querySelector('script[type="application/json"]') || {{}}).textContent || '',
        x: tx ? +tx[1] : null, readout: ht ? ht.textContent : null, bar: bar ? [+bar.getAttribute('x'), +bar.getAttribute('width')] : null,
        played: played ? +played.getAttribute('width') : null, film_t: ft ? ft.textContent : null,
        token: tok ? [+tok.getAttribute('cx'), +tok.getAttribute('cy')] : null}}; }})()"""
    LOC = f"(() => {{ const el = {C}; el.scrollIntoView({{block: 'center'}}); const r = el.getBoundingClientRect(); return [r.x, r.y, r.width, r.height]; }})()"

    # E1 static: scripts off, the declarative pre-render at two widths
    e1 = Edge(("Static", "Resize", "Static"), "Static: the pre-render without script",
              "With script execution disabled the page still holds the finished chart in a declarative shadow root: the svg the build rendered, "
              "the caption with its summary and the data table, at 640 px and at 360 px, where the narrow variant keeps every label at ten pixels or more.")
    e1.matrix_title = "The pre-render without script"
    b.call("Emulation.setScriptExecutionDisabled", value=True)
    try:
        for w in (640, 360):
            b.call("Emulation.setDeviceMetricsOverride", width=w, height=1000, deviceScaleFactor=0, mobile=False)
            b.goto(base + ctrl["page"], f"!!{C}")
            b.wait(0.3)
            p = b.js(PROBE)
            rect = b.js(LOC)
            b.wait(0.15)
            b.save_frame(d / f"nojs-{w}.png", rect, scale=1)
            e1.matrix.append((f"no JavaScript, {w} px viewport", f"nojs-{w}.png"))
            labels = (p.get("ticks") or []) + (p.get("ends") or [])
            smallest = min(labels) if labels else 0
            block_hash = fnv1a64_hex(p.get("block", ""))
            # each viewport is served by the pre-render drawn for it: 640 by the wide one, 360 by the narrow
            e1.checks += [Check(f"{w} px: declarative root before any script", p.get("root") and not p.get("defined"), f"root={p.get('root')} defined={p.get('defined')}"),
                          Check(f"{w} px: the build's svg is shown", p.get("by") == "op-pages" and p.get("vbw") == w, f"rendered-by={p.get('by')} viewBox width={p.get('vbw')} css width={p.get('cssw')}"),
                          Check(f"{w} px: caption, summary and table present", p.get("summary", 0) > 40 and p.get("rows", 0) >= 8, f"summary {p.get('summary')} chars, {p.get('rows')} table rows"),
                          Check(f"{w} px: every label at least 10 px effective", labels and smallest >= 10, f"smallest {smallest} px of {len(labels)} labels"),
                          Check(f"{w} px: data-hash matches the block", bool(p.get("hash")) and p.get("hash") == block_hash, f"{p.get('hash')} vs {block_hash}")]
    finally:
        b.call("Emulation.setScriptExecutionDisabled", value=False)
        b.call("Emulation.clearDeviceMetricsOverride")
    rep.edges.append(e1)

    # E2 upgrade: hydration keeps the pre-render, then one live svg at the measured width
    READY = f"!!customElements.get('opt-chart') && {C}.matches(':defined') && !!{F}.shadowRoot?.querySelector('.stage')"
    e2 = Edge(("Static", "Upgrade", "Following"), "Upgrade: hydration keeps the pre-render",
              "When the element upgrades it finds the declarative root and the matching data-hash and keeps the markup (the hydrated state); "
              "on the page the build rendered for, the wide pre-rendered svg fits the container, so it survives upgrade untouched and only its narrow sibling is removed.")
    b.goto(base + ctrl["page"], READY)
    b.wait(0.5)
    p2 = b.js(PROBE)
    rect = b.js(LOC)
    e2.checks += [Check("element defined and hydrated from the declarative root", p2.get("defined") and p2.get("hydrated"), f"defined={p2.get('defined')} hydrated={p2.get('hydrated')}"),
                  Check("the build's svg survives the upgrade", p2.get("svgs") == 1 and p2.get("by") == "op-pages", f"{p2.get('svgs')} svg(s), rendered-by={p2.get('by')}"),
                  Check("viewBox equals the CSS box", p2.get("vbw") is not None and p2.get("cssw") is not None and abs(p2["vbw"] - p2["cssw"]) <= 1, f"viewBox {p2.get('vbw')} vs css {p2.get('cssw')}")]
    rep.edges.append(e2)

    # E3 tick: a full play; the chart's playhead tracks the film's clock and the machine still follows
    duration = 0.0
    try:
        duration = float(json.loads(p2.get("block") or "{}").get("duration", 0))
    except ValueError:
        pass
    e3 = Edge(("Following", "Tick", "Following"), "Tick: the chart follows the film through a full play",
              "The film plays to its end; at every sample the chart's readout matches the film's clock, its playhead moves monotonically to where the time maps on the plot, "
              "and the machine bound to the same film moves its token.")
    film = b.film_start(rect)
    t0 = b.now()
    # the sampling period: a frame on the synthetic clock, where every frame is a sample
    period = INTERVAL_MS / 1000 if b.synthetic else 0.05
    b.js(f"{F}.shadowRoot.querySelector('button.play').click(); 'ok'")
    rows = sample(b, PROBE, duration + 0.6, period=period, t0=t0, film=film, rect=rect)
    b.js(f"(() => {{ const f = {F}; if (f.matches(':state(playing)')) f.shadowRoot.querySelector('button.play').click(); return 'ok'; }})()")
    xs = [s["x"] for _, s in rows if s.get("x") is not None]
    travel = [(x - xs[0]) / max(1e-6, xs[-1] - xs[0]) * 100 for x in xs] if xs else []
    make_film(e3, film, d, "follow", keys=6, trace=[(0.0, "Following", "Tick", "Following")],
              series=[{"label": "playhead travel", "series": SERIES["thumb"], "t": [t for t, _ in rows], "y": travel, "lw": 2.4, "at": 0.5}] if travel else None)

    def seconds(text):
        try:
            return float(str(text).replace("s", "").strip())
        except ValueError:
            return None

    pairs = [(t, seconds(s["readout"]), seconds(s["film_t"])) for t, s in rows if seconds(s.get("readout")) is not None and seconds(s.get("film_t")) is not None]
    behind = [f - c for _, c, f in pairs]
    # the chart draws the tick one animation frame after the film wrote its own readout, so it
    # may trail by one frame; the frame interval under the recording is measured from the film's
    # own readout changes rather than assumed, and the bound adds the sampling period and the
    # 0.01 s readout rounding
    changes = [t for (t, _, f), (_, _, fp) in zip(pairs[1:], pairs) if f != fp]
    frame = max((b_ - a for a, b_ in zip(changes, changes[1:])), default=period)
    allowed = frame + period + 0.011
    bar = p2.get("bar") or [0, 0]
    # the playhead is judged against the chart's own readout, the time it drew, not the film's
    # clock: the chart draws one animation frame after the tick, and under the recording that
    # frame can lag the film by tens of milliseconds, which the trailing check above bounds
    expected = [(bar[0] + min(1.0, max(0.0, (seconds(s["readout"]) or 0) / duration)) * bar[1], s["x"]) for _, s in rows if s.get("x") is not None and seconds(s.get("readout")) is not None and duration > 0]
    off = [abs(a - x) for a, x in expected]
    monotone = all(b_ >= a - 1e-6 for a, b_ in zip(xs, xs[1:]))
    tokens = [tuple(s["token"]) for _, s in rows if s.get("token")]
    last = rows[-1][1] if rows else {}
    # the measurements are reported at the resolution they are judged at: a millisecond
    # of clock and a hundredth of a pixel of plot, not the last digit of a double
    ahead_s = f"{-min(behind):.3f}" if behind else "n/a"
    behind_s = f"{max(behind):.3f}" if behind else "n/a"
    off_px = f"{max(off):.2f}" if off else "n/a"
    e3.checks += [Check("readout never runs ahead of the film", behind and min(behind) >= -0.011, f"chart ahead by at most {ahead_s} s"),
                  Check("readout trails the film by at most one frame", behind and max(behind) <= allowed, f"max {behind_s} s behind, frame interval {frame:.3f} s, allowed {allowed:.3f} s, {len(behind)} samples"),
                  Check("playhead moves monotonically", len(xs) > 5 and monotone, f"{len(xs)} positions, {xs[0] if xs else '-'} to {xs[-1] if xs else '-'}"),
                  # the readout is rounded to 0.01 s, which is up to 1.8 px of plot at this duration
                  Check("playhead sits where its readout maps on the plot", off and max(off) <= 2.0, f"max offset {off_px} px"),
                  Check("following the film once its ticks arrive", bool(last.get("following")), f"following={last.get('following')}"),
                  Check("the play reached the end", (seconds(last.get("film_t")) or 0) >= duration - 0.05, f"film at {last.get('film_t')} of {duration}s"),
                  Check("the machine bound to the film moved its token", len(set(tokens)) > 1, f"{len(set(tokens))} distinct token positions")]
    rep.edges.append(e3)

    # E4 resize: the live chart re-lays out at the measured width and thins its labels when narrow
    e4 = Edge(("Following", "Resize", "Following"), "Resize: the live chart re-lays out",
              "Loaded at a 900 px viewport and then narrowed to 360 px without a reload, the svg's viewBox equals its CSS width at both widths, "
              "tick labels keep their 12 px token size, the narrowed chart is a live render, and below 480 px every second tick label is hidden.")
    counts = {}
    try:
        for i, w in enumerate((900, 360)):
            b.call("Emulation.setDeviceMetricsOverride", width=w, height=1000, deviceScaleFactor=0, mobile=False)
            if i == 0:
                b.goto(base + ctrl["page"], READY)
            b.wait(0.6)
            p = b.js(PROBE)
            counts[w] = p
            e4.checks += [Check(f"{w} px: viewBox equals the CSS box", p.get("vbw") is not None and p.get("cssw") is not None and abs(p["vbw"] - p["cssw"]) <= 1, f"viewBox {p.get('vbw')} vs css {p.get('cssw')}"),
                          Check(f"{w} px: tick labels keep their token size", p.get("tick_px") and all(abs(v - 12) < 0.5 for v in p["tick_px"]), f"{sorted(set(p.get('tick_px') or []))} px")]
        e4.checks.append(Check("narrowing a live chart re-renders it in place", counts[360].get("svgs") == 1 and counts[360].get("by") == "op-site", f"{counts[360].get('svgs')} svg(s), rendered-by={counts[360].get('by')}"))
    finally:
        b.call("Emulation.clearDeviceMetricsOverride")
    wide, narrow = len(counts.get(900, {}).get("tick_px") or []), len(counts.get(360, {}).get("tick_px") or [])
    e4.checks.append(Check("every second tick label hidden when narrow", wide > 0 and 0 < narrow <= (wide + 1) // 2, f"{wide} labels wide, {narrow} narrow"))
    rep.edges.append(e4)

    # ---- the gestures: real pointer and key input, answered by the film ------
    # The chart moves nothing itself. A press or a key emits opt-chart-seek,
    # opt-chart-peek or opt-chart-toggle; the film's clock answers; the chart
    # follows that answer like any other tick. So what a gesture did is read off
    # the film's own readout, which is the authority for the clock, and the chart's
    # readout is held against it. The thresholds are the element's own, from
    # crates/op-site/src/components/chart.rs: how far along x a position may be
    # from a sample and still mean it, how far a press may travel and still be a
    # tap, how long a coarse press is held before it aims, and how near its own
    # origin an aiming drag has to come back to be a change of mind.
    SNAP_RADIUS, DRAG_PX, LONG_PRESS, SNAPBACK_PX = 40.0, 3.0, 0.5, 4.0
    # the one chart svg that is on screen: the element ships a wide chart and a
    # narrow one and the container query hides whichever does not fit, so the
    # hidden one has no box at all. Every script below opens by finding it, and
    # a gesture aimed at the hidden chart would land nowhere.
    SHOWN = "[...%s.querySelectorAll('svg.chart')].find(e => e.getBoundingClientRect().width > 0)"
    b.goto(base + ctrl["page"], READY)
    b.wait(0.4)
    rect = b.js(LOC)  # scrolls the chart into view; nothing below scrolls again
    # the chart's own geometry: the box a client coordinate is measured in, the
    # viewBox its units are, and the track bar, whose x and width are the plot's
    # time axis (E3 judges the playhead against the same pair). The pointer
    # capabilities come with it, because the element gates its hover preview on them.
    GEOM = f"""(() => {{ const c = {C}, sr = c.shadowRoot;
      const svg = {SHOWN % 'sr'};
      if (!svg) return null;
      const r = svg.getBoundingClientRect(), vb = svg.viewBox.baseVal, bar = svg.querySelector('.bar-bg');
      return {{x: r.x, y: r.y, w: r.width, h: r.height, vbw: vb.width, vbh: vb.height,
        bar: [+bar.getAttribute('x'), +bar.getAttribute('width')], track: +bar.getAttribute('y') + 2,
        block: (c.querySelector('script[type="application/json"]') || {{}}).textContent || '',
        fine: matchMedia('(pointer: fine)').matches, coarse: matchMedia('(pointer: coarse)').matches,
        no_pointer: matchMedia('(pointer: none)').matches, hover: matchMedia('(hover: hover)').matches,
        touches: navigator.maxTouchPoints}}; }})()"""
    # what a gesture has done: the three custom states, the readout (the preview's
    # time while one is showing, the clock's otherwise), the playhead, the preview
    # rule, the film's clock and its playing state, and whether the chart has the focus
    GEST = f"""(() => {{ const c = {C}, f = {F}, sr = c.shadowRoot;
      const svg = {SHOWN % 'sr'};
      const head = svg && svg.querySelector('g.playhead'), ht = svg && svg.querySelector('.head-t');
      const tx = head ? (head.getAttribute('transform') || '').match(/translate\\(([-\\d.]+)/) : null;
      const line = svg && svg.querySelector('.peek-line');
      return {{peeking: c.matches(':state(peeking)'), pending: c.matches(':state(pending)'),
        cancelling: c.matches(':state(cancelling)'),
        readout: ht ? ht.textContent : null, x: tx ? +tx[1] : null,
        rule: line && line.getAttribute('visibility') !== 'hidden' ? +line.getAttribute('x1') : null,
        film_t: f.shadowRoot.querySelector('.t').textContent, playing: f.matches(':state(playing)'),
        focused: sr.activeElement === svg}}; }})()"""
    # Every event the page receives is recorded with its isTrusted, so a check can
    # say the gesture came from the browser's input pipeline: an element.click() and
    # a dispatched event are both untrusted, and a report that drove the chart that
    # way would say so here rather than pass. A key also carries where it went: the
    # svg on its composed path, and the element, whose own listener is the one the
    # focus rule refuses a key at.
    b.js("(() => { const c = " + C + ";\n"
         "      const svg = " + SHOWN % "c.shadowRoot" + ";\n"
         """      const rec = window.__opgest = {down: null, up: null, key: null};
      const seen = e => ({trusted: e.isTrusted, kind: e.pointerType || '', chart: e.composedPath().indexOf(svg) >= 0});
      document.addEventListener('pointerdown', e => { rec.down = seen(e); }, true);
      document.addEventListener('pointerup', e => { rec.up = seen(e); }, true);
      document.addEventListener('keydown', e => { rec.key = {trusted: e.isTrusted, key: e.key, chart: e.composedPath().indexOf(svg) >= 0,
        element: e.composedPath().indexOf(c) >= 0}; }, true);
      return true; })()""")
    g = b.js(GEOM)
    block = json.loads(g["block"])
    duration = float(block["duration"])
    times = [row[0] for row in block["rows"]]
    chapters = [ch["t"] for ch in block["chapters"]]
    gap = max(later - earlier for earlier, later in zip(times, times[1:]))
    scale = g["w"] / g["vbw"]  # CSS px per chart unit: one, to within the pixel a kept pre-render may differ by
    bx, bw = g["bar"]
    y_track, y_plot = g["y"] + g["track"] * scale, g["y"] + g["vbh"] * 0.4 * scale

    def unit(cx: float) -> float:
        """A client x in the chart's own units, as the element reads it."""
        return (cx - g["x"]) / scale

    def at_x(t: float) -> float:
        """The client x of a time, through the track the chart drew."""
        return g["x"] + (bx + min(1.0, max(0.0, t / duration)) * bw) * scale

    def lands(got: float | None, want: float) -> bool:
        """Whether a time read off the page is the one expected. The readouts
        are written to the hundredth of a second, so that is the tolerance;
        a reading that never arrived is never the expected time."""
        return got is not None and abs(got - want) <= 0.011

    def maps_to(cx: float) -> float:
        """What the element makes of a position: the nearest sample within the
        snap radius, and the position's own time when none is near enough. A tie
        goes to the earlier sample, as the layout's own hit test does."""
        u = unit(cx)
        near = min(times, key=lambda t: abs(bx + t / duration * bw - u))
        if abs(bx + near / duration * bw - u) <= SNAP_RADIUS:
            return near
        return min(duration, max(0.0, (u - bx) / bw * duration))

    def mouse(cx: float, cy: float, kind: str = "mousePressed"):
        b.call("Input.dispatchMouseEvent", type=kind, x=cx, y=cy, button="left", clickCount=1)

    def drag_to(cx: float, cy: float):
        b.call("Input.dispatchMouseEvent", type="mouseMoved", x=cx, y=cy, button="left", buttons=1)

    def key(name: str, code: str | None = None, mods: int = 0):
        """One keydown/keyup. mods is the CDP bitmask: Alt 1, Control 2, Meta 4, Shift 8."""
        p = {"key": name, **({"code": code} if code else {}), **({"modifiers": mods} if mods else {})}
        b.call("Input.dispatchKeyEvent", type="keyDown", **p)
        b.call("Input.dispatchKeyEvent", type="keyUp", **p)

    # E5 tap: a press and a release at one position, with no travel between them
    e5 = Edge(("Following", "Seek", "Following"), "Tap: a press on the track seeks the film",
              "A press and a release at one position on the track, with nothing in between: the chart reads the position, "
              "snaps it to the sample within its snap radius, and asks the film for that time. The press is a CDP mouse event, "
              "so the page sees a trusted pointerdown; a scripted click carries an untrusted one, and the check below says which arrived.")
    x_tap = at_x(times[5]) + 0.4 * SNAP_RADIUS * scale
    t_tap = maps_to(x_tap)
    tap0 = b.js(GEST)
    mouse(x_tap, y_track)
    tap_down = b.js(GEST)
    mouse(x_tap, y_track, "mouseReleased")
    b.wait(0.15)
    tap1 = b.js(GEST)
    rec = b.js("window.__opgest")
    down, up = rec.get("down") or {}, rec.get("up") or {}
    landed = seconds(tap1["film_t"])
    e5.checks += [Check("the press arrives as a trusted pointer event on the chart",
                        bool(down.get("trusted") and down.get("chart") and up.get("trusted") and up.get("chart")),
                        f"pointerdown trusted={down.get('trusted')} pointerType={down.get('kind')} on the chart={down.get('chart')}, "
                        f"pointerup trusted={up.get('trusted')}"),
                  Check("the chart takes the focus from the press itself", tap_down["focused"], f"focused={tap_down['focused']} while the button is down"),
                  Check("the press alone does not seek", tap_down["film_t"] == tap0["film_t"], f"film still at {tap_down['film_t']}"),
                  Check("the release moves the film's clock", tap1["film_t"] != tap0["film_t"], f"{tap0['film_t']} to {tap1['film_t']}"),
                  Check("the tap lands on the sample its position snaps to", lands(landed, t_tap),
                        f"film at {tap1['film_t']}, expected {t_tap:.2f}s from a press {unit(x_tap) - unit(at_x(t_tap)):.0f} px past that sample "
                        f"(snap radius {SNAP_RADIUS:.0f} px, one sample is {gap:.2f}s)"),
                  Check("the chart's readout followed the film", tap1["readout"] == tap1["film_t"], f"chart {tap1['readout']}, film {tap1['film_t']}")]
    rep.edges.append(e5)

    # E6 drag: the preview aims while the clock stays, and the release commits
    e6 = Edge(("Following", "Peek", "Following"), "Drag: the preview aims, the release commits",
              "A press, five moves across the plot and a release. Past the slop a tap is allowed the press becomes a pending seek: "
              "the host carries the pending state, the preview names the time under the pointer, and the film's clock does not move, "
              "because a preview is not a seek. The release is what commits, and it commits where the pointer left it.")
    x_from, x_to = at_x(times[1]), at_x(times[6]) + 0.35 * SNAP_RADIUS * scale
    steps = [x_from + (x_to - x_from) * i / 5 for i in range(1, 6)]
    t_to = maps_to(x_to)
    drag0 = b.js(GEST)
    film = b.film_start(rect)
    t0 = b.now()
    mouse(x_from, y_plot)
    rows = sample(b, GEST, 0.18, t0=t0, film=film, rect=rect)
    held = len(rows)  # the rows before the first move: the press on its own
    aims = []
    for cx in steps:
        drag_to(cx, y_plot)
        rows += sample(b, GEST, 0.12, t0=t0, film=film, rect=rect)
        aims.append((cx, rows[-1][1]))
    mouse(x_to, y_plot, "mouseReleased")
    t_rel = round(b.now() - t0, 3)
    rows += sample(b, GEST, 0.35, t0=t0, film=film, rect=rect)
    ts = [t for t, _ in rows]
    make_film(e6, film, d, "drag", keys=8, ylabel="% of the film's duration", chapter0="aiming", marks=[(t_rel, "release")],
              title=f"the preview crosses {len(steps)} positions before the release commits",
              trace=[(0.0, "Following", "Peek", "Following"), (t_rel, "Following", "Seek", "Following")],
              series=[{"label": "the time the preview names", "series": SERIES["preview"], "t": ts,
                       "y": [(seconds(s["readout"]) or 0.0) / duration * 100 for _, s in rows], "lw": 2.4, "at": 0.35},
                      {"label": "the film's clock", "series": SERIES["thumb"], "t": ts,
                       "y": [(seconds(s["film_t"]) or 0.0) / duration * 100 for _, s in rows], "lw": 2.6, "at": 0.8}])
    off = max(abs(unit(at_x(seconds(s["readout"]) or 0.0)) - unit(cx)) for cx, s in aims)
    wrong = [f"{cx:.0f} px wanted {maps_to(cx):.2f}s got {s['readout']}" for cx, s in aims if not lands(seconds(s["readout"]), maps_to(cx))]
    still = sorted({s["film_t"] for _, s in aims} | {s["film_t"] for _, s in rows[:held]})
    last = rows[-1][1]
    e6.checks += [Check("the press on its own aims nothing", not any(s["pending"] or s["peeking"] for _, s in rows[:held]),
                        f"{held} samples with the button down and no travel, none of them pending"),
                  Check("a drag past the slop aims a pending seek", all(s["pending"] and s["peeking"] for _, s in aims),
                        f"{sum(1 for _, s in aims if s['pending'])} of {len(aims)} positions pending, "
                        f"the first {unit(steps[0]) - unit(x_from):.0f} px along and the slop is {DRAG_PX:.0f} px"),
                  Check("the preview names the time under the pointer", not wrong,
                        f"{len(aims)} positions, each within {off:.1f} px of the time it names" if not wrong else "; ".join(wrong)),
                  Check("the film's clock does not move while the drag aims", still == [drag0["film_t"]],
                        f"film at {', '.join(still)} throughout, playhead at {drag0['x']} px"),
                  Check("the release commits where the pointer was", lands(seconds(last["film_t"]), t_to),
                        f"film at {last['film_t']}, expected {t_to:.2f}s (one sample is {gap:.2f}s)"),
                  Check("the pending state and the preview clear at the release", not last["pending"] and not last["peeking"] and last["rule"] is None,
                        f"pending={last['pending']}, peeking={last['peeking']}, preview rule {last['rule']}")]
    rep.edges.append(e6)

    # E7 Escape during a drag: the aim is dropped and the release commits nothing
    e7 = Edge(("Following", "Cancel", "Following"), "Escape during a drag: the seek is dropped",
              "A press, a move onto another sample, then Escape while the button is still down. The press takes the focus itself, "
              "which is what lets Escape reach the very drag it started: the aim goes, and the release that follows commits nothing, "
              "so the film's clock ends where it stood before the press.")
    xa, xb = at_x(times[2]), at_x(times[5])
    esc0 = b.js(GEST)
    mouse(xa, y_plot)
    drag_to(xb, y_plot)
    b.wait(0.1)
    esc1 = b.js(GEST)
    key("Escape", "Escape")
    esc2 = b.js(GEST)
    mouse(xb, y_plot, "mouseReleased")
    b.wait(0.15)
    esc3 = b.js(GEST)
    struck = b.js("window.__opgest.key") or {}
    e7.checks += [Check("the drag was aiming at another sample before Escape",
                        esc1["pending"] and lands(seconds(esc1["readout"]), maps_to(xb)),
                        f"pending={esc1['pending']}, previewing {esc1['readout']} while the film reads {esc1['film_t']}"),
                  Check("Escape arrives as a trusted key event on the chart",
                        bool(struck.get("trusted") and struck.get("chart") and struck.get("key") == "Escape"),
                        f"keydown {struck.get('key')} trusted={struck.get('trusted')} on the chart={struck.get('chart')}"),
                  Check("Escape clears the pending seek and its preview",
                        not esc2["pending"] and not esc2["peeking"] and esc2["readout"] == esc0["film_t"],
                        f"pending={esc2['pending']}, peeking={esc2['peeking']}, readout back to {esc2['readout']}"),
                  Check("the release after Escape commits nothing", esc3["film_t"] == esc0["film_t"] and esc3["x"] == esc0["x"],
                        f"film at {esc3['film_t']}, as before the press; playhead at {esc3['x']} px")]
    rep.edges.append(e7)

    # E8 hover: the preview a fine hovering pointer gets, and no clock moved
    e8 = Edge(("Following", "Peek", "Following"), "Hover: the pointer previews a time without asking for it",
              "The pointer crosses the plot with no button down. A hover is not a gesture a finger can make, so the element gates this "
              "preview on a fine pointer, and the browser is launched with the Blink settings that give the page one: the first check "
              "below is that precondition, and it fails rather than weakens if the browser answers otherwise.")
    x_hover = at_x(times[3]) + 0.5 * SNAP_RADIUS * scale
    t_hover = maps_to(x_hover)
    hov0 = b.js(GEST)
    b.hover(x_hover, y_plot)
    b.wait(0.12)
    hov1 = b.js(GEST)
    b.hover(4, 4)
    b.wait(0.12)
    hov2 = b.js(GEST)
    shown = seconds(hov1["readout"])
    e8.checks += [Check("the browser reports a fine hovering pointer", bool(g["fine"] and g["hover"]),
                        f"pointer: fine {g['fine']}, coarse {g['coarse']}, none {g['no_pointer']}; hover: hover {g['hover']}; "
                        f"navigator.maxTouchPoints {g['touches']}"),
                  Check("the pointer over the plot peeks", hov1["peeking"] and not hov1["pending"],
                        f"peeking={hov1['peeking']}, pending={hov1['pending']}, previewing {hov1['readout']}"),
                  Check("the peek names the sample the pointer is nearest", lands(shown, t_hover),
                        f"previewing {hov1['readout']}, expected {t_hover:.2f}s from a pointer {unit(x_hover) - unit(at_x(t_hover)):.0f} px past "
                        f"that sample (snap radius {SNAP_RADIUS:.0f} px, one sample is {gap:.2f}s)"),
                  Check("the peek leaves the film's clock alone", hov1["film_t"] == hov0["film_t"] and hov1["x"] == hov0["x"],
                        f"film at {hov1['film_t']}, playhead at {hov1['x']} px, as before the pointer arrived"),
                  Check("the state and the preview clear when the pointer leaves",
                        not hov2["peeking"] and hov2["readout"] == hov2["film_t"] and hov2["rule"] is None,
                        f"peeking={hov2['peeking']}, readout back to {hov2['readout']}, preview rule {hov2['rule']}")]
    rep.edges.append(e8)

    # E9 keys: the chart's own table, every landing read from the page's data block
    e9 = Edge(("Following", "Seek", "Following"), "Keys: every rule of the table, against the page's own data",
              "The chart is focused by a press on it, then driven by CDP key events. Every expected landing below is computed from the "
              "block the page carries, never from a number typed into this tool: the samples a comma and an arrow step between, the "
              "chapter starts a page key walks, and the duration the digits and End are fractions of.")

    def index_at(t: float) -> int:
        """The sample at or before t, the first when t is before all of them."""
        return max([j for j, s in enumerate(times) if s <= t + 1e-9] or [0])

    def stepped(t: float, n: int) -> float:
        return times[min(len(times) - 1, max(0, index_at(t) + n))]

    def chapter_next(t: float) -> float:
        return next((ch for ch in chapters if ch > t + 1e-9), duration)

    mouse(at_x(0.0), y_track)
    mouse(at_x(0.0), y_track, "mouseReleased")
    b.wait(0.12)
    start = b.js(GEST)
    plan = [("End", "End", "End", 0, "the end of the announced timeline", lambda t: duration),
            ("ArrowLeft", "ArrowLeft", "ArrowLeft", 0, "five samples back", lambda t: stepped(t, -5)),
            (",", ",", "Comma", 0, "one sample back", lambda t: stepped(t, -1)),
            ("Shift and ArrowRight", "ArrowRight", "ArrowRight", 8, "one second on", lambda t: min(duration, t + 1.0)),
            ("Control and ArrowRight", "ArrowRight", "ArrowRight", 2, "the next chapter, as Page Down does", chapter_next),
            ("PageDown", "PageDown", "PageDown", 0, "the next chapter, the announced maximum when there is none", chapter_next),
            ("7", "7", "Digit7", 0, "seven tenths of the axis", lambda t: duration * 0.7),
            ("l", "l", "KeyL", 0, "ten samples on, held at the last", lambda t: stepped(t, 10)),
            ("Home", "Home", "Home", 0, "the start", lambda t: 0.0)]
    at = seconds(start["film_t"]) or 0.0
    e9.checks.append(Check("the press on the chart focused it", start["focused"], f"focused={start['focused']}, film at {start['film_t']}"))
    for label, name, code, mods, says, want in plan:
        target = want(at)
        key(name, code, mods)
        b.wait(0.08)
        st = b.js(GEST)
        got = seconds(st["film_t"])
        e9.checks.append(Check(f"{label} seeks {says}", lands(got, target),
                               f"film {secs(at)} to {st['film_t']}, expected {target:.2f}s from the page's data; the chart's readout {st['readout']}"))
        at = target
    rep.edges.append(e9)

    # E10 Space: the one key that asks the film to run rather than to move
    e10 = Edge(("Following", "Toggle", "Following"), "Space: the chart asks the film to play, and to stop again",
               "Space is the one key in the table that carries no time: the chart emits opt-chart-toggle and the film plays or pauses. "
               "The clock that answers is the film's, so what Space did is read from the film's playing state and from its readout moving.")
    sp0 = b.js(GEST)
    key(" ", "Space")
    b.wait(0.3)
    sp1 = b.js(GEST)
    key(" ", "Space")
    sp2 = b.js(GEST)
    b.wait(0.2)
    sp3 = b.js(GEST)
    ran = (seconds(sp1["film_t"]) or 0.0) - (seconds(sp0["film_t"]) or 0.0)
    e10.checks += [Check("Space starts the film", sp1["playing"] and not sp0["playing"], f"playing {sp0['playing']} then {sp1['playing']}"),
                   Check("the film's clock runs while it plays", ran > 0.1, f"{sp0['film_t']} to {sp1['film_t']}"),
                   Check("Space again stops it", sp2["playing"] is False, f"playing {sp2['playing']}"),
                   Check("the clock stops with it", sp3["film_t"] == sp2["film_t"], f"still at {sp3['film_t']}"),
                   Check("the chart followed the clock both ways", sp3["readout"] == sp3["film_t"], f"chart {sp3['readout']}, film {sp3['film_t']}")]
    rep.edges.append(e10)

    # E11 focus: the table's keys are the chart's own only while the chart has the focus
    e11 = Edge(("Following", "Seek", "Following"), "Focus: the keys act on the chart and nowhere else",
               "The chart answers its key table only while the chart itself has the focus. The focus goes to the data table's "
               "disclosure, which is inside the chart's own shadow root: a key struck there still reaches the element's own listener, "
               "so nothing but the focus rule can refuse it, and each check below says the key arrived before it says nothing moved. "
               "An arrow and Space there move no clock, and Space opens the table the disclosure belongs to instead. The same two keys "
               "are then struck again with the chart focused, and both act.")
    e11.note = ("The focus goes to the disclosure rather than to something outside the chart because a key struck outside would not "
                "reach the element at all, and the rule would go untested. The film's play button, the nearest thing on the page, "
                "answers an arrow by stepping five of its own frames and Space by playing: that is the film's key table, and it would "
                "move the very clock these checks watch.")
    SUMMARY = f"{C}.shadowRoot.querySelector('details.data summary')"
    OPEN = f"{C}.shadowRoot.querySelector('details.data').open"
    blur0 = b.js(GEST)
    b.js(f"{SUMMARY}.focus(); 'ok'")
    key("ArrowRight", "ArrowRight")
    b.wait(0.12)
    blur1, arrow = b.js(GEST), b.js("window.__opgest.key") or {}
    key(" ", "Space")
    b.wait(0.2)
    blur2, space = b.js(GEST), b.js("window.__opgest.key") or {}
    opened = b.js(OPEN)
    key(" ", "Space")  # the disclosure closes again, so the page is left as this edge found it
    b.wait(0.12)
    # the chart takes the focus back the way E9 gives it: a press on the chart itself
    x_back = at_x(times[0])
    t_back = maps_to(x_back)
    want = stepped(t_back, 5)
    mouse(x_back, y_track)
    mouse(x_back, y_track, "mouseReleased")
    b.wait(0.15)
    back = b.js(GEST)
    key("ArrowRight", "ArrowRight")
    b.wait(0.12)
    keyed = b.js(GEST)
    key(" ", "Space")
    b.wait(0.25)
    played = b.js(GEST)
    key(" ", "Space")  # stopped again, so the coarse press below starts from a still clock
    b.wait(0.15)
    landed = seconds(keyed["film_t"])
    e11.checks += [Check("an arrow struck inside the chart, with the focus off it, seeks nothing",
                         bool(arrow.get("element")) and not blur1["focused"] and blur1["film_t"] == blur0["film_t"] and blur1["readout"] == blur0["readout"],
                         f"keydown {arrow.get('key')} trusted={arrow.get('trusted')} reached the element={arrow.get('element')}, "
                         f"the chart focused={blur1['focused']}; film still at {blur1['film_t']}, the chart's readout still {blur1['readout']}"),
                   Check("Space there plays nothing, and opens the data table instead",
                         bool(space.get("element")) and blur2["playing"] == blur0["playing"] and blur2["film_t"] == blur0["film_t"] and bool(opened),
                         f"keydown {space.get('key')!r} reached the element={space.get('element')}; film still at {blur2['film_t']}, "
                         f"playing={blur2['playing']}, and the disclosure the key belongs to is open={opened}"),
                   Check("the same arrow seeks once the chart has the focus back",
                         back["focused"] and lands(landed, want),
                         f"focused={back['focused']} after the press; film {back['film_t']} to {keyed['film_t']}, expected {want:.2f}s, "
                         f"five samples on from {t_back:.2f}s in the page's data"),
                   Check("the same Space plays once the chart has the focus back", played["playing"] and not keyed["playing"],
                         f"playing {keyed['playing']} then {played['playing']}, film {keyed['film_t']} to {played['film_t']}")]
    rep.edges.append(e11)

    # E12 the coarse path: no hover to preview with, so a press held long enough aims
    e12 = Edge(("Following", "Seek", "Following"), "The coarse path: a press held long enough aims, and the release commits",
               "With touch emulation on, the browser answers (pointer: coarse) and offers five touch points, and the input is a real "
               "touch: CDP touch events, which the page receives as pointer events of type touch. A finger that does not move sends no "
               "events at all, so the element arms a wake-up instead and the press becomes a pending seek once it has been held.")
    x_touch = at_x(times[4]) - 0.4 * SNAP_RADIUS * scale
    t_touch = maps_to(x_touch)
    b.call("Emulation.setTouchEmulationEnabled", enabled=True, maxTouchPoints=5)
    try:
        b.frame()  # an emulation change reaches the page in the next frame
        media = b.js("[matchMedia('(pointer: coarse)').matches, matchMedia('(pointer: fine)').matches, navigator.maxTouchPoints]")
        touch0 = b.js(GEST)
        b.call("Input.dispatchTouchEvent", type="touchStart", touchPoints=[{"x": x_touch, "y": y_track, "id": 1}])
        touch1 = b.js(GEST)
        b.wait(LONG_PRESS + 0.2)
        touch2 = b.js(GEST)
        b.call("Input.dispatchTouchEvent", type="touchEnd", touchPoints=[])
        b.wait(0.2)
        touch3 = b.js(GEST)
        finger = b.js("window.__opgest.down") or {}
    finally:
        b.call("Emulation.setTouchEmulationEnabled", enabled=False)
        b.frame()
        restored = b.js("[matchMedia('(pointer: coarse)').matches, navigator.maxTouchPoints]")
    previewed, committed = seconds(touch2["readout"]), seconds(touch3["film_t"])
    e12.checks += [Check("touch emulation makes the pointer coarse", media == [True, False, 5],
                         f"pointer: coarse {media[0]}, fine {media[1]}, navigator.maxTouchPoints {media[2]}"),
                   Check("the touch arrives as a trusted pointer event of type touch",
                         bool(finger.get("trusted") and finger.get("kind") == "touch" and finger.get("chart")),
                         f"pointerdown trusted={finger.get('trusted')} pointerType={finger.get('kind')} on the chart={finger.get('chart')}"),
                   Check("a press not yet held long enough aims nothing", not touch1["pending"],
                         f"pending={touch1['pending']} one frame after the finger went down"),
                   Check("held past the delay it aims a seek", touch2["pending"] and touch2["peeking"],
                         f"pending={touch2['pending']}, peeking={touch2['peeking']} once held past the {LONG_PRESS:.1f}s delay, on the page's own clock"),
                   Check("the held press previews the sample under the finger", lands(previewed, t_touch),
                         f"previewing {touch2['readout']}, expected {t_touch:.2f}s from a finger {unit(at_x(t_touch)) - unit(x_touch):.0f} px short of that sample"),
                   Check("the clock waits for the release", touch2["film_t"] == touch0["film_t"], f"film still at {touch2['film_t']}"),
                   Check("the release commits the seek", lands(committed, t_touch) and not touch3["pending"],
                         f"film at {touch3['film_t']}, expected {t_touch:.2f}s, pending={touch3['pending']}"),
                   Check("touch emulation is off again", restored == [False, 0],
                         f"pointer: coarse {restored[0]}, navigator.maxTouchPoints {restored[1]}")]
    rep.edges.append(e12)

    # E13 two fingers: a second point down before the first came up, which is the
    # one gesture a mouse cannot make. The rule the checks below hold the element
    # to is its own, read off crates/op-site/src/components/chart.rs: on_down ends
    # any live press before it takes the new one, and on_move and on_up ignore a
    # pointer whose id is not the live press's (should_ignore). So the gesture
    # belongs to whichever finger went down last, and the other one's release
    # means nothing rather than committing at a position its own press never saw.
    e13 = Edge(("Following", "Seek", "Following"), "Two fingers: the press belongs to the one that went down last",
               "A second finger arriving on a chart already being pressed is the gesture a mouse cannot make, and the element's rule "
               "for it is that the later press takes the gesture over: the press it replaces is ended rather than left lying, its "
               "capture released and its aim dropped, and a release carried by any pointer but the live press's is ignored. So the "
               "seek that commits is the last press's own, and never one finger's origin read at the other finger's position. The "
               "same two places are then pressed in the opposite order, and the commit follows the order the fingers arrived in "
               "rather than where on the track they landed.")
    e13.note = ("Both points are held through the protocol's own bookkeeping: Input.dispatchTouchEvent carries the set of active "
                "points and Chrome dispatches the one that changed, so the touchStart that adds the second finger names the first "
                "one again and leaves it pressed, and each touchEnd names the point that lifts. The page's own touch log is what "
                "says the two were down together, and the first check of each round reads it: a dispatch that had quietly replaced "
                "the point set rather than added to it would fail there, rather than pass everything after it against one finger.")
    # the page's own witness for this edge: every touch event with the points it
    # left active and the one it changed, and every pointer event with the id it
    # carried, its isTrusted and whether the chart was on its composed path.
    # __opgest above keeps only the last event of each kind, and two pointers at
    # once is the whole question here. The checks below are mostly negative - that
    # a finger's lift moved nothing - and a negative check with no witness passes
    # just as well when the input never reached the page at all, so each of them
    # reads the event out of this log before it says what did not happen.
    b.js("(() => { const log = window.__optouch = [];\n"
         "      const svg = " + SHOWN % f"{C}.shadowRoot" + ";\n"
         """      for (const t of ['touchstart', 'touchend', 'touchcancel'])
        document.addEventListener(t, e => log.push({e: e.type, active: [...e.touches].map(p => p.identifier),
          changed: [...e.changedTouches].map(p => p.identifier)}), true);
      for (const t of ['pointerdown', 'pointerup', 'pointercancel'])
        document.addEventListener(t, e => log.push({e: e.type, id: e.pointerId, kind: e.pointerType,
          trusted: e.isTrusted, chart: e.composedPath().indexOf(svg) >= 0}), true);
      return true; })()""")

    def touch(kind: str, points: list):
        """One touch event carrying these (client x, touch id) points on the track:
        for a start, every point down once it has arrived; for an end, the points that lift."""
        b.call("Input.dispatchTouchEvent", type=kind, touchPoints=[{"x": cx, "y": y_track, "id": i} for cx, i in points])

    def logged() -> list:
        """Everything the page has seen since the last read, taken off the log."""
        return b.js("window.__optouch.splice(0)")

    def captures(ids: list) -> list:
        """Which of these pointer ids the svg still holds a capture for."""
        return b.js(f"""(() => {{ const svg = {SHOWN % f'{C}.shadowRoot'};
          return {ids}.map(id => svg.hasPointerCapture(id)); }})()""")

    def two_fingers(x_first: float, x_second: float) -> dict:
        """One finger down and held past the long-press delay, a second down beside
        it while the first is still pressed and held in its turn, then the first
        lifted and the second lifted. What a step did is read at the frame its event
        was delivered in, which is the smallest unit of page time there is."""
        m = {}
        logged()
        m["before"] = b.js(GEST)
        touch("touchStart", [(x_first, 1)])
        b.wait(LONG_PRESS + 0.2)
        m["held"] = b.js(GEST)
        touch("touchStart", [(x_first, 1), (x_second, 2)])
        m["taken"] = b.js(GEST)
        m["log"] = logged()
        m["ids"] = [e["id"] for e in m["log"] if e["e"] == "pointerdown"]
        m["caps"] = captures(m["ids"])
        b.wait(LONG_PRESS + 0.2)
        m["aimed"] = b.js(GEST)
        touch("touchEnd", [(x_first, 1)])
        m["lifted"] = b.js(GEST)
        m["lift_log"] = logged()
        touch("touchEnd", [(x_second, 2)])
        b.wait(0.2)
        m["done"] = b.js(GEST)
        m["left"] = captures(m["ids"])
        return m

    # far enough apart that neither finger's sample is within the other's snap
    # radius, and each a little off its own sample so that the time a check
    # expects is the sample the element snapped to and not the position itself
    x_early, x_late = at_x(times[1]) + 0.35 * SNAP_RADIUS * scale, at_x(times[6]) - 0.35 * SNAP_RADIUS * scale
    b.call("Emulation.setTouchEmulationEnabled", enabled=True, maxTouchPoints=5)
    try:
        b.frame()  # an emulation change reaches the page in the next frame
        media = b.js("[matchMedia('(pointer: coarse)').matches, matchMedia('(pointer: fine)').matches, navigator.maxTouchPoints]")
        e13.checks.append(Check("touch emulation offers the five points this edge needs two of", media == [True, False, 5],
                                f"pointer: coarse {media[0]}, fine {media[1]}, navigator.maxTouchPoints {media[2]}"))
        for x_first, x_second in ((x_early, x_late), (x_late, x_early)):
            t_first, t_second = maps_to(x_first), maps_to(x_second)
            order = f"{t_first:.2f}s first, {t_second:.2f}s second"
            m = two_fingers(x_first, x_second)
            clock = m["before"]["film_t"]
            held, taken, aimed, lifted, done = (m[k] for k in ("held", "taken", "aimed", "lifted", "done"))
            downs = [e for e in m["log"] if e["e"] == "pointerdown"]
            adds = next((e for e in m["log"] if e["e"] == "touchstart" and e["changed"] == [2]), {})
            # the two checks that say a finger changed nothing read the event that
            # carried it out of the log first: without that, a dispatch the page
            # never saw would pass them both
            arrived = downs[-1] if downs else {}
            lift = next((e for e in m["lift_log"] if e["e"] == "pointerup" and e["id"] == (m["ids"] or [None])[0]), {})
            committed = seconds(done["film_t"])
            e13.checks += [
                Check(f"{order}: both fingers are down at once",
                      sorted(adds.get("active") or []) == [1, 2] and len(downs) == 2 and downs[0]["id"] != downs[1]["id"]
                      and all(e["kind"] == "touch" for e in downs),
                      f"the touchstart that added the second point left {adds.get('active')} active and changed {adds.get('changed')}; "
                      f"two pointerdowns of type {sorted({e['kind'] for e in downs})}, ids {[e['id'] for e in downs]}"),
                Check(f"{order}: the first finger, held past the delay, aims at its own sample",
                      held["pending"] and held["peeking"] and lands(seconds(held["readout"]), t_first)
                      and held["film_t"] == clock,
                      f"pending={held['pending']}, peeking={held['peeking']}, previewing {held['readout']} for a finger "
                      f"{unit(x_first) - unit(at_x(t_first)):.0f} px from the {t_first:.2f}s sample; film still at {held['film_t']}"),
                Check(f"{order}: the second finger going down reaches the chart, ends that aim and seeks nothing",
                      bool(arrived.get("trusted") and arrived.get("chart")) and arrived.get("kind") == "touch"
                      and not taken["pending"] and not taken["peeking"] and taken["rule"] is None and taken["film_t"] == clock,
                      f"pointerdown id {arrived.get('id')} trusted={arrived.get('trusted')} of type {arrived.get('kind')} "
                      f"on the chart={arrived.get('chart')}; one frame later: pending={taken['pending']}, peeking={taken['peeking']}, "
                      f"preview rule {taken['rule']}, the readout back to the clock's own {taken['readout']}, film still at {taken['film_t']}"),
                Check(f"{order}: the capture goes with the press", m["caps"] == [False, True],
                      f"the svg holds a pointer capture for {m['ids']}: {m['caps']}, so the first finger's was released as its press ended"),
                Check(f"{order}: the second finger, held in its turn, aims at its own sample",
                      aimed["pending"] and aimed["peeking"] and lands(seconds(aimed["readout"]), t_second)
                      and aimed["film_t"] == clock,
                      f"pending={aimed['pending']}, previewing {aimed['readout']}, expected {t_second:.2f}s from a finger "
                      f"{unit(x_second) - unit(at_x(t_second)):.0f} px from that sample; film still at {aimed['film_t']}"),
                Check(f"{order}: the first finger lifting reaches the chart, commits nothing and disturbs nothing",
                      bool(lift.get("trusted") and lift.get("chart")) and lift.get("kind") == "touch"
                      and lifted["film_t"] == clock and lifted["x"] == m["before"]["x"] and lifted["pending"]
                      and lands(seconds(lifted["readout"]), t_second),
                      f"pointerup id {lift.get('id')} trusted={lift.get('trusted')} of type {lift.get('kind')} "
                      f"on the chart={lift.get('chart')}; film still at {lifted['film_t']} and the playhead still at "
                      f"{lifted['x']} px; the second finger goes on aiming at {lifted['readout']}, pending={lifted['pending']}"),
                Check(f"{order}: the release that commits is the last press's own",
                      lands(committed, t_second),
                      f"film {clock} to {done['film_t']}, expected the {t_second:.2f}s under the second finger and not the "
                      f"{t_first:.2f}s under the first, {abs(t_second - t_first):.2f}s away (one sample is {gap:.2f}s)"),
                Check(f"{order}: nothing is left set behind the gesture",
                      not done["pending"] and not done["peeking"] and done["rule"] is None and m["left"] == [False, False]
                      and done["readout"] == done["film_t"],
                      f"pending={done['pending']}, peeking={done['peeking']}, preview rule {done['rule']}, captures {m['left']} for "
                      f"{m['ids']}; the chart's readout {done['readout']} on the film's {done['film_t']}")]
    finally:
        b.call("Emulation.setTouchEmulationEnabled", enabled=False)
        b.frame()
        restored = b.js("[matchMedia('(pointer: coarse)').matches, navigator.maxTouchPoints]")
    e13.checks.append(Check("touch emulation is off again", restored == [False, 0],
                            f"pointer: coarse {restored[0]}, navigator.maxTouchPoints {restored[1]}"))
    rep.edges.append(e13)

    # E14 the snap-back, the other way a drag is cancelled and the one nothing
    # here drove: the cancelling state is published on the docs page and appeared
    # in no check, so a release inside the band could have committed a seek and
    # this report would have said nothing. Decision 19 marks the snap-back as an
    # experiment, which is exactly the kind of thing a report is for. The band is
    # the element's own SNAPBACK_PX, wider than the slop a tap is allowed, so no
    # position sits on both thresholds at once; it is measured in the chart's
    # units, hence the scale on the way back.
    e14 = Edge(("Following", "Cancel", "Following"), "The snap-back: a drag brought home cancels on release",
               "A press aims a seek, then comes back within a few pixels of where it went down. The chart says so at once - the "
               "cancelling state, with the preview still showing - because nothing has been decided yet: aiming out again resumes "
               "the pending seek, and that release commits where the pointer left it, so one drag across its own start is one "
               "gesture and not two. The second round lets go inside the band instead, which is the cancel, and the film's clock "
               "ends exactly where it stood before the press.")
    x_origin, x_out = at_x(times[2]), at_x(times[6])
    x_in = x_origin + 0.5 * SNAPBACK_PX * scale  # half the band from the origin, and under the slop as well
    t_out = maps_to(x_out)
    snap0 = b.js(GEST)
    mouse(x_origin, y_plot)
    drag_to(x_out, y_plot)
    b.wait(0.12)
    snap_out = b.js(GEST)
    drag_to(x_in, y_plot)
    b.wait(0.12)
    snap_home = b.js(GEST)
    drag_to(x_out, y_plot)
    b.wait(0.12)
    snap_again = b.js(GEST)
    mouse(x_out, y_plot, "mouseReleased")
    b.wait(0.15)
    # that release committed, so it is also where the clock stands before the
    # second round presses
    stood = b.js(GEST)
    # the second round: the same drag, let go inside the band
    mouse(x_origin, y_plot)
    drag_to(x_out, y_plot)
    b.wait(0.12)
    drop_out = b.js(GEST)
    drag_to(x_in, y_plot)
    b.wait(0.12)
    drop_home = b.js(GEST)
    mouse(x_in, y_plot, "mouseReleased")
    b.wait(0.15)
    dropped = b.js(GEST)
    let_go = (b.js("window.__opgest") or {}).get("up") or {}
    e14.checks += [Check("the drag out aims a pending seek at the sample it reached",
                         snap_out["pending"] and not snap_out["cancelling"] and snap_out["peeking"]
                         and lands(seconds(snap_out["readout"]), t_out),
                         f"pending={snap_out['pending']}, cancelling={snap_out['cancelling']}, previewing "
                         f"{snap_out['readout']}, expected {t_out:.2f}s {unit(x_out) - unit(x_origin):.0f} px from the press"),
                   Check("brought back inside the band it says it will cancel, and goes on previewing",
                         snap_home["cancelling"] and snap_home["pending"] and snap_home["peeking"]
                         and snap_home["rule"] is not None and snap_home["film_t"] == snap0["film_t"],
                         f"cancelling={snap_home['cancelling']}, pending={snap_home['pending']}, the preview rule still drawn at "
                         f"{snap_home['rule']} and naming {snap_home['readout']}, from {unit(x_in) - unit(x_origin):.1f} px off the "
                         f"origin with a band of {SNAPBACK_PX:.0f} px; film still at {snap_home['film_t']}"),
                   Check("aiming out again resumes the pending seek and drops the warning",
                         snap_again["pending"] and not snap_again["cancelling"] and snap_again["peeking"]
                         and lands(seconds(snap_again["readout"]), t_out),
                         f"cancelling={snap_again['cancelling']}, pending={snap_again['pending']}, previewing "
                         f"{snap_again['readout']} again"),
                   Check("the release out there commits, so the return was not itself the cancel",
                         lands(seconds(stood["film_t"]), t_out)
                         and not stood["pending"] and not stood["cancelling"],
                         f"film {snap0['film_t']} to {stood['film_t']}, expected {t_out:.2f}s (one sample is {gap:.2f}s)"),
                   Check("a second drag brought home the same way warns the same way",
                         drop_out["pending"] and drop_home["cancelling"] and drop_home["peeking"],
                         f"out: pending={drop_out['pending']}; home: cancelling={drop_home['cancelling']}, "
                         f"peeking={drop_home['peeking']}, previewing {drop_home['readout']}"),
                   Check("the release inside the band arrives on the chart and commits nothing",
                         bool(let_go.get("trusted") and let_go.get("chart"))
                         and dropped["film_t"] == stood["film_t"] and dropped["x"] == stood["x"],
                         f"pointerup trusted={let_go.get('trusted')} on the chart={let_go.get('chart')}; film at "
                         f"{dropped['film_t']}, as before the press, and the playhead still at {dropped['x']} px, "
                         f"after aiming at {drop_out['readout']} on the way out"),
                   Check("both states and the preview clear with that release",
                         not dropped["pending"] and not dropped["cancelling"] and not dropped["peeking"]
                         and dropped["rule"] is None and dropped["readout"] == dropped["film_t"],
                         f"pending={dropped['pending']}, cancelling={dropped['cancelling']}, peeking={dropped['peeking']}, "
                         f"preview rule {dropped['rule']}, the chart's readout {dropped['readout']} back on the film's "
                         f"{dropped['film_t']}")]
    rep.edges.append(e14)
    b.hover(2, 2)
    return rep


# ----------------------------------------------------------------------------
# running the controls: one at a time, or one worker process each
# ----------------------------------------------------------------------------
# Longest first, so the slowest controls are not left to start last. The kind is
# the whole cost signal, which saves naming tags: on this machine the toggle's
# eight edges take about a minute, the chart three quarters of that, the switch
# a third, and an attention control six to ten seconds (the film is about ninety
# and has a phase to itself). Ties go to the tag so the order is stable. This is
# a scheduling hint and never a correctness requirement: the reports are sorted
# back into contract order before anything is rendered.
KIND_COST = {"toggle": 0, "chart": 1, "switch": 2}


def longest_first(ctrl: dict) -> tuple:
    return (KIND_COST.get(ctrl["kind"], 3), ctrl["tag"])


def failed_control(ctrl: dict, exc: BaseException) -> ControlReport:
    """A crashed run is a failed control, not a crashed report."""
    rep = ControlReport(tag=ctrl["tag"], kind=ctrl["kind"], page=ctrl["page"], nodes=["Idle"])
    e = Edge(("Idle", "Attend", "Idle"), "Run failed", "")
    e.checks.append(Check("run completes", False, repr(exc)[:300]))
    rep.edges.append(e)
    return rep


def run_kind(b: Browser, base: str, ctrl: dict, out: Path, machine: list) -> ControlReport:
    """One control, driven by the runner its kind names."""
    if ctrl["kind"] == "toggle":
        return run_toggle(b, base, ctrl, out, machine)
    if ctrl["kind"] == "switch":
        return run_switch(b, base, ctrl, out)
    if ctrl["kind"] == "film":
        return run_film(b, base, ctrl, out)
    if ctrl["kind"] == "chart":
        return run_chart(b, base, ctrl, out)
    return run_attention(b, base, ctrl, out)


def run_control(job: tuple) -> tuple:
    """One control in a worker process: its own browser on its own port and its
    own profile, driving its own page against the server the parent started, then
    its ad hoc page. Returns the finished report and whatever the run printed, so
    the parent can put a control's lines out as one block rather than interleave
    six workers'. Only plain data crosses back: no socket, no Browser, no image."""
    ctrl, base, out, clock, binary, machine, work = job
    global CLOCK, RECORDER
    CLOCK = clock
    said = io.StringIO()
    b = None
    try:
        with contextlib.redirect_stdout(said):
            # the port is picked and bound here in the child, so two workers
            # cannot be handed the same one; the parallel path is synthetic
            # only, which is why there is no clip calibration to do
            b = Browser(binary, work, synthetic=(clock == CLOCK_SYNTHETIC))
            RECORDER = b
            b.goto(base + "/", "document.readyState === 'complete'")
            rep = run_kind(b, base, ctrl, out, machine)
    # a shell that never starts and a page that never becomes ready both call
    # sys.exit; in a worker that is this control failing, not the run
    except (Exception, SystemExit) as exc:
        rep = failed_control(ctrl, exc)
    finally:
        if b is not None:
            b.close()
    rep.clock = clock
    render_control(rep, out)
    return rep, said.getvalue()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dist", help="serve this directory")
    ap.add_argument("--base", help="or use a running server")
    ap.add_argument("--out", default="reports/interactions")
    ap.add_argument("--contract", default="data/interaction-contract.json")
    ap.add_argument("--machine", help="machine table JSON (default: cargo run machine_table)")
    ap.add_argument("--chrome")
    ap.add_argument("--clock", choices=(CLOCK_SYNTHETIC, CLOCK_REAL),
                    help="page timing: a synthetic frame clock (chrome-headless-shell, deterministic) or real time "
                         "(default: synthetic when a shell is found)")
    ap.add_argument("--shell", help="the chrome-headless-shell the synthetic clock drives")
    ap.add_argument("--checks-json", help="write every control's checks (name, outcome, detail) here as JSON")
    ap.add_argument("--only", help="comma-separated tags")
    ap.add_argument("--jobs", type=int, help="how many controls to run at once, each in its own worker process and browser "
                                             "(default: up to six on the synthetic clock, where the measurements are virtual "
                                             "time; one on the real clock, which measures the machine itself)")
    ap.add_argument("--publish", action="store_true", help="copy the integrated revision into <dist>/reports/interactions/ (deploys with the site)")
    args = ap.parse_args()

    started = time.time()
    shell = find_shell(args.shell)
    if args.clock == CLOCK_SYNTHETIC and not shell:
        sys.exit("--clock synthetic needs a chrome-headless-shell: pass --shell PATH or set OP_HEADLESS_SHELL")
    clock = args.clock or (CLOCK_SYNTHETIC if shell else CLOCK_REAL)
    if args.clock is None and not shell:
        print("clock: no chrome-headless-shell found (--shell, OP_HEADLESS_SHELL, ~/.cache/puppeteer, ./ or /tmp), "
              "so the pages run on the real clock")
    binary = shell if clock == CLOCK_SYNTHETIC else find_chrome(args.chrome)
    global CLOCK
    CLOCK = clock
    print(f"{clock_note(clock)}; {binary} ({binary_version(binary)})", flush=True)
    if clock == CLOCK_REAL:
        if args.jobs is not None and args.jobs > 1:
            print("--jobs ignored on the real clock: it measures the machine this runs on, and a second browser under "
                  "load would move the timings it reports; the controls run one at a time")
        jobs = 1
    else:
        jobs = max(1, args.jobs if args.jobs is not None else min(6, os.cpu_count() or 1))

    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    server = None
    if args.dist:
        port = free_port()
        # RangeHTTPServer answers Range requests, which browsers need before they
        # treat a film's recording as seekable (python's own http.server does not)
        server = subprocess.Popen([sys.executable, "-m", "RangeHTTPServer", str(port)], cwd=args.dist,
                                  stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        base = f"http://127.0.0.1:{port}"
        time.sleep(0.8)
    else:
        base = args.base.rstrip("/")
    machine = json.load(open(args.machine)) if args.machine else json.loads(
        subprocess.check_output(["cargo", "run", "-q", "-p", "op-webc", "--bin", "machine_table"], text=True))
    contract = json.load(open(args.contract))["controls"]
    only = set(args.only.split(",")) if args.only else None
    selected = [c for c in contract if not only or c["tag"] in only]
    statics = [c["tag"] for c in selected if c["kind"] == "static"]
    # phase 1: every control except the film, which needs the integrated pages to exist
    phase1 = [c for c in selected if c["kind"] not in ("static", "film")]
    jobs = max(1, min(jobs, len(phase1) or 1))  # no more workers than there are controls
    print(f"controls: {len(phase1)} in phase 1, {jobs} at a time" +
          ("; on the synthetic clock a measurement is virtual time, so workers cannot move each other's results"
           if jobs > 1 else ""), flush=True)

    adhoc = out / "adhoc"
    integrated = out / "integrated"
    adhoc.mkdir(parents=True, exist_ok=True)
    work = out / ".work"
    b = Browser(binary, work, synthetic=(clock == CLOCK_SYNTHETIC))
    global RECORDER
    RECORDER = b
    reports, failed = [], False
    prefix = "/reports/interactions/"

    def tally(rep) -> list:
        """A control's outcome: the failures it found, then how it stands."""
        nonlocal failed
        lines = []
        for c in rep.checks:
            if not c.ok:
                failed = True
                lines.append(f"   FAIL {c.name}: {c.detail}")
        lines.append(f"   {sum(1 for c in rep.checks if c.ok)}/{len(rep.checks)} checks pass")
        return lines

    def run_one(ctrl):
        print(f"== {ctrl['tag']} ({ctrl['kind']})", flush=True)
        try:
            rep = run_kind(b, base, ctrl, adhoc, machine)
        except Exception as exc:  # a crashed run is a failed control, not a crashed report
            rep = failed_control(ctrl, exc)
        rep.clock = clock
        render_control(rep, adhoc)
        reports.append(rep)
        print("\n".join(tally(rep)), flush=True)

    def run_together(controls):
        """The controls at once, longest first, one worker process each. A worker
        prints nothing itself: its lines come back with its report and go out as
        one block, so a control's header, its failures and its count stay together.
        The reports are sorted back into contract order here, so neither the pages
        nor the checks dump depends on which worker finished first."""
        place = {c["tag"]: i for i, c in enumerate(controls)}
        done = []
        # spawn, not fork: a worker starts from the module rather than from a copy
        # of this process, which holds a CDP socket and the thread that reads it
        with ProcessPoolExecutor(max_workers=jobs, mp_context=multiprocessing.get_context("spawn")) as pool:
            pending = {pool.submit(run_control, (c, base, adhoc, clock, binary, machine, work / c["tag"])): c
                       for c in sorted(controls, key=longest_first)}
            for future in as_completed(pending):
                ctrl = pending[future]
                try:
                    rep, said = future.result()
                except Exception as exc:  # a worker that died is still one failed control
                    rep, said = failed_control(ctrl, exc), ""
                    rep.clock = clock
                    render_control(rep, adhoc)
                done.append((place[ctrl["tag"]], rep))
                block = [f"== {ctrl['tag']} ({ctrl['kind']})"] + ([said.rstrip("\n")] if said.strip() else []) + tally(rep)
                print("\n".join(block), flush=True)
        reports.extend(rep for _, rep in sorted(done, key=lambda pair: pair[0]))

    try:
        b.goto(base + "/", "document.readyState === 'complete'")
        if clock == CLOCK_REAL:
            b.calibrate_clip()
        if jobs > 1:
            run_together(phase1)
        else:
            for ctrl in phase1:
                run_one(ctrl)
        render_index(reports, statics, adhoc)
        # the integrated revision, from the site's own elements
        render_integrated(reports, statics, adhoc, integrated, base, prefix)
        if args.publish and args.dist:
            target = Path(args.dist) / "reports" / "interactions"
            if target.exists():
                shutil.rmtree(target)
            shutil.copytree(integrated, target)
            # phase 2: the film, exercised on an integrated page it renders (one level of recursion)
            for ctrl in selected:
                if ctrl["kind"] == "film":
                    run_one(ctrl)
            render_index(reports, statics, adhoc)
            render_integrated(reports, statics, adhoc, integrated, base, prefix)
            shutil.rmtree(target)
            shutil.copytree(integrated, target)
        elif any(c["kind"] == "film" for c in selected):
            print("   (film kind skipped: needs --dist with --publish so the integrated pages are served)")
    finally:
        b.close()
        if server:
            server.kill()
        shutil.rmtree(work, ignore_errors=True)
    if args.checks_json:
        Path(args.checks_json).write_text(json.dumps(
            [{"tag": r.tag, "kind": r.kind, "clock": r.clock,
              "checks": [{"name": c.name, "ok": c.ok, "detail": c.detail} for c in r.checks]} for r in reports],
            indent=1) + "\n")
        print(f"checks:            {args.checks_json}")
    print(f"ad hoc report:     {adhoc / 'index.html'}")
    print(f"integrated report: {integrated / 'index.html'}" + (f"  (published to {Path(args.dist) / 'reports' / 'interactions'})" if args.publish and args.dist else ""))
    print(f"wall clock:        {time.time() - started:.1f}s over {len(reports)} controls, {jobs} at a time")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
