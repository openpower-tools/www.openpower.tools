# /// script
# requires-python = ">=3.11"
# dependencies = ["websocket-client>=1.8"]
# ///
"""Capture the chart label specimen in the site's own faces.

The page comes from `cargo run -p op-verify --bin specimen-page`: one
cell per label the chart draws, each in its real family, weight and size
over the surface it sits on, each with the advance sum op-chart would
place it by written onto the cell. This loads that page in
chrome-headless-shell with the served faces, proves the faces are the
real ones rather than a fallback, and writes one PNG per theme at a
device pixel ratio of 2, with a JSON of everything only the browser
knows: the resolved palette, the faces' own vertical metrics, and the
advance the browser lays each cell's text out at.

This is a script beside the interaction report rather than a mode of it.
The report drives a contract of controls through an interaction machine
on a synthetic clock and gates CI on the assertions that come out; a
specimen has no controls, no machine and no assertions, only one still
picture per theme. What it does need is the report's browser plumbing,
so it imports that rather than restating it.

It does not take the report's clock with it. chrome-headless-shell lays
text out with whole-pixel glyph advances: a digit that advances 7.2 px in
the served face advances 7 px there, so a run of seven measures 49 px
rather than 50.4. Chrome's own headless lays the same run out at 50.400.
The question here is what a reader's browser paints, so the specimen is
captured in Chrome's own headless by default; `--browser shell` captures
in the shell instead, which is how that difference was found and how it
can be seen again.

It captures two pages. The specimen is one still per theme, measured in
ink. The kerning sweep is no picture at all: a De Bruijn sequence over
the block the advance tables cover, holding every ordered pair of
characters exactly once, laid out in one text node per face and shaping,
whose per-character positions are read in one go. One layout gives the
kern of all 9025 pairs, which is why the sweep can be exhaustive.

    cargo run -p op-verify --bin specimen-page -- reports/specimens
    uv run tools/interaction_report/specimen_capture.py --dist dist
    cargo run -p op-verify --bin specimen-measure -- reports/specimens
    cargo run -p op-verify --bin sweep-measure -- reports/specimens
"""
from __future__ import annotations

import argparse
import base64
import json
import os
import shutil
import subprocess
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import report  # noqa: E402  (the browser plumbing, imported after the path is set)

# The specimen is captured at two device pixels to the CSS pixel: whole,
# so every cell box in the page is a whole box of image pixels, and high
# enough that the antialiasing of a 12 px glyph is resolved rather than
# averaged away.
DPR = 2.0
# The two themes the site ships, which are the two surfaces every label
# has to sit on.
THEMES = ("light", "dark")
# How far a measured advance may be from the advance table before the
# face is judged a fallback rather than the served one. Both faces set
# every digit on one advance, so a run of digits is exact in the table
# and unmistakable in any other face: the served face gives 50.400 px for
# seven digits at 12 px, Arial gives 46.7 and DejaVu Sans 53.4.
FALLBACK_TOLERANCE = 0.25
# The advance every served weight sets a digit on, in thousandths of the
# em. Both faces set the ten digits on one advance, which is what makes a
# run of digits a test of identity rather than of shape.
DIGIT_PER_MILLE = 600.0
# How much larger than the drawn size the vertical metrics are measured
# at, so a whole-pixel ink extent resolves the outline to four figures.
METRIC_SCALE = 100


def serve_root(dist: Path, pages: list[Path], work: Path) -> Path:
    """A directory to serve the specimen from: the page, and links to the
    faces and the palette under the names the page asks for. The site's
    own font stylesheet points at `/font-*.woff2`, so those have to sit at
    a server root; making that root here rather than writing into dist
    leaves the built site untouched."""
    root = work / "serve"
    if root.exists():
        shutil.rmtree(root)
    root.mkdir(parents=True)
    for page in pages:
        shutil.copy(page, root / page.name)
    for pattern, name in (("fonts-*.css", "fonts.css"), ("theme-*.css", "theme.css")):
        hits = sorted(dist.glob(pattern))
        if not hits:
            sys.exit(f"no {pattern} in {dist}: build the site first, or pass --dist")
        os.symlink(hits[0].resolve(), root / name)
    faces = sorted(dist.glob("font-*.woff2"))
    if not faces:
        sys.exit(f"no font-*.woff2 in {dist}")
    for face in faces:
        os.symlink(face.resolve(), root / face.name)
    return root


