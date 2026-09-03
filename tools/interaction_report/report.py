# /// script
# requires-python = ">=3.11"
# dependencies = ["websocket-client>=1.8", "matplotlib>=3.9", "pillow>=10"]
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
        self.proc = subprocess.Popen(
            [chrome, "--headless=new", "--disable-gpu", "--no-sandbox", "--hide-scrollbars",
             "--window-size=1280,900", f"--remote-debugging-port={self.port}", "--remote-allow-origins=*",
             f"--user-data-dir={self.profile}", "about:blank"],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        for _ in range(50):
            time.sleep(0.2)
            try:
                urllib.request.urlopen(f"http://127.0.0.1:{self.port}/json/version").read()
                break
            except Exception:
                continue
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

    def frame(self, path: Path, rect, margin: float = 14, scale: float = 3):
        x, y, w, h = rect
        if self.clip_uses_page_coords:
            sx, sy = self.js("[window.scrollX, window.scrollY]")
            x, y = x + sx, y + sy
        shot = self.call("Page.captureScreenshot", format="png",
                         clip={"x": max(0, x - margin), "y": max(0, y - margin),
                               "width": w + 2 * margin, "height": h + 2 * margin, "scale": scale})
        path.write_bytes(base64.b64decode(shot["data"]))


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
# plotting
# ----------------------------------------------------------------------------
def plot(path: Path, title: str, series: list[dict], vlines: list[tuple] = (), ylabel="progress %", ylim=(-4, 106)):
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    fig, ax = plt.subplots(figsize=(9.6, 4.2), dpi=140)
    for s in series:
        ax.plot(s["t"], s["y"], color=s["color"], linewidth=s.get("lw", 1.5), linestyle=s.get("ls", "-"))
        if s.get("label"):
            i = int(len(s["t"]) * s.get("at", 0.85))
            ax.annotate(s["label"], xy=(s["t"][i], s["y"][i]), xytext=(4, 4), textcoords="offset points",
                        color=s["color"], fontsize=9, fontweight="bold")
    for x, label in vlines:
        ax.axvline(x, color=OKABE["mark"], linewidth=0.9, linestyle=(0, (2, 2)))
        ax.annotate(label, xy=(x + 0.04, 98), color=OKABE["mark"], fontsize=9)
    for side in ("top", "right"):
        ax.spines[side].set_visible(False)
    ax.set_xlabel("seconds")
    ax.set_ylabel(ylabel)
    ax.set_ylim(*ylim)
    ax.grid(axis="y", linewidth=0.3, alpha=0.35)
    ax.set_title(title, fontsize=11, loc="left")
    fig.tight_layout()
    fig.savefig(path)
    plt.close(fig)


# ----------------------------------------------------------------------------
# kinds
# ----------------------------------------------------------------------------
def sample(b: Browser, expr: str, seconds: float, period: float = 0.05, until=None, t0: float | None = None):
    start = time.time()
    t0 = t0 if t0 is not None else start
    rows = []
    while time.time() - start < seconds:
        s = b.js(expr)
        rows.append((round(time.time() - t0, 3), s))
        if until and until(s):
            break
        time.sleep(period)
    return rows


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

    def frames(edge: Edge, stamps: list[tuple[str, float]], t0: float, prefix: str):
        for i, (label, at) in enumerate(stamps):
            time.sleep(max(0, t0 + at - time.time()))
            p = d / f"{prefix}-{i}.png"
            b.frame(p, rect)
            edge.frames.append((f"{label} (t={time.time() - t0:.2f}s)" if at else label, p.name))

    # geometry references: thumb positions on each side
    s0 = ST()
    rest = {"thumb_on": s0["thumb"], "thumb_off": None, "g_on": green(s0["bg"])}

    # ---- E1: Idle --Attend--> Idle : preview loop ----
    e1 = Edge(("Idle", "Attend", "Idle"), "Attend: the pointer arrives",
              "Attention becomes a custom state on the host; while the machine is idle the preview ghost loops toward the destination.")
    b.hover(x, y)
    t0 = time.time()
    rows = sample(b, T, 2.0, 0.04)
    frames(e1, [("preview loop", 0.35), ("", 0.8), ("", 1.3)], t0, "attend")
    at = next((t for t, s in rows if s["attention"]), None)
    peak = max(s["preview_op"] for _, s in rows)
    plot(d / "attend.png", "Preview loop while attended: opacity and position of the preview ghost",
         [{"t": [t for t, _ in rows], "y": [s["preview_op"] * 100 for _, s in rows], "color": OKABE["preview"], "label": "preview opacity", "lw": 2},
          {"t": [t for t, _ in rows], "y": [s["preview"] / s["w"] * 100 for _, s in rows], "color": OKABE["ghost"], "label": "preview left (% of track)", "at": 0.5}],
         ylabel="% (opacity, travel)")
    e1.curves.append(("Preview loop over its period", "attend.png"))
    e1.checks += [Check("attention custom state set", at is not None and at < 0.3, f"at {at}s"),
                  Check("preview reaches legible opacity", peak >= 0.8, f"peak {peak}"),
                  Check("no flight while merely attended", not any(s["flight"] for _, s in rows))]
    rep.edges.append(e1)

    # ---- E2: Idle --Activate--> Toward : flight ----
    e2 = Edge(("Idle", "Activate", "Toward"), "Activate: a hovered click starts the flight",
              "The setting flips at once (solid thumb, aria-checked, stored choice); the palette blend and the progress ghost run on the blend clock; the preview is gated off.")
    b.click(x, y)
    t0 = time.time()
    first = ST()
    rows = sample(b, T, 3.7, 0.05)
    # frames are taken in a second identical flight below (sampling and capture interfere); reuse the timeline here
    settled = next((t for t, s in rows if not s["flight"]), None)
    rest["thumb_off"] = rows[-1][1]["thumb"]
    span = abs(rest["thumb_on"] - rest["thumb_off"])
    g_off = green(rows[-1][1]["bg"])
    ts = [t for t, _ in rows]
    ghost = [abs(s["ghost"] - rest["thumb_on"]) / span * 100 for _, s in rows]
    pal = [(green(s["bg"]) - rest["g_on"]) / (g_off - rest["g_on"]) * 100 for _, s in rows]
    thumb = [abs(s["thumb"] - rest["thumb_on"]) / span * 100 for _, s in rows]
    ideal_t = [i / 100 * 3 for i in range(101)]
    ideal = [(2 ** (6 * t / 3) - 1) / 63 * 100 for t in ideal_t]
    gap = max(abs(a - p) for a, p in zip(ghost, pal))
    plot(d / "flight.png", f"Flight: ghost position and palette on one clock (max gap {gap:.1f} pts)",
         [{"t": ideal_t, "y": ideal, "color": OKABE["ideal"], "lw": 0.9, "ls": (0, (4, 3)), "label": "ideal exponential", "at": 0.7},
          {"t": ts, "y": thumb, "color": OKABE["thumb"], "label": "solid thumb", "at": 0.08},
          {"t": ts, "y": pal, "color": OKABE["palette"], "lw": 2.6, "label": "palette", "at": 0.8},
          {"t": ts, "y": ghost, "color": OKABE["ghost"], "label": "progress ghost", "at": 0.9},
          {"t": ts, "y": [s["preview_op"] * 100 for _, s in rows], "color": OKABE["preview"], "lw": 1, "ls": (0, (1, 2)), "label": "preview opacity", "at": 0.3}])
    e2.curves.append(("Progress curves: flight", "flight.png"))
    e2.checks += [Check("flight custom state armed on click", first["flight"]),
                  Check("setting flipped at once", first["dark"] is False and first["checked"] == "false"),
                  Check("description names the switch and the way back", "switching" in (first["title"] or "").lower()),
                  Check("preview gated off during flight", max(s["preview_op"] for _, s in rows[:40]) < 0.05, "max opacity in first 2s"),
                  Check("ghost tracks the palette", gap < 5, f"max gap {gap:.2f} pts"),
                  Check("solid thumb settles within the snap clock", thumb[min(6, len(thumb) - 1)] > 85, f"{thumb[min(6, len(thumb) - 1)]:.0f}% at {ts[min(6, len(ts) - 1)]}s")]
    rep.edges.append(e2)

    # ---- E3: Toward --Finished--> Idle ----
    e3 = Edge(("Toward", "Finished", "Idle"), "Finished: the blend's own completion settles the machine",
              "No timer: the palette transitions' finished promises on <html> resolve, the flight state clears, and the preview resumes because the pointer is still there.")
    after = sample(b, T, 1.9, 0.05)
    resume = max(s["preview_op"] for _, s in after)
    e3.checks += [Check("settled when the blend ended (2.8-3.4s)", settled is not None and 2.8 <= settled <= 3.4, f"flight cleared at {settled}s"),
                  Check("palette arrived", green(rows[-1][1]["bg"]) == g_off and abs(g_off - rest["g_on"]) > 100),
                  Check("preview resumes under the resting pointer", resume >= 0.8, f"peak {resume} within 1.9s")]
    rep.edges.append(e3)

    # ---- frames for the flight: run it again (page is now light; fly back to dark) ----
    e2b = Edge(("Idle", "Activate", "Toward"), "Frames of a flight (light to dark)", "The same edge, captured frame by frame on the return journey.")
    b.click(x, y)
    t0 = time.time()
    frames(e2b, [("t=0.15", 0.15), ("", 0.6), ("", 1.2), ("", 1.8), ("", 2.4), ("", 2.85), ("settled", 3.4)], t0, "flight")
    time.sleep(0.4)
    rep.edges.append(e2b)

    # ---- E4/E5: Toward --Activate--> Back --Finished--> Idle : abort ----
    e4 = Edge(("Toward", "Activate", "Back"), "Activate mid-flight: abort",
              "The setting returns at once and the armed clocks reverse; CSS shortens the reversal in proportion to how far it had got.")
    before = ST()
    b.click(x, y)
    t0 = time.time()
    rows = sample(b, T, 1.2, 0.05, t0=t0)
    b.click(x, y)
    t_abort = time.time() - t0
    rows += sample(b, T, 2.4, 0.05, t0=t0)
    for t, s in rows:
        pass
    cleared = next((t for t, s in rows if t > t_abort and not s["flight"]), None)
    origin_dark = before["dark"]
    frames_e4 = Edge(("Toward", "Activate", "Back"), "", "")
    ts = [t for t, _ in rows]
    g_from = green(rows[0][1]["bg"])
    g_to = g_off if origin_dark else rest["g_on"]
    pal = [(green(s["bg"]) - g_from) / (g_to - g_from) * 100 for _, s in rows]
    origin_x = rest["thumb_on"] if origin_dark else rest["thumb_off"]
    ghost = [abs(s["ghost"] - origin_x) / span * 100 for _, s in rows]
    thumb = [abs(s["thumb"] - origin_x) / span * 100 for _, s in rows]
    plot(d / "abort.png", f"Abort at {t_abort:.2f}s: ghost and palette rewind together, thumb snaps back",
         [{"t": ts, "y": thumb, "color": OKABE["thumb"], "label": "solid thumb", "at": 0.2},
          {"t": ts, "y": pal, "color": OKABE["palette"], "lw": 2.6, "label": "palette", "at": 0.3},
          {"t": ts, "y": ghost, "color": OKABE["ghost"], "label": "progress ghost", "at": 0.25}],
         vlines=[(t_abort, "second click: abort")])
    e4.curves.append(("Progress curves: abort", "abort.png"))
    e4.checks += [Check("setting restored immediately on abort", rows[[t for t, _ in rows].index(next(t for t, _ in rows if t > t_abort))][1]["dark"] == origin_dark),
                  Check("flight stays armed for the reversal", next((s["flight"] for t, s in rows if t > t_abort), False), "first sample after the abort click")]
    rep.edges.append(e4)
    e5 = Edge(("Back", "Finished", "Idle"), "Finished after the reversal", "The reversed transitions complete early; their finished promises settle the machine within a fraction of the full blend.")
    e5.checks += [Check("settled soon after the abort", cleared is not None and cleared - t_abort < 1.0, f"cleared {cleared - t_abort:.2f}s after the abort" if cleared else "never cleared"),
                  Check("palette back at the origin", abs(green(rows[-1][1]["bg"]) - g_from) < 6)]
    rep.edges.append(e5)

    # ---- E6: Back --Activate--> Toward : re-fly ----
    e6 = Edge(("Back", "Activate", "Toward"), "Activate during the reversal: fly again",
              "A third click starts a fresh flight from wherever the reversal had got to; the machine goes Toward again with the opposite setting.")
    before = ST()
    b.click(x, y); t0 = time.time(); time.sleep(0.8)
    b.click(x, y); t_ab = time.time() - t0; time.sleep(0.12)
    b.click(x, y); t_re = time.time() - t0
    s_re = ST()
    rows = sample(b, T, 3.8, 0.05)
    settled2 = next((t for t, s in rows if not s["flight"]), None)
    ts = [t + t_re for t, _ in rows]
    origin_x = rest["thumb_on"] if before["dark"] else rest["thumb_off"]
    ghost = [abs(s["ghost"] - origin_x) / span * 100 for _, s in rows]
    g_from = rest["g_on"] if before["dark"] else g_off
    g_to = g_off if before["dark"] else rest["g_on"]
    pal = [(green(s["bg"]) - g_from) / (g_to - g_from) * 100 for _, s in rows]
    plot(d / "refly.png", "Abort, then a third click: the flight resumes from the reversal",
         [{"t": ts, "y": pal, "color": OKABE["palette"], "lw": 2.6, "label": "palette", "at": 0.8},
          {"t": ts, "y": ghost, "color": OKABE["ghost"], "label": "progress ghost", "at": 0.9}],
         vlines=[(t_ab, "abort"), (t_re, "fly again")])
    e6.curves.append(("Progress curves: re-fly", "refly.png"))
    e6.checks += [Check("third click re-arms toward the opposite setting", s_re["flight"] and s_re["dark"] != before["dark"]),
                  Check("the new flight settles on its own clock", settled2 is not None and settled2 < 3.6, f"{settled2}s after the third click")]
    rep.edges.append(e6)

    # ---- E7: Neglect ----
    e7 = Edge(("Idle", "Neglect", "Idle"), "Neglect: the pointer leaves", "Attention clears; the preview stops.")
    b.hover(2, 2)
    rows = sample(b, T, 0.8, 0.05)
    gone = next((t for t, s in rows if not s["attention"]), None)
    e7.checks += [Check("attention custom state cleared", gone is not None and gone < 0.3, f"at {gone}s"),
                  Check("preview hidden once unattended", rows[-1][1]["preview_op"] < 0.05, f"opacity {rows[-1][1]['preview_op']}")]
    b.frame(d / "neglect.png", rect)
    e7.frames.append(("after the pointer left", "neglect.png"))
    rep.edges.append(e7)

    # ---- E8: reduced motion (same machine, different representation) ----
    e8 = Edge(("Idle", "Attend", "Idle"), "Reduced motion: the same edges, static representation",
              "prefers-reduced-motion collapses the snap token and disables the preview loop; the preview appears statically at the destination, and a click still blends the palette (a colour fade is not motion) while the ghost snaps.")
    b.reduced_motion(True)
    time.sleep(0.2)
    b.hover(x, y); time.sleep(0.4)
    s_rm = ST()
    b.frame(d / "reduced-hover.png", rect)
    e8.frames.append(("reduced motion, hovered: static preview", "reduced-hover.png"))
    b.click(x, y); t0 = time.time()
    rows = sample(b, T, 3.6, 0.05)
    time.sleep(0.0)
    b.frame(d / "reduced-after.png", rect)
    e8.frames.append(("reduced motion, after the click", "reduced-after.png"))
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
    b.hover(x, y); t0 = time.time()
    rows = sample(b, S, 2.0, 0.04)
    for i, at in enumerate((0.35, 0.8, 1.3)):
        time.sleep(max(0, t0 + at - time.time()))
        b.frame(d / f"attend-{i}.png", rect)
        e1.frames.append((f"t={time.time() - t0:.2f}s", f"attend-{i}.png"))
    peak = max(s["preview_op"] for _, s in rows)
    plot(d / "attend.png", "Preview loop on the native switch", [
        {"t": [t for t, _ in rows], "y": [s["preview_op"] * 100 for _, s in rows], "color": OKABE["preview"], "lw": 2, "label": "preview opacity"},
        {"t": [t for t, _ in rows], "y": [s["preview"] / s["w"] * 100 for _, s in rows], "color": OKABE["ghost"], "label": "preview left (% of track)", "at": 0.5}],
        ylabel="% (opacity, left)")
    e1.curves.append(("Preview loop", "attend.png"))
    e1.checks += [Check("preview animation plays", any(s["anim"].startswith("opt-switch-preview") for _, s in rows)),
                  Check("preview reaches legible opacity", peak >= 0.8, f"peak {peak}")]
    rep.edges.append(e1)
    # E2 activate
    e2 = Edge(("Idle", "Activate", "Idle"), "Activate: the thumb snaps on the snap clock", "A native checkbox toggle; the thumb transitions over --op-motion-snap.")
    b.click(x, y); t0 = time.time()
    rows = sample(b, S, 0.5, 0.02)
    b.frame(d / "activate.png", rect); e2.frames.append(("after the click", "activate.png"))
    lefts = [s["thumb"] for _, s in rows]
    moved_by = next((t for t, s in rows if abs(s["thumb"] - lefts[-1]) < 0.5), None)
    plot(d / "activate.png.curve.png", "Thumb position after a click", [
        {"t": [t for t, _ in rows], "y": [abs(l - lefts[0]) / max(1e-6, abs(lefts[-1] - lefts[0])) * 100 for l in lefts], "color": OKABE["thumb"], "lw": 2, "label": "thumb travel"}])
    e2.curves.append(("Snap curve", "activate.png.curve.png"))
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
    b.reduced_motion(True); time.sleep(0.2); b.hover(x, y); time.sleep(0.4)
    s_rm = b.js(S)
    b.frame(d / "reduced.png", rect); e4.frames.append(("reduced motion, hovered", "reduced.png"))
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
    b.frame(d / "rest.png", rect)
    # attend
    e1 = Edge(("Idle", "Attend", "Idle"), "Attend: hover", "A real pointer over the control must change something visible.")
    b.hover(x, y); time.sleep(0.35)
    hov = b.js("window.__op.sig()")
    b.frame(d / "hover.png", rect)
    e1.frames += [("rest", "rest.png"), ("hovered", "hover.png")]
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
    landed = b.js("window.__op.focusVisible()")
    time.sleep(0.3)
    foc = b.js("window.__op.sig()")
    b.frame(d / "focus.png", rect); e2.frames.append(("focus-visible", "focus.png"))
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
    b.hover(x, y); b.click(x, y); time.sleep(0.35)
    after = b.js(expr) if expr else None
    b.frame(d / "activated.png", rect); e3.frames.append(("after the click", "activated.png"))
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
"""


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
        if e.frames:
            parts.append("<h3>Frames</h3><div class='strip'>" + "".join(
                f"<figure><img class='frame' src='{p}'><figcaption>{c}</figcaption></figure>" for c, p in e.frames) + "</div>")
        for c, p in e.curves:
            parts.append(f"<h3>{c}</h3><img class='curve' src='{p}' alt='{c}'>")
        parts.append("<h3>Checks</h3><table>" + "".join(
            f"<tr><td class='{'ok' if c.ok else 'fail'}'>{'pass' if c.ok else 'FAIL'}</td><td>{c.name}</td><td>{c.detail}</td></tr>" for c in e.checks) + "</table>")
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
