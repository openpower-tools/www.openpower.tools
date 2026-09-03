# /// script
# requires-python = ">=3.11"
# dependencies = ["websocket-client>=1.8", "pillow>=10"]
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

    uv run tools/interaction_report/report.py --dist dist --out reports/interactions
"""
from __future__ import annotations

import argparse
import base64
import json
import math
import os
import shutil
import socket
import subprocess
import sys
import time
import urllib.request
from dataclasses import dataclass, field
from pathlib import Path

import websocket

STORAGE_KEY = "tools.openpower.sites.www.storage.version.1.configuration.version.1.ux.theme.current"
OKABE = {"ghost": "#0072B2", "palette": "#E69F00", "thumb": "#009E73", "preview": "#CC79A7",
         "ideal": "#000000", "mark": "#D55E00", "muted": "#888888"}


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


class Browser:
    def __init__(self, chrome: str, workdir: Path):
        self.port = free_port()
        self.profile = workdir / "profile"
        self.profile.mkdir(parents=True, exist_ok=True)
        log = (workdir / "chrome.log").open("wb")
        self.proc = subprocess.Popen(
            [chrome, "--headless=new", "--disable-gpu", "--no-sandbox", "--hide-scrollbars",
             "--disable-dev-shm-usage", "--no-first-run", "--no-default-browser-check",
             "--window-size=1280,900", f"--remote-debugging-port={self.port}", "--remote-allow-origins=*",
             f"--user-data-dir={self.profile}", "about:blank"],
            stdout=log, stderr=log)
        # CI runners can take well over ten seconds to bring Chrome up.
        deadline = time.time() + 90
        while True:
            try:
                urllib.request.urlopen(f"http://127.0.0.1:{self.port}/json/version", timeout=2).read()
                break
            except Exception:
                if self.proc.poll() is not None:
                    sys.exit(f"Chrome exited early (code {self.proc.returncode}); see {workdir / 'chrome.log'}")
                if time.time() > deadline:
                    sys.exit(f"Chrome did not open its DevTools port within 90s; see {workdir / 'chrome.log'}")
                time.sleep(0.3)
        target = json.load(urllib.request.urlopen(urllib.request.Request(
            f"http://127.0.0.1:{self.port}/json/new?url=about:blank", method="PUT")))
        self.ws = websocket.create_connection(target["webSocketDebuggerUrl"], timeout=60)
        self.mid = 0
        self.events: list[dict] = []
        self.call("Page.enable")
        self.call("Runtime.enable")
        self.clip_uses_page_coords = None

    def close(self):
        try:
            self.ws.close()
        finally:
            self.proc.kill()

    def call(self, method: str, **params):
        self.mid += 1
        self.ws.send(json.dumps({"id": self.mid, "method": method, "params": params}))
        while True:
            msg = json.loads(self.ws.recv())
            if msg.get("id") == self.mid:
                if "error" in msg:
                    raise RuntimeError(f"{method}: {msg['error']}")
                return msg.get("result", {})
            self.events.append(msg)

    def js(self, expr: str):
        r = self.call("Runtime.evaluate", expression=expr, returnByValue=True, awaitPromise=True)
        if "exceptionDetails" in r:
            ex = r["exceptionDetails"]
            raise RuntimeError(f"JS: {ex.get('exception', {}).get('description', ex.get('text'))} in {expr[:80]}")
        return r.get("result", {}).get("value")

    def seed_theme(self, mode: str | None):
        script = f"try{{localStorage.setItem({STORAGE_KEY!r},{mode!r})}}catch(e){{}}" if mode else \
                 f"try{{localStorage.removeItem({STORAGE_KEY!r})}}catch(e){{}}"
        self.call("Page.addScriptToEvaluateOnNewDocument", source=script)

    def reduced_motion(self, on: bool):
        self.call("Emulation.setEmulatedMedia",
                  features=[{"name": "prefers-reduced-motion", "value": "reduce" if on else "no-preference"}])

    def goto(self, url: str, ready: str, timeout: float = 25):
        # Mark the outgoing document so a same-URL navigation cannot pass
        # the readiness check on the old page (and install helpers there).
        try:
            self.js("window.__op_stale = true")
        except RuntimeError:
            pass
        self.call("Page.navigate", url=url)
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
        return [e["params"] for e in self.events if e.get("method") == "Runtime.exceptionThrown"]

    def calibrate_clip(self):
        """Whether captureScreenshot clips are page- or viewport-relative here."""
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
        if self.clip_uses_page_coords:
            sx, sy = self.js("[window.scrollX, window.scrollY]")
            x, y = x + sx, y + sy
        shot = self.call("Page.captureScreenshot", format="png",
                         clip={"x": max(0, x - margin), "y": max(0, y - margin),
                               "width": w + 2 * margin, "height": h + 2 * margin, "scale": scale})
        return base64.b64decode(shot["data"])

    def frame(self, path: Path, rect, margin: float = 14, scale: float = 2):
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
    film: dict | None = None  # {sheet, times, w, h, keys: [(caption, relpath)]}


@dataclass
class ControlReport:
    tag: str
    kind: str
    page: str
    edges: list = field(default_factory=list)
    machine_edges: list = field(default_factory=list)  # folded (from,input,to)
    nodes: list = field(default_factory=list)

    @property
    def checks(self):
        return [c for e in self.edges for c in e.checks]


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
        out.append(f'<polyline points="{pts}" fill="none" stroke="{sr["color"]}" stroke-width="{sr.get("lw", 1.8)}"{dash} stroke-linejoin="round"/>')
        if sr.get("label") and sr["t"]:
            i = min(len(sr["t"]) - 1, int(len(sr["t"]) * sr.get("at", 0.85)))
            out.append(f'<text x="{x_of(sr["t"][i]) + 4:.1f}" y="{y_of(sr["y"][i]) - 5:.1f}" fill="{sr["color"]}" font-weight="700" paint-order="stroke" stroke="#fff" stroke-width="4">{sr["label"]}</text>')
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
def sample(b: Browser, expr: str, seconds: float, period: float = 0.05, until=None, t0: float | None = None,
           film: list | None = None, rect=None, every: int = 1, scale: float = 2):
    """Samples `expr` for `seconds`; with `film`, also captures a clipped
    frame every `every`-th sample so frames and curves share one clock."""
    start = time.time()
    t0 = t0 if t0 is not None else start
    rows = []
    i = 0
    while time.time() - start < seconds:
        s = b.js(expr)
        t = round(time.time() - t0, 3)
        rows.append((t, s))
        if film is not None and i % every == 0:
            film.append((t, b.frame_bytes(rect, scale=scale)))
        i += 1
        if until and until(s):
            break
        time.sleep(period)
    return rows


def burst(b: Browser, rect, seconds: float, fps: float = 15, t0: float | None = None, scale: float = 1.5) -> list:
    """A short frame sequence with no sampling in between."""
    start = time.time()
    t0 = t0 if t0 is not None else start
    film = []
    while time.time() - start < seconds:
        film.append((round(time.time() - t0, 3), b.frame_bytes(rect, scale=scale)))
        time.sleep(max(0, 1 / fps - 0.02))
    return film


def changed_fraction(a: bytes, b_: bytes) -> float:
    """Fraction of pixels that differ between two PNG frames of equal size."""
    from PIL import Image, ImageChops
    import io
    ia, ib = Image.open(io.BytesIO(a)).convert("RGB"), Image.open(io.BytesIO(b_)).convert("RGB")
    if ia.size != ib.size:
        return 1.0
    diff = ImageChops.difference(ia, ib).convert("L").point(lambda v: 255 if v > 24 else 0)
    hist = diff.histogram()
    return hist[255] / (ia.size[0] * ia.size[1])


def make_film(edge: Edge, frames: list, d: Path, name: str, keys: int = 8, series=None, marks=(), ylabel="progress %", title="", chapter0="start", trace=()):
    """Stitches (t, png) frames into a horizontal sprite sheet with a
    timestamp table and, per frame, the fraction of pixels that changed
    since the previous frame - shown as the reel's captions, so a frame
    that did not change says so instead of merely looking repeated."""
    from PIL import Image
    import io
    if not frames:
        return
    images = [Image.open(io.BytesIO(png)).convert("RGB") for _, png in frames]
    w, h = images[0].size
    images = [im if im.size == (w, h) else im.resize((w, h)) for im in images]
    sheet = Image.new("RGB", (w * len(images), h), "#ffffff")
    for i, im in enumerate(images):
        sheet.paste(im, (i * w, 0))
    sheet.save(d / f"{name}-film.png", optimize=True)
    times = [t for t, _ in frames]
    deltas = [0.0] + [changed_fraction(frames[i - 1][1], frames[i][1]) for i in range(1, len(frames))]
    film = {"sheet": f"{name}-film.png", "times": times, "deltas": deltas, "w": w, "h": h, "title": title,
            "chapters": [[0.0, chapter0]] + [[float(t), label] for t, label in marks],
            "series": [{k: v for k, v in sr.items() if k in ("label", "color", "t", "y", "dash", "lw", "at")} for sr in (series or [])],
            "ylabel": ylabel,
            "trace": [[round(float(t), 3), a, i, b] for t, a, i, b in trace]}
    if series:
        t_max = max(max(times), max(max(sr["t"]) for sr in series if sr["t"]))
        film["chart"] = chart_svg(series, t_max, marks, ylabel)
    edge.film = film


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
              series=[{"label": "preview opacity", "color": OKABE["preview"], "t": ts, "y": [s["preview_op"] * 100 for _, s in rows], "lw": 2.4},
                      {"label": "preview left (% of track)", "color": OKABE["ghost"], "t": ts, "y": [s["preview"] / s["w"] * 100 for _, s in rows], "at": 0.5}])
    at = next((t for t, s in rows if s["attention"]), None)
    peak = max(s["preview_op"] for _, s in rows)
    e1.checks += [Check("attention custom state set", at is not None and at < 0.3, f"at {at}s"),
                  Check("preview reaches legible opacity", peak >= 0.8, f"peak {peak}"),
                  Check("no flight while merely attended", not any(s["flight"] for _, s in rows))]
    rep.edges.append(e1)

    # ---- E2: Idle --Activate--> Toward : flight ----
    e2 = Edge(("Idle", "Activate", "Toward"), "Activate: a hovered click starts the flight",
              "The setting flips at once (solid thumb, aria-checked, stored choice); the palette blend and the progress ghost run on the blend clock; the preview is gated off.")
    film = []
    b.click(x, y)
    t0 = time.time()
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
              series=[{"label": "ideal exponential", "color": OKABE["ideal"], "t": ideal_t, "y": ideal, "lw": 1, "dash": True, "at": 0.68},
                      {"label": "solid thumb", "color": OKABE["thumb"], "t": ts, "y": thumb, "at": 0.06},
                      {"label": "palette", "color": OKABE["palette"], "t": ts, "y": pal, "lw": 2.6, "at": 0.8},
                      {"label": "progress ghost", "color": OKABE["ghost"], "t": ts, "y": ghost, "at": 0.9},
                      {"label": "preview opacity", "color": OKABE["preview"], "t": ts, "y": [s["preview_op"] * 100 for _, s in rows], "lw": 1, "dash": True, "at": 0.3}])
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
              series=[{"label": "preview opacity (resuming)", "color": OKABE["preview"], "t": ts, "y": [s["preview_op"] * 100 for _, s in after], "lw": 2.4, "at": 0.5}])
    resume = max(s["preview_op"] for _, s in after)
    e3.checks += [Check("settled when the blend ended (2.8-3.4s)", settled is not None and 2.8 <= settled <= 3.4, f"flight cleared at {settled}s"),
                  Check("palette arrived", green(rows[-1][1]["bg"]) == g_off and abs(g_off - g_on) > 100),
                  Check("preview resumes under the resting pointer", resume >= 0.8, f"peak {resume} within 1.9s")]
    rep.edges.append(e3)

    # ---- fly back so the abort below starts from dark, as the page began ----
    b.click(x, y)
    sample(b, T, 3.6, 0.1, until=lambda s_: not s_["flight"] and s_["dark"])
    time.sleep(0.3)

    # ---- E4/E5: Toward --Activate--> Back --Finished--> Idle : abort ----
    e4 = Edge(("Toward", "Activate", "Back"), "Activate mid-flight: abort",
              "The setting returns at once and the armed clocks reverse; CSS shortens the reversal in proportion to how far it had got.")
    before = ST()
    film = []
    b.click(x, y)
    t0 = time.time()
    rows = sample(b, T, 1.2, 0.05, t0=t0, film=film, rect=rect)
    b.click(x, y)
    t_abort = time.time() - t0
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
              series=[{"label": "solid thumb", "color": OKABE["thumb"], "t": ts, "y": thumb, "at": 0.2},
                      {"label": "palette", "color": OKABE["palette"], "t": ts, "y": pal, "lw": 2.6, "at": 0.3},
                      {"label": "progress ghost", "color": OKABE["ghost"], "t": ts, "y": ghost, "at": 0.25}])
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
    b.click(x, y); t0 = time.time()
    rows = sample(b, T, 0.8, 0.05, t0=t0, film=film, rect=rect)
    b.click(x, y); t_ab = time.time() - t0; time.sleep(0.12)
    b.click(x, y); t_re = time.time() - t0
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
              series=[{"label": "palette", "color": OKABE["palette"], "t": ts, "y": pal, "lw": 2.6, "at": 0.8},
                      {"label": "progress ghost", "color": OKABE["ghost"], "t": ts, "y": ghost, "at": 0.9}])
    e6.checks += [Check("third click re-arms toward the opposite setting", s_re["flight"] and s_re["dark"] != before["dark"]),
                  Check("the new flight settles on its own clock", settled2 is not None and settled2 - t_re < 3.6, f"{settled2 - t_re:.2f}s after the third click" if settled2 else "never")]
    rep.edges.append(e6)

    # ---- E7: Neglect ----
    e7 = Edge(("Idle", "Neglect", "Idle"), "Neglect: the pointer leaves", "Attention clears; the preview stops.")
    film = [(0.0, b.frame_bytes(rect))]
    t0 = time.time()
    b.hover(2, 2)
    rows = sample(b, T, 0.8, 0.05, t0=t0, film=film, rect=rect)
    ts = [t for t, _ in rows]
    make_film(e7, film, d, "neglect", keys=4, ylabel="% (opacity)", trace=[(0.0, "Idle", "Neglect", "Idle")],
              series=[{"label": "preview opacity", "color": OKABE["preview"], "t": ts, "y": [s["preview_op"] * 100 for _, s in rows], "lw": 2.4, "at": 0.5}])
    gone = next((t for t, s in rows if not s["attention"]), None)
    e7.checks += [Check("attention custom state cleared", gone is not None and gone < 0.3, f"at {gone}s"),
                  Check("preview hidden once unattended", rows[-1][1]["preview_op"] < 0.05, f"opacity {rows[-1][1]['preview_op']}")]
    rep.edges.append(e7)

    # ---- E8: reduced motion (same machine, different representation) ----
    e8 = Edge(("Idle", "Attend", "Idle"), "Reduced motion: the same edges, static representation",
              "prefers-reduced-motion collapses the snap token and disables the preview loop; the preview appears statically at the destination, and a click still blends the palette (a colour fade is not motion) while the ghost snaps.")
    b.reduced_motion(True)
    time.sleep(0.2)
    film = [(0.0, b.frame_bytes(rect))]
    t0 = time.time()
    b.hover(x, y); time.sleep(0.4)
    s_rm = ST()
    film.append((round(time.time() - t0, 3), b.frame_bytes(rect)))
    t_click = time.time() - t0
    b.click(x, y)
    rows = sample(b, T, 3.6, 0.05, t0=t0, film=film, rect=rect)
    ts = [t for t, _ in rows]
    g_from = green(rows[0][1]["bg"]); g_to = green(rows[-1][1]["bg"])
    settle_rm = next((t for t, s_ in rows if not s_["flight"]), None)
    make_film(e8, film, d, "reduced", keys=6, marks=[(t_click, "click")], chapter0="hover",
              trace=[(0.0, "Idle", "Attend", "Idle"), (t_click, "Idle", "Activate", "Toward")] + ([(settle_rm, "Toward", "Finished", "Idle")] if settle_rm else []),
              series=[{"label": "palette (still fades)", "color": OKABE["palette"], "t": ts, "y": [(green(s["bg"]) - g_from) / (g_to - g_from or 1) * 100 for _, s in rows], "lw": 2.6, "at": 0.7},
                      {"label": "ghost (snaps)", "color": OKABE["ghost"], "t": ts, "y": [abs(s["ghost"] - rows[0][1]["ghost"]) / span * 100 for _, s in rows], "at": 0.3}])
    mid = green(rows[len(rows) // 2][1]["bg"])
    e8.checks += [Check("static preview shown while attended", s_rm["preview_anim"] == "none" and s_rm["preview_op"] >= 0.8, f"animation {s_rm['preview_anim']}, opacity {s_rm['preview_op']}"),
                  Check("ghost snaps to the destination", abs(rows[1][1]["ghost"] - rows[-1][1]["ghost"]) < 1.0),
                  Check("palette still fades", 20 < mid < 220, f"mid-flight green {mid}")]
    b.reduced_motion(False)
    b.hover(2, 2)
    rep.edges.append(e8)
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
    b.hover(2, 2); time.sleep(0.2)
    s0 = b.js(S)
    # E1 attend
    e1 = Edge(("Idle", "Attend", "Idle"), "Attend: hover plays the preview",
              "The native input's ::after is the preview ghost, generated from the same parts as the theme toggle: same contrast, same plateau, same clock.")
    film = []
    b.hover(x, y)
    rows = sample(b, S, 2.0, 0.04, film=film, rect=rect)
    ts = [t for t, _ in rows]
    make_film(e1, film, d, "attend", keys=6, ylabel="% (opacity, left)", trace=[(0.0, "Idle", "Attend", "Idle")],
              series=[{"label": "preview opacity", "color": OKABE["preview"], "t": ts, "y": [s["preview_op"] * 100 for _, s in rows], "lw": 2.4},
                      {"label": "preview left (% of track)", "color": OKABE["ghost"], "t": ts, "y": [s["preview"] / s["w"] * 100 for _, s in rows], "at": 0.5}])
    peak = max(s["preview_op"] for _, s in rows)
    e1.checks += [Check("preview animation plays", any(s["anim"].startswith("opt-switch-preview") for _, s in rows)),
                  Check("preview reaches legible opacity", peak >= 0.8, f"peak {peak}")]
    rep.edges.append(e1)
    # E2 activate
    e2 = Edge(("Idle", "Activate", "Idle"), "Activate: the thumb snaps on the snap clock", "A native checkbox toggle; the thumb transitions over --op-motion-snap.")
    film = [(0.0, b.frame_bytes(rect))]
    t0 = time.time()
    b.click(x, y)
    rows = sample(b, S, 0.5, 0.02, t0=t0, film=film, rect=rect)
    lefts = [s["thumb"] for _, s in rows]
    ts = [t for t, _ in rows]
    travel = [abs(l - lefts[0]) / max(1e-6, abs(lefts[-1] - lefts[0])) * 100 for l in lefts]
    make_film(e2, film, d, "activate", keys=6, trace=[(0.0, "Idle", "Activate", "Idle")],
              series=[{"label": "thumb travel", "color": OKABE["thumb"], "t": ts, "y": travel, "lw": 2.4, "at": 0.5}])
    moved_by = next((t for t, s in rows if abs(s["thumb"] - lefts[-1]) < 0.5), None)
    e2.checks += [Check("checked state toggled", rows[-1][1]["checked"] != s0["checked"]),
                  Check("thumb transitions (not a jump)", len({round(l) for l in lefts[:6]}) > 2, f"first positions {[round(l, 1) for l in lefts[:6]]}"),
                  Check("thumb arrives within the snap clock", moved_by is not None and moved_by <= 0.3, f"arrived at {moved_by}s")]
    rep.edges.append(e2)
    # E3 neglect
    e3 = Edge(("Idle", "Neglect", "Idle"), "Neglect", "The preview stops when the pointer leaves.")
    b.hover(2, 2); rows = sample(b, S, 0.6, 0.05)
    e3.checks += [Check("preview hidden once unattended", rows[-1][1]["preview_op"] < 0.05)]
    rep.edges.append(e3)
    # E4 reduced motion
    e4 = Edge(("Idle", "Attend", "Idle"), "Reduced motion: static preview", "The loop is off; the preview appears at the destination while attended.")
    b.reduced_motion(True); time.sleep(0.2)
    film = [(0.0, b.frame_bytes(rect))]
    t0 = time.time()
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
    b.hover(2, 2); b.js("window.__op.blur()"); time.sleep(0.2)
    base_sig = b.js("window.__op.sig()")
    rest_png = b.frame_bytes(rect, scale=1.5)
    # attend
    e1 = Edge(("Idle", "Attend", "Idle"), "Attend: hover", "A real pointer over the control must change something visible.")
    film = [(0.0, rest_png)]
    t0 = time.time()
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
    b.hover(2, 2); time.sleep(0.2)
    film = [(0.0, b.frame_bytes(rect, scale=1.5))]
    t0 = time.time()
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
    b.hover(x, y); time.sleep(0.15)
    film = [(0.0, b.frame_bytes(rect, scale=1.5))]
    t0 = time.time()
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
    b.hover(2, 2); b.js("window.__op.blur()"); time.sleep(0.4)
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
.film .stage{background-repeat:no-repeat;border:1px solid #e3e3e3;box-shadow:0 1px 4px rgba(0,0,0,.08)}
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
    total = len(rep.checks)
    passed = sum(1 for c in rep.checks if c.ok)
    parts = [f"<!doctype html><html lang='en'><head><meta charset='utf-8'><title>{rep.tag} — interaction report</title><style>{CSS}</style></head><body>",
             f"<p><a href='../index.html'>All controls</a></p><h1>&lt;{rep.tag}&gt; — interaction report</h1>",
             f"<p class='note'>Kind: <code>{rep.kind}</code>. Page: <code>{rep.page}</code>. Every frame and sample below comes from real pointer and keyboard events in headless Chromium against the built site.</p>",
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
                f"<div class='stagebox'><div class='stage'></div><div class='stagelabel'></div></div>"
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
            "<p class='note'>Generated by tools/interaction_report/report.py. The machine diagrams come from the code's own transition table; the frames and curves from CDP mouse and keyboard events with the pointer resting on the control. A failing check here fails the build.</p>",
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
             f"<p>Kind: <code>{rep.kind}</code>. Page under test: <code>{esc(rep.page)}</code>. Every frame and sample comes from real pointer and keyboard events in headless Chromium against the built site; this page is rendered with the site's own elements.</p>\n"
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
                         f"<opt-film id=\"{film_id}\" sheet=\"{f['sheet']}\" title=\"{esc(e.title)}\"><script type=\"application/json\">{json.dumps(data)}</script></opt-film>\n")
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
            f"<p>Generated by tools/interaction_report/report.py from the code's own machine table. The ad hoc revision of the same evidence, which depends on none of these elements, is kept as a build artifact so gross failures in the controls can still be read.</p>\n"
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
    film = [(0.0, b.frame_bytes(rect, scale=1))]
    t0 = time.time()
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
    e2 = Edge(("Paused", "Seek", "Paused"), "Seek by keys", "Home, then . . , -> : YouTube's model, frame by frame.")
    b.js(f"{F}.focus(); 'ok'")
    ks = []
    for key_, code in (("Home", "Home"), (".", None), (".", None), (",", None), ("ArrowRight", "ArrowRight")):
        b.call("Input.dispatchKeyEvent", type="keyDown", key=key_, **({"code": code} if code else {}))
        b.call("Input.dispatchKeyEvent", type="keyUp", key=key_, **({"code": code} if code else {}))
        ks.append(b.js(S)["k"])
    e2.checks.append(Check("keys step frames as documented", ks == [0, 1, 2, 1, 6], f"{ks}"))
    rep.edges.append(e2)
    # E3 peek
    e3 = Edge(("Paused", "Peek", "Paused"), "Peek: hover without seeking", "The pointer over the chart shows a thumbnail and exposes :state(peeking); the playhead stays.")
    cr = b.js(f"(() => {{ const r = {F}.shadowRoot.querySelector('.chart').getBoundingClientRect(); return [r.x, r.y, r.width, r.height]; }})()")
    before = b.js(S)
    b.hover(cr[0] + cr[2] * 0.6, cr[1] + cr[3] * 0.4); time.sleep(0.15)
    peek = b.js(S)
    b.hover(2, 2); time.sleep(0.1)
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
    b.call("Input.dispatchMouseEvent", type="mousePressed", x=cells[1][0], y=cells[1][1], button="left", clickCount=1); time.sleep(0.1)
    p1 = b.js(S)
    b.call("Input.dispatchMouseEvent", type="mouseMoved", x=cells[2][0], y=cells[2][1], button="left", buttons=1); time.sleep(0.1)
    p2 = b.js(S)
    b.call("Input.dispatchMouseEvent", type="mouseReleased", x=cells[2][0], y=cells[2][1], button="left", clickCount=1); time.sleep(0.1)
    p3 = b.js(S)
    b.call("Input.dispatchMouseEvent", type="mousePressed", x=cells[0][0], y=cells[0][1], button="left", clickCount=1); time.sleep(0.1)
    b.call("Input.dispatchKeyEvent", type="keyDown", key="Escape", code="Escape"); b.call("Input.dispatchKeyEvent", type="keyUp", key="Escape", code="Escape")
    b.call("Input.dispatchMouseEvent", type="mouseReleased", x=cells[0][0], y=cells[0][1], button="left", clickCount=1); time.sleep(0.1)
    p4 = b.js(S)
    e4.checks += [Check("press marks a pending frame with the custom state", p1["pending"] == 1 and p1["pend_state"], f"pending {p1['pending']}"),
                  Check("dragging moves the pending frame, not the playhead", p2["pending"] == 2 and p2["k"] == 0, f"pending {p2['pending']}, playhead {p2['k']}"),
                  Check("release seeks and clears pending", p3["k"] == 2 and p3["pending"] is None and not p3["pend_state"], f"playhead {p3['k']}"),
                  Check("Escape cancels a press", p4["k"] == 2 and p4["pending"] is None, f"playhead {p4['k']}")]
    rep.edges.append(e4)
    return rep

# ----------------------------------------------------------------------------
def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dist", help="serve this directory")
    ap.add_argument("--base", help="or use a running server")
    ap.add_argument("--out", default="reports/interactions")
    ap.add_argument("--contract", default="data/interaction-contract.json")
    ap.add_argument("--machine", help="machine table JSON (default: cargo run machine_table)")
    ap.add_argument("--chrome")
    ap.add_argument("--only", help="comma-separated tags")
    ap.add_argument("--publish", action="store_true", help="copy the integrated revision into <dist>/reports/interactions/ (deploys with the site)")
    args = ap.parse_args()

    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    server = None
    if args.dist:
        port = free_port()
        server = subprocess.Popen([sys.executable, "-m", "http.server", str(port), "--directory", args.dist],
                                  stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        base = f"http://127.0.0.1:{port}"
        time.sleep(0.8)
    else:
        base = args.base.rstrip("/")
    machine = json.load(open(args.machine)) if args.machine else json.loads(
        subprocess.check_output(["cargo", "run", "-q", "-p", "op-webc", "--bin", "machine_table"], text=True))
    contract = json.load(open(args.contract))["controls"]
    only = set(args.only.split(",")) if args.only else None

    adhoc = out / "adhoc"
    integrated = out / "integrated"
    adhoc.mkdir(parents=True, exist_ok=True)
    work = out / ".work"
    b = Browser(find_chrome(args.chrome), work)
    reports, statics, failed = [], [], False
    prefix = "/reports/interactions/"

    def run_one(ctrl):
        nonlocal failed
        print(f"== {ctrl['tag']} ({ctrl['kind']})", flush=True)
        try:
            if ctrl["kind"] == "toggle":
                rep = run_toggle(b, base, ctrl, adhoc, machine)
            elif ctrl["kind"] == "switch":
                rep = run_switch(b, base, ctrl, adhoc)
            elif ctrl["kind"] == "film":
                rep = run_film(b, base, ctrl, adhoc)
            else:
                rep = run_attention(b, base, ctrl, adhoc)
        except Exception as exc:  # a crashed run is a failed control, not a crashed report
            rep = ControlReport(tag=ctrl["tag"], kind=ctrl["kind"], page=ctrl["page"], nodes=["Idle"])
            e = Edge(("Idle", "Attend", "Idle"), "Run failed", "")
            e.checks.append(Check("run completes", False, repr(exc)[:300]))
            rep.edges.append(e)
        render_control(rep, adhoc)
        reports.append(rep)
        for c in rep.checks:
            if not c.ok:
                failed = True
                print(f"   FAIL {c.name}: {c.detail}")
        print(f"   {sum(1 for c in rep.checks if c.ok)}/{len(rep.checks)} checks pass")

    try:
        b.goto(base + "/", "document.readyState === 'complete'")
        b.calibrate_clip()
        selected = [c for c in contract if not only or c["tag"] in only]
        statics = [c["tag"] for c in selected if c["kind"] == "static"]
        # phase 1: every control except the film, which needs the integrated pages to exist
        for ctrl in selected:
            if ctrl["kind"] not in ("static", "film"):
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
    print(f"ad hoc report:     {adhoc / 'index.html'}")
    print(f"integrated report: {integrated / 'index.html'}" + (f"  (published to {Path(args.dist) / 'reports' / 'interactions'})" if args.publish and args.dist else ""))
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
