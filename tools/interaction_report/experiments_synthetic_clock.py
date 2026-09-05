# /// script
# requires-python = ">=3.11"
# dependencies = ["websocket-client>=1.8"]
# ///
"""Can the site's timing checks run on a synthetic frame clock?

Drives chrome-headless-shell with BeginFrameControl and virtual time: the
page's clock advances only when we say so, one frame per BeginFrame, so
every sample sits at an exact virtual time. Two identical runs must give
identical samples; that is the whole point.
"""
import base64
import json
import pathlib
import subprocess
import sys
import threading
import time
import urllib.request

import websocket

SHELL = sys.argv[1]
# Somewhere to put the shell's log and the frame it ends on. Under the
# project, because /tmp is one reboot from gone and this script opened a
# path there without creating it, so a cleaned /tmp made it fail on its
# first line rather than on anything to do with the experiment.
WORK = pathlib.Path(__file__).resolve().parents[2] / ".omc" / "scratch" / "synthetic-clock"
WORK.mkdir(parents=True, exist_ok=True)
BASE = "http://127.0.0.1:8952"
DPR = 1.5
INTERVAL = 1000.0 / 60.0


class Shell:
    def __init__(self):
        self.port = 9333
        self.proc = subprocess.Popen(
            [SHELL, "--headless", "--disable-gpu", "--no-sandbox", "--hide-scrollbars",
             "--enable-begin-frame-control", "--deterministic-mode", "--run-all-compositor-stages-before-draw",
             "--disable-new-content-rendering-timeout", "--disable-threaded-animation", "--disable-threaded-scrolling",
             "--disable-checker-imaging", "--disable-image-animation-resync",
             "--window-size=1280,900", f"--force-device-scale-factor={DPR}",
             f"--remote-debugging-port={self.port}", "--remote-allow-origins=*", "about:blank"],
            stdout=subprocess.DEVNULL, stderr=open(WORK / "shell.log", "ab"))
        deadline = time.time() + 30
        while True:
            try:
                targets = json.load(urllib.request.urlopen(f"http://127.0.0.1:{self.port}/json/list", timeout=2))
                page = next(t for t in targets if t["type"] == "page")
                break
            except Exception:
                if time.time() > deadline:
                    sys.exit("shell did not open its port")
                time.sleep(0.2)
        self.ws = websocket.create_connection(page["webSocketDebuggerUrl"], timeout=60)
        self.mid = 0
        self.lock = threading.Lock()
        self.cond = threading.Condition(self.lock)
        self.replies = {}
        self.events = []
        self.closing = False
        threading.Thread(target=self._read, daemon=True).start()
        self.call("Page.enable")
        self.call("Runtime.enable")
        self.call("HeadlessExperimental.enable")

    def _read(self):
        while not self.closing:
            try:
                raw = self.ws.recv()
            except Exception:
                if self.closing:
                    return
                continue
            if not raw:
                continue
            msg = json.loads(raw)
            with self.cond:
                if "id" in msg:
                    self.replies[msg["id"]] = msg
                else:
                    self.events.append(msg)
                self.cond.notify_all()

    def call(self, method, **params):
        with self.cond:
            self.mid += 1
            mid = self.mid
            self.ws.send(json.dumps({"id": mid, "method": method, "params": params}))
            deadline = time.time() + 60
            while mid not in self.replies:
                if time.time() > deadline:
                    raise RuntimeError(f"{method}: no reply")
                self.cond.wait(1)
            msg = self.replies.pop(mid)
        if "error" in msg:
            raise RuntimeError(f"{method}: {msg['error']}")
        return msg.get("result", {})

    def wait_event(self, method, timeout=30):
        deadline = time.time() + timeout
        with self.cond:
            while True:
                for i, e in enumerate(self.events):
                    if e.get("method") == method:
                        del self.events[i]
                        return e
                if time.time() > deadline:
                    raise RuntimeError(f"no {method} within {timeout}s")
                self.cond.wait(1)

    def js(self, expr):
        r = self.call("Runtime.evaluate", expression=expr, returnByValue=True)
        if "exceptionDetails" in r:
            raise RuntimeError(r["exceptionDetails"].get("text"))
        return r.get("result", {}).get("value")

    def close(self):
        self.closing = True
        try:
            self.ws.close()
        finally:
            self.proc.kill()