def start_server(root: Path, probe: str = "specimen.html") -> tuple[subprocess.Popen, str]:
    """A plain static server on `root`, and its base URL. `probe` is a path
    the root really has, since a server that answers 404 to everything is
    not a server that is up. The process is killed if it never answers, so
    a failed start leaves nothing behind."""
    port = report.free_port()
    proc = subprocess.Popen([sys.executable, "-m", "http.server", str(port)], cwd=root,
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    base = f"http://127.0.0.1:{port}"
    deadline = time.time() + 30
    while True:
        try:
            urllib.request.urlopen(f"{base}/{probe}", timeout=2).read()
            return proc, base
        except (urllib.error.URLError, OSError):
            if proc.poll() is not None:
                sys.exit(f"the server on {root} exited early (code {proc.returncode})")
            if time.time() > deadline:
                proc.kill()
                sys.exit(f"the server on {root} did not answer for /{probe} within 30s")
            time.sleep(0.2)


def hexed(css: str) -> str:
    """`rgb(r, g, b)` or `rgba(r, g, b, a)` as `#RRGGBB`. The palette is
    registered with `@property ... <color>`, so a computed value comes
    back in that form whatever the stylesheet wrote."""
    inside = css[css.index("(") + 1:css.rindex(")")]
    parts = [p for p in inside.replace("/", " ").replace(",", " ").split() if p]
    r, g, b = (round(float(p)) for p in parts[:3])
    return f"#{r:02X}{g:02X}{b:02X}"


def resolve_colours(b: report.Browser, tokens: list[str]) -> dict:
    """Each token as the browser paints it, read off a probe element so a
    registered custom property, a fallback or a colour function all come
    back as the colour that reaches the screen."""
    got = b.js("""(() => {
      const probe = document.createElement('span');
      probe.style.cssText = 'position:absolute;left:-9999px;top:0';
      document.body.appendChild(probe);
      const out = {};
      for (const token of %s) {
        probe.style.color = 'rgb(1, 2, 3)';
        probe.style.color = `var(${token})`;
        out[token] = getComputedStyle(probe).color;
      }
      probe.remove();
      return out;
    })()""" % json.dumps(tokens))
    resolved = {}
    for token, css in got.items():
        if css.replace(" ", "") == "rgb(1,2,3)":
            sys.exit(f"{token} did not resolve: the palette stylesheet is not loaded")
        resolved[token] = hexed(css)
    return resolved


def face_metrics(b: report.Browser, px: float, faces: list[str]) -> dict:
    """The vertical metrics of each weight, taken from the face itself by
    the engine that paints it: the cap height and the x height are the ink
    tops of H and x, the descender depth is the ink bottom of p, and the
    font box is the face's own ascent and descent. Measured rather than
    assumed, because the advance tables carry advances and nothing else.

    Measured at a hundred times the size the chart draws and divided down.
    An ink extent comes back as a whole number of pixels, so at 12 px the
    cap height would be 8 and the x height 6, which is a rule drawn a
    third of a pixel from where the glyph stops. At 1200 px the same whole
    number is the outline to four figures, and the outline is what the
    metric is."""
    return b.js("""(() => {
      const c = document.createElement('canvas').getContext('2d');
      const out = {}, scale = %s, px = %s;
      for (const weight of %s) {
        c.font = `${weight} ${px * scale}px "IBM Plex Sans"`;
        const cap = c.measureText('H'), ex = c.measureText('x'), desc = c.measureText('p');
        const box = c.measureText('Hxp');
        out[weight] = {
          cap: cap.actualBoundingBoxAscent / scale,
          x: ex.actualBoundingBoxAscent / scale,
          descender: desc.actualBoundingBoxDescent / scale,
          font_ascent: box.fontBoundingBoxAscent / scale,
          font_descent: box.fontBoundingBoxDescent / scale,
        };
      }
      return out;
    })()""" % (METRIC_SCALE, px, json.dumps(faces)))


def prove_the_faces(b: report.Browser, spec: dict) -> dict:
    """Whether the served faces are the ones being drawn, answered twice.

    `document.fonts.check` says the face is loaded and would be used for
    the text; a run of digits says it is the right face, because both
    served weights set every digit on 600 thousandths of the em and no
    fallback the stack names does. A page that captured Arial would look
    plausible and measure nothing.

    A run of digits also says how the browser positions glyphs: the served
    face gives 50.400 px for seven of them at 12 px where the advance is
    kept, and 49 where each advance is rounded to a whole pixel first.
    Both are the right face, and which one is drawing is recorded rather
    than refused, since it is a fact about the browser and the sheet has
    to state it."""
    px = spec["text_px"]
    text = "".join(sorted({c for cell in spec["cells"] for c in cell["text"]}))
    probe = "0" * 7
    exact = DIGIT_PER_MILLE / 1000.0 * px * len(probe)
    whole = round(DIGIT_PER_MILLE / 1000.0 * px) * len(probe)
    got = b.js("""(() => {
      const c = document.createElement('canvas').getContext('2d');
      const out = {check: {}, digits: {}, status: []};
      for (const weight of %s) {
        out.check[weight] = document.fonts.check(`${weight} %spx "IBM Plex Sans"`, %s);
        c.font = `${weight} %spx "IBM Plex Sans"`;
        out.digits[weight] = c.measureText(%s).width;
      }
      for (const f of document.fonts) out.status.push(`${f.family} ${f.weight} ${f.style} ${f.status}`);
      return out;
    })()""" % (json.dumps(spec["faces"]), px, json.dumps(text), px, json.dumps(probe)))
    positioning = {}
    for weight in spec["faces"]:
        if not got["check"][weight]:
            sys.exit(f"IBM Plex Sans {weight} is not loaded: document.fonts.check says no")
        wide = got["digits"][weight]
        if abs(wide - exact) <= FALLBACK_TOLERANCE:
            positioning[weight] = "subpixel"
        elif abs(wide - whole) <= FALLBACK_TOLERANCE:
            positioning[weight] = "whole pixel advances"
        else:
            sys.exit(f"IBM Plex Sans {weight} measures {wide:.3f} px for {probe!r} where the "
                     f"served face gives {exact:.3f} (or {whole:.3f} rounded per glyph): "
                     f"a fallback is being drawn")
    got["probe"] = probe
    got["digits_exact"] = exact
    got["digits_whole_pixel"] = whole
    got["positioning"] = positioning
    return got


def browser_advances(b: report.Browser, spec: dict) -> dict:
    """The advance the browser lays each cell's text out at, from the text
    element itself. This is the pen's travel with whatever shaping the
    cell asks for, which is the number the advance sum claims to be."""
    ids = [cell["id"] for cell in spec["cells"]]
    return b.js("""(() => {
      const out = {};
      for (const id of %s) {
        const t = document.getElementById(id).querySelector('text');
        out[id] = t.getComputedTextLength();
      }
      return out;
    })()""" % json.dumps(ids))


def capture(b: report.Browser, base: str, spec: dict, theme: str, out: Path, binary: str, name: str) -> dict:
    """One theme: load the page, set the theme, prove the faces, measure
    what only the browser knows, and write the picture."""
    width, height = spec["page"]["width"], spec["page"]["height"]
    b.call("Emulation.setDeviceMetricsOverride", width=width, height=height,
           deviceScaleFactor=DPR, mobile=False)
    b.call("Emulation.setEmulatedMedia",
           features=[{"name": "prefers-color-scheme", "value": theme},
                     {"name": "prefers-reduced-motion", "value": "reduce"}])
    b.goto(f"{base}/specimen.html", "document.readyState === 'complete'")
    b.js(f"document.documentElement.dataset.theme = {theme!r}")
    b.js("document.fonts.ready.then(() => { window.__op_fonts = true; }); true")
    for _ in range(600):
        b.frame() if b.synthetic else b.wait(0.05)
        if b.js("window.__op_fonts === true"):
            break
    else:
        sys.exit("document.fonts.ready never settled")
    fonts = prove_the_faces(b, spec)
    colours = resolve_colours(b, spec["tokens"])
    metrics = face_metrics(b, spec["text_px"], spec["faces"])
    advances = browser_advances(b, spec)
    if b.synthetic:
        # under begin frame control the frame carries the picture
        b.frame()
        png = base64.b64decode(b.frame({"format": "png"})["screenshotData"])
    else:
        png = base64.b64decode(b.call("Page.captureScreenshot", format="png")["data"])
    image = out / f"specimen-{theme}.png"
    image.write_bytes(png)
    got = png_size(png)
    want = (int(width * DPR), int(height * DPR))
    if got != want:
        sys.exit(f"the capture is {got[0]} by {got[1]} where the page is {want[0]} by {want[1]} "
                 f"at a device pixel ratio of {DPR}")
    record = {
        "theme": theme,
        "dpr": DPR,
        "image": image.name,
        "page": {"width": width, "height": height},
        "browser": name,
        "binary": f"{binary} ({report.binary_version(binary)})",
        "fonts": fonts,
        "colours": colours,
        "metrics": metrics,
        "advances": advances,
    }
    path = out / f"capture-{theme}.json"
    path.write_text(json.dumps(record, indent=1) + "\n")
    return record


def sweep(b: report.Browser, base: str, spec: dict, out: Path, binary: str, name: str) -> dict:
    """The kerning sweep: load the page, prove the faces, and read every
    character's position in every run.

    The reading is one call per run and 9026 positions out of it, because
    the whole point of a De Bruijn sequence is that one layout carries
    every pair. Each run also gives back the extent of every character,
    so a pair that laid out as a single glyph can be counted rather than
    quietly averaged into the kerns: an extent that does not match the
    step to the next character is a character that is not one glyph of
    its own."""
    b.call("Emulation.setDeviceMetricsOverride", width=1200, height=400,
           deviceScaleFactor=DPR, mobile=False)
    b.goto(f"{base}/sweep.html", "document.readyState === 'complete'")
    b.js("document.fonts.ready.then(() => { window.__op_fonts = true; }); true")
    for _ in range(600):
        b.frame() if b.synthetic else b.wait(0.05)
        if b.js("window.__op_fonts === true"):
            break
    else:
        sys.exit("document.fonts.ready never settled on the sweep page")
    faces = sorted({run["face"] for run in spec["runs"]})
    fonts = prove_the_faces(b, {"text_px": spec["text_px"], "faces": faces,
                                "cells": [{"text": spec["sequence"]}]})
    ids = [run["id"] for run in spec["runs"]]
    runs = b.js("""(() => {
      const out = {};
      for (const id of %s) {
        const t = document.getElementById(id);
        const n = t.getNumberOfChars();
        const x = new Array(n), width = new Array(n);
        for (let i = 0; i < n; i++) {
          x[i] = t.getStartPositionOfChar(i).x;
          width[i] = t.getExtentOfChar(i).width;
        }
        out[id] = {chars: n, x: x, width: width, total: t.getComputedTextLength()};
      }
      return out;
    })()""" % json.dumps(ids))
    for run in spec["runs"]:
        got = runs[run["id"]]["chars"]
        if got != spec["length"]:
            sys.exit(f"{run['id']} laid out {got} characters where the sweep is {spec['length']}: "
                     f"the page lost some of its text, most likely its whitespace")
    record = {
        "order": spec["order"],
        "alphabet": spec["alphabet"],
        "pairs": spec["pairs"],
        "length": spec["length"],
        "text_px": spec["text_px"],
        "browser": name,
        "binary": f"{binary} ({report.binary_version(binary)})",
        "fonts": fonts,
        "runs": runs,
    }
    path = out / "sweep-capture.json"
    path.write_text(json.dumps(record) + "\n")
    return record


LABEL_WIDTHS = (900, 360)
"""The viewports the chart's labels are read at: one where the wide
pre-render is what the container query keeps, and one below the width the
element switches at."""

CHART_READY = ("!!document.querySelector('opt-chart')?.shadowRoot?.querySelector('svg.chart')"
               " && document.readyState === 'complete'")

READ_LABELS = """(() => {
  const c = document.querySelector('opt-chart'), sr = c && c.shadowRoot;
  if (!sr) return null;
  const svgs = [...sr.querySelectorAll('svg.chart')];
  const vis = svgs.find(s => s.getBoundingClientRect().width > 0) || svgs[0];
  if (!vis) return null;
  const labels = [];
  for (const t of vis.querySelectorAll('text')) {
    if (getComputedStyle(t).display === 'none') continue;
    // the value axis' own name is turned on its side, where a width along
    // x is not what keeps it clear of anything
    if (/rotate/.test(t.getAttribute('transform') || '')) continue;
    const n = t.getNumberOfChars();
    if (!n) continue;
    // the drawn advance, read off the glyphs themselves rather than from
    // getComputedTextLength, because a label pinned by textLength is
    // exactly the case this has to tell apart and the two disagree there
    const m = t.getCTM();
    const at = p => ({x: m.a * p.x + m.c * p.y + m.e, y: m.b * p.x + m.d * p.y + m.f});
    const start = at(t.getStartPositionOfChar(0)), end = at(t.getEndPositionOfChar(n - 1));
    const bb = t.getBBox(), ink = at({x: bb.x, y: bb.y});
    labels.push({
      class: t.getAttribute('class') || '', text: t.textContent,
      left: +start.x.toFixed(4), right: +end.x.toFixed(4), baseline: +start.y.toFixed(4),
      ink_left: +ink.x.toFixed(4), ink_right: +(ink.x + bb.width * m.a).toFixed(4),
      ink_top: +ink.y.toFixed(4), ink_bottom: +(ink.y + bb.height * m.d).toFixed(4),
      measured: +t.getComputedTextLength().toFixed(4),
      pinned: t.hasAttribute('textLength') ? +t.getAttribute('textLength') : null,
    });
  }
  const r = vis.getBoundingClientRect();
  return {hydrated: c.matches(':state(hydrated)'), rendered_by: vis.dataset.renderedBy || '',
          chart_width: +r.width.toFixed(2), labels: labels};
})()"""


def label_view(b: report.Browser, base: str, width: int, blocked: bool, load_at: int = 0) -> dict:
    """Load the chart's own page at `width` and read every label the
    browser drew, in the svg's own coordinates.

    The positions come from the glyphs rather than from the markup, so a
    label carried by a transformed group lands in the same coordinate
    system as the rest and a label pinned by `textLength` is measured as
    drawn and not as asked for."""
    # loading at one width and then narrowing without a reload is what makes
    # the element re-render in place, where loading narrow lets it keep the
    # pre-render it was served. Both are what a reader can meet.
    b.call("Emulation.setDeviceMetricsOverride", width=load_at or width, height=1000,
           deviceScaleFactor=DPR, mobile=False)
    b.goto(f"{base}/component/chart/", CHART_READY)
    b.js("document.fonts.ready.then(() => { window.__op_fonts = true; }); true")
    for _ in range(600):
        b.frame() if b.synthetic else b.wait(0.05)
        if b.js("window.__op_fonts === true"):
            break
    else:
        sys.exit(f"document.fonts.ready never settled on the chart page at {width} px")
    # the element re-lays out once when the face it measures with is
    # missing, so give that its frames before reading anything
    for _ in range(30):
        b.frame() if b.synthetic else b.wait(0.02)
    if load_at and load_at != width:
        b.call("Emulation.setDeviceMetricsOverride", width=width, height=1000,
               deviceScaleFactor=DPR, mobile=False)
        for _ in range(60):
            b.frame() if b.synthetic else b.wait(0.02)
    read = b.js(READ_LABELS)
    if not read:
        sys.exit(f"no chart on the page at {width} px")
    return {"width": float(width), "hydrated": bool(read["hydrated"]), "blocked": blocked,
            "rendered_by": read["rendered_by"], "chart_width": read["chart_width"],
            "labels": read["labels"]}


def label_boxes(b: report.Browser, dist: Path, out: Path, binary: str, name: str, text_px: float) -> dict:
    """Every label the chart draws, measured by the browser, at the widths
    the element switches between, and once more with the served faces
    blocked.

    The blocked load is the one the advance tables are wrong for: the
    element measures what it is really set in and re-lays out, and the
    labels that must fit a fixed slot stay pinned by `textLength`."""
    # the specimen's own root holds only the pages and the faces, so the
    # chart's page needs the built site served as it is deployed
    server, base = start_server(dist, probe="component/chart/")
    try:
        views = [label_view(b, base, width, blocked=False) for width in LABEL_WIDTHS]
        # and the narrow width reached by resizing, which is the live
        # re-render rather than the pre-render the server sent
        views.append(label_view(b, base, LABEL_WIDTHS[-1], blocked=False, load_at=LABEL_WIDTHS[0]))
        b.call("Network.enable")
        try:
            # the served faces, by the names the build gives them
            b.call("Network.setBlockedURLs", urls=["*plexsans*"])
            views.append(label_view(b, base, LABEL_WIDTHS[-1], blocked=True))
        finally:
            b.call("Network.setBlockedURLs", urls=[])
            b.call("Emulation.clearDeviceMetricsOverride")
    finally:
        server.kill()
    record = {"browser": name, "binary": f"{binary} ({report.binary_version(binary)})",
              "text_px": text_px, "views": views}
    path = out / "labels-capture.json"
    path.write_text(json.dumps(record) + "\n")
    return record


def png_size(data: bytes) -> tuple[int, int]:
    """A PNG's pixel size, read from its IHDR, so this script needs no
    image library to hold the capture to the size it asked for."""
    if data[:8] != b"\x89PNG\r\n\x1a\n" or data[12:16] != b"IHDR":
        raise ValueError("not a PNG")
    return int.from_bytes(data[16:20], "big"), int.from_bytes(data[20:24], "big")


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--dist", default="dist", help="the built site, for the served faces and the palette")
    ap.add_argument("--specimens", default="reports/specimens", help="where the page is and the capture goes")
    ap.add_argument("--browser", choices=("chrome", "shell"), default="chrome",
                    help="chrome: Chrome's own headless, which keeps a glyph's advance (default). "
                         "shell: chrome-headless-shell, which rounds every advance to a whole pixel")
    ap.add_argument("--chrome", help="the Chrome or Chromium binary to drive")
    ap.add_argument("--shell", help="the chrome-headless-shell to drive")
    ap.add_argument("--themes", default=",".join(THEMES), help="comma-separated themes to capture")
    ap.add_argument("--only", choices=("specimen", "sweep", "labels", "both", "all"), default="both",
                    help="which page to capture: both is the specimen and the sweep, all adds the labels (default: both)")
    args = ap.parse_args()

    dist = Path(args.dist)
    out = Path(args.specimens)
    html = out / "specimen.html"
    if not html.exists():
        sys.exit(f"no {html}: run `cargo run -p op-verify --bin specimen-page -- {out}` first")
    spec = json.loads((out / "specimen.json").read_text())
    if spec["dpr"] != DPR:
        sys.exit(f"the manifest wants a device pixel ratio of {spec['dpr']} and this captures at {DPR}")
    sweep_page = out / "sweep.html"
    sweep_spec = json.loads((out / "sweep.json").read_text()) if sweep_page.exists() else None
    if args.only in ("sweep", "both", "all") and sweep_spec is None:
        sys.exit(f"no {sweep_page}: run the page generator first")

    if args.browser == "shell":
        binary = report.find_shell(args.shell)
        if not binary:
            sys.exit("no chrome-headless-shell found: pass --shell PATH or set OP_HEADLESS_SHELL")
    else:
        binary = report.find_chrome(args.chrome)
    print(f"{args.browser}: {binary} ({report.binary_version(binary)})", flush=True)

    work = out / ".work"
    work.mkdir(parents=True, exist_ok=True)
    pages = [html] + ([sweep_page] if sweep_spec else [])
    root = serve_root(dist, pages, work)
    server, base = start_server(root)
    # the report's browser reads this when it starts and when it captures
    report.DPR = DPR
    b = report.Browser(binary, work, synthetic=(args.browser == "shell"))
    written = []
    try:
        for theme in (args.themes.split(",") if args.only in ("specimen", "both", "all") else []):
            record = capture(b, base, spec, theme, out, binary, args.browser)
            proof = record["fonts"]
            digits = ", ".join(f"{w} {proof['digits'][w]:.3f} px, {proof['positioning'][w]}"
                               for w in sorted(proof["digits"]))
            print(f"{theme}: {out / record['image']}  faces proved by {proof['probe']!r} "
                  f"against {proof['digits_exact']:.3f} px ({digits})", flush=True)
            written.append(record)
        if args.only in ("sweep", "both", "all"):
            swept = sweep(b, base, sweep_spec, out, binary, args.browser)
            print(f"sweep: {swept['pairs']} ordered pairs in {swept['length']} characters, "
                  f"{len(swept['runs'])} runs, {out / 'sweep-capture.json'}", flush=True)
        if args.only in ("labels", "all"):
            read = label_boxes(b, dist, out, binary, args.browser, spec["text_px"])
            drawn = ", ".join(f"{v['width']:.0f} px: {len(v['labels'])} labels"
                              + (" with the faces blocked" if v["blocked"] else "")
                              for v in read["views"])
            print(f"labels: {drawn}; {out / 'labels-capture.json'}", flush=True)
    finally:
        b.close()
        server.kill()
        shutil.rmtree(work, ignore_errors=True)
    for record in written:
        print(f"capture: {out / ('capture-' + record['theme'] + '.json')}")


if __name__ == "__main__":
    main()
