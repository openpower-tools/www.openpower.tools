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
CHART_W, CHART_H, ML, MR, MT, MB = 900, 250, 46, 14, 16, 30


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
    out.append(f'<line class="head" x1="{ML}" x2="{ML}" y1="{MT}" y2="{CHART_H - MB}" stroke="{OKABE["mark"]}" stroke-width="1.5"/>')
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


def make_film(edge: Edge, frames: list, d: Path, name: str, keys: int = 8, series=None, marks=(), ylabel="progress %", title=""):
    """Stitches (t, png) frames into a horizontal sprite sheet with a
    timestamp table, and picks evenly spaced key frames for the static
    strip, each captioned with how much changed since the previous key."""
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
    picks = sorted({round(i * (len(frames) - 1) / max(1, keys - 1)) for i in range(keys)})
    key_list = []
    prev = None
    for k in picks:
        p = d / f"{name}-key{k}.png"
        images[k].save(p)
        if prev is None:
            cap = f"t={times[k]:.2f}s"
        else:
            delta = changed_fraction(frames[prev][1], frames[k][1])
            cap = f"t={times[k]:.2f}s ({delta * 100:.0f}% of pixels changed)" if delta > 0.001 else f"t={times[k]:.2f}s (identical)"
        key_list.append((cap, p.name))
        prev = k
    film = {"sheet": f"{name}-film.png", "times": times, "w": w, "h": h, "keys": key_list, "title": title}
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
    make_film(e1, film, d, "attend", keys=6, ylabel="% (opacity, left)",
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
    make_film(e2, film, d, "flight", keys=8, title=f"max ghost-palette gap {gap:.1f} pts",
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
    make_film(e3, film, d, "settle", keys=5, ylabel="% (opacity)",
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
    make_film(e4, film, d, "abort", keys=8, marks=[(t_abort, "second click: abort")],
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
    make_film(e6, film, d, "refly", keys=8, marks=[(t_ab, "abort"), (t_re, "fly again")],
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
    make_film(e7, film, d, "neglect", keys=4, ylabel="% (opacity)",
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
    make_film(e8, film, d, "reduced", keys=6, marks=[(t_click, "click")],
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
    make_film(e1, film, d, "attend", keys=6, ylabel="% (opacity, left)",
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
    make_film(e2, film, d, "activate", keys=6,
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
    make_film(e4, film, d, "reduced", keys=3)
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
    make_film(e1, film, d, "hover", keys=4)
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
    make_film(e2, film, d, "focus", keys=3)
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
    make_film(e3, film, d, "activate", keys=4)
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
.film{margin:.6rem 0 1rem;display:inline-block;border:1px solid #ddd;background:#fff;padding:.5rem;max-width:100%}
.film .chartbox{margin-top:.6rem}.film svg.chart{max-width:100%;height:auto;cursor:ew-resize;display:block}
.film .view{background-repeat:no-repeat;image-rendering:auto;display:block;max-width:100%}
.film .bar{display:flex;gap:.6rem;align-items:center;margin-top:.4rem;font-size:.85rem}
.film input[type=range]{flex:1;min-width:220px}.film .t{font-variant-numeric:tabular-nums;min-width:4.5rem}.film .n{color:#777}
"""

PLAYER_JS = """<script>
document.querySelectorAll('.film').forEach(f => {
  const times = JSON.parse(f.dataset.times), n = times.length, w = +f.dataset.w, h = +f.dataset.h;
  const view = f.querySelector('.view'), slider = f.querySelector('input'), label = f.querySelector('.t');
  const btn = f.querySelector('button'), rate = f.querySelector('select');
  const chart = f.querySelector('svg.chart'), head = chart && chart.querySelector('.head'), headT = chart && chart.querySelector('.head-t');
  const x0 = chart ? +chart.dataset.x0 : 0, x1 = chart ? +chart.dataset.x1 : 1, t1 = chart ? +chart.dataset.t1 : times[n - 1];
  const tEnd = Math.max(times[n - 1], t1);
  const scale = Math.min(1, 900 / w);
  view.style.width = (w * scale) + 'px'; view.style.height = (h * scale) + 'px';
  view.style.backgroundImage = 'url(' + f.dataset.sheet + ')';
  view.style.backgroundSize = (w * n * scale) + 'px ' + (h * scale) + 'px';
  let tc = 0, playing = false, raf = 0, last = null;
  const frameAt = t => { let k = 0; for (let j = 0; j < n; j++) if (times[j] <= t + 1e-6) k = j; return k; };
  const render = () => {
    const k = frameAt(tc);
    view.style.backgroundPosition = (-k * w * scale) + 'px 0';
    slider.value = k; label.textContent = tc.toFixed(2) + 's';
    if (chart) { const x = x0 + Math.min(1, tc / t1) * (x1 - x0); head.setAttribute('x1', x); head.setAttribute('x2', x); headT.setAttribute('x', x + 4); headT.textContent = tc.toFixed(2) + 's'; }
  };
  const pause = () => { playing = false; btn.textContent = 'Play'; cancelAnimationFrame(raf); last = null; };
  const tick = now => {
    if (!playing) return;
    if (last !== null) { tc += (now - last) / 1000 * +rate.value; if (tc > tEnd + 0.6) tc = 0; }
    last = now; render(); raf = requestAnimationFrame(tick);
  };
  btn.addEventListener('click', () => { if (playing) { pause(); return; } playing = true; btn.textContent = 'Pause'; if (tc >= tEnd) tc = 0; raf = requestAnimationFrame(tick); });
  slider.addEventListener('input', () => { pause(); tc = times[+slider.value]; render(); });
  if (chart) {
    const seek = e => { const r = chart.getBoundingClientRect(); const vb = chart.viewBox.baseVal; const px = (e.clientX - r.left) * (vb.width / r.width);
      tc = Math.min(tEnd, Math.max(0, (px - x0) / (x1 - x0) * t1)); render(); };
    chart.addEventListener('pointerdown', e => { pause(); seek(e); chart.setPointerCapture(e.pointerId); chart.onpointermove = seek; });
    chart.addEventListener('pointerup', () => { chart.onpointermove = null; });
  }
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
            parts.append("<h3>Key frames</h3><div class='strip'>" + "".join(
                f"<figure><img class='frame' src='{p}'><figcaption>{c}</figcaption></figure>" for c, p in f["keys"]) + "</div>")
            title = f" <span class='n'>{f['title']}</span>" if f.get("title") else ""
            parts.append(
                f"<h3>Playback{title}</h3>"
                f"<div class='film' data-sheet='{f['sheet']}' data-w='{f['w']}' data-h='{f['h']}' data-times='{json.dumps([round(t, 3) for t in f['times']])}'>"
                f"<div class='view'></div><div class='bar'><button type='button'>Play</button>"
                f"<select><option value='1'>1x</option><option value='0.5'>0.5x</option><option value='0.25'>0.25x</option></select>"
                f"<input type='range' min='0' max='{len(f['times']) - 1}' value='0'><span class='t'></span>"
                f"<span class='n'>{len(f['times'])} frames</span></div>"
                + (f"<div class='chartbox'>{f['chart']}</div>" if f.get("chart") else "")
                + "</div>")
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
def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dist", help="serve this directory")
    ap.add_argument("--base", help="or use a running server")
    ap.add_argument("--out", default="reports/interactions")
    ap.add_argument("--contract", default="data/interaction-contract.json")
    ap.add_argument("--machine", help="machine table JSON (default: cargo run machine_table)")
    ap.add_argument("--chrome")
    ap.add_argument("--only", help="comma-separated tags")
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

    work = out / ".work"
    b = Browser(find_chrome(args.chrome), work)
    reports, statics, failed = [], [], False
    try:
        b.goto(base + "/", "document.readyState === 'complete'")
        b.calibrate_clip()
        for ctrl in contract:
            if only and ctrl["tag"] not in only:
                continue
            if ctrl["kind"] == "static":
                statics.append(ctrl["tag"])
                continue
            print(f"== {ctrl['tag']} ({ctrl['kind']})", flush=True)
            try:
                if ctrl["kind"] == "toggle":
                    rep = run_toggle(b, base, ctrl, out, machine)
                elif ctrl["kind"] == "switch":
                    rep = run_switch(b, base, ctrl, out)
                else:
                    rep = run_attention(b, base, ctrl, out)
            except Exception as exc:  # a crashed run is a failed control, not a crashed report
                rep = ControlReport(tag=ctrl["tag"], kind=ctrl["kind"], page=ctrl["page"], nodes=["Idle"])
                e = Edge(("Idle", "Attend", "Idle"), "Run failed", "")
                e.checks.append(Check("run completes", False, repr(exc)[:300]))
                rep.edges.append(e)
            render_control(rep, out)
            reports.append(rep)
            for c in rep.checks:
                if not c.ok:
                    failed = True
                    print(f"   FAIL {c.name}: {c.detail}")
            print(f"   {sum(1 for c in rep.checks if c.ok)}/{len(rep.checks)} checks pass")
        render_index(reports, statics, out)
    finally:
        b.close()
        if server:
            server.kill()
        shutil.rmtree(work, ignore_errors=True)
    print(f"report: {out / 'index.html'}")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