def run(label: str, frames: int):
    s = Shell()
    try:
        # the page's clock starts paused at a fixed instant; loading gets a budget
        # that only runs while fetches are pending
        s.call("Emulation.setVirtualTimePolicy", policy="pause", initialVirtualTime=1_700_000_000)
        s.call("Page.navigate", url=BASE + "/component/chart/")
        s.call("Emulation.setVirtualTimePolicy", policy="pauseIfNetworkFetchesPending", budget=5000, maxVirtualTimeTaskStarvationCount=100000)
        s.wait_event("Emulation.virtualTimeBudgetExpired", timeout=60)
        # BeginFrame times are renderer TimeTicks (CLOCK_MONOTONIC ms); requestAnimationFrame
        # hands the page frame_time minus its time origin, so one calibration frame tells us the
        # origin and every later frame is stamped so the page sees its own virtual clock
        origin = None
        diag = []
        tick = time.monotonic() * 1000.0 + 500.0
        s.js("window.__raf = null; (function loop(t) { window.__raf = t; requestAnimationFrame(loop); })(0); 'ok'")

        def frame(screenshot=False):
            nonlocal tick, origin
            s.call("Emulation.setVirtualTimePolicy", policy="advance", budget=INTERVAL, maxVirtualTimeTaskStarvationCount=100000)
            s.wait_event("Emulation.virtualTimeBudgetExpired")
            if origin is None:
                tick += INTERVAL
            else:
                tick = max(tick + 0.001, origin + s.js("performance.now()"))
            params = {"frameTimeTicks": tick, "interval": INTERVAL}
            if screenshot:
                params["screenshot"] = {"format": "png"}
            r = s.call("HeadlessExperimental.beginFrame", **params)
            if origin is None:
                raf = s.js("window.__raf")
                if raf is not None and raf > 0:
                    origin = tick - raf
                    diag.append(("calibration", tick, raf, s.js("performance.now()")))
            elif len(diag) < 6:
                diag.append(("frame", tick, s.js("window.__raf"), s.js("performance.now()")))
            return r

        # let the wasm define its elements: pump frames until the film and chart are ready
        for _ in range(600):
            frame()
            if s.js("!!customElements.get('opt-chart') && !!document.getElementById('chart-film')?.shadowRoot?.querySelector('.stage') && document.querySelector('opt-chart').matches(':defined')"):
                break
        else:
            sys.exit("elements never defined under virtual time")
        ready_frames = "n/a"
        # the theme toggle: press it with a real input event and follow the blend token
        bg = "getComputedStyle(document.documentElement).getPropertyValue('--op-bg').trim()"
        s.js("document.querySelector('opt-theme-toggle').shadowRoot.querySelector('button').scrollIntoView({block: 'center'}); 'ok'")
        rect = s.js("(() => { const r = document.querySelector('opt-theme-toggle').shadowRoot.querySelector('button').getBoundingClientRect(); return [r.x + r.width / 2, r.y + r.height / 2]; })()")
        frame()
        before = s.js(bg)
        s.call("Input.dispatchMouseEvent", type="mousePressed", x=rect[0], y=rect[1], button="left", clickCount=1)
        s.call("Input.dispatchMouseEvent", type="mouseReleased", x=rect[0], y=rect[1], button="left", clickCount=1)
        blend = []
        for i in range(frames):
            frame()
            blend.append(s.js(bg))
        # the film: press play and follow its readout and the chart's per frame
        s.js("document.getElementById('chart-film').shadowRoot.querySelector('button.play').click(); 'ok'")
        clock = []
        for i in range(frames):
            frame()
            clock.append(s.js("[document.getElementById('chart-film').shadowRoot.querySelector('.t').textContent, document.querySelector('opt-chart').shadowRoot.querySelector('.head-t').textContent, document.querySelector('opt-chart').shadowRoot.querySelector('g.playhead').getAttribute('transform')]"))
        shot = frame(screenshot=True)
        png = base64.b64decode(shot.get("screenshotData", "")) if shot.get("screenshotData") else b""
        print(label, "diag:", [(k, round(t, 3), round(r or 0, 3), round(p, 3)) for k, t, r, p in diag])
        return {"ready_frames": ready_frames, "before": before, "blend": blend, "clock": clock, "png": png}
    finally:
        s.close()


t0 = time.time()
a = run("a", 200)
ta = time.time() - t0
t0 = time.time()
b = run("b", 200)
tb = time.time() - t0
same_blend = a["blend"] == b["blend"]
same_clock = a["clock"] == b["clock"]
same_png = a["png"] == b["png"] and len(a["png"]) > 0
changes = sum(1 for x, y in zip(a["blend"], a["blend"][1:]) if x != y)
settled_at = next((i for i in range(len(a["blend"]) - 1, 0, -1) if a["blend"][i] != a["blend"][i - 1]), None)
print(f"ready after {a['ready_frames']} frames; run a {ta:.1f}s, run b {tb:.1f}s for 2 x 200 frames")
print(f"blend: before {a['before']}; {changes} changes over 200 frames; last change at frame {settled_at} ({(settled_at or 0) * INTERVAL / 1000:.3f}s); identical across runs: {same_blend}")
print("blend samples 0, 30, 60, 120, 180:", [a["blend"][i] for i in (0, 30, 60, 120, 180)])
print(f"film clock identical across runs: {same_clock}; samples 0, 1, 2, 60, 199:", [a["clock"][i] for i in (0, 1, 2, 60, 199)])
print(f"final screenshot identical across runs: {same_png} ({len(a['png'])} bytes)")
(WORK / "final.png").write_bytes(a["png"])

def first_diff(x, y):
    for i, (p, q) in enumerate(zip(x, y)):
        if p != q:
            return i, p, q
    return None

print("first blend difference:", first_diff(a["blend"], b["blend"]))
print("first clock difference:", first_diff(a["clock"], b["clock"]))
print("blend run b samples 0, 30, 60, 120, 180:", [b["blend"][i] for i in (0, 30, 60, 120, 180)])
