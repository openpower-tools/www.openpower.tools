"""Worked OKLCH re-fit of the Okabe-Ito hues against the brief's surfaces.
Oklab<->linear sRGB matrices: Björn Ottosson's published values as reproduced in the
richdevtools script fetched on 2026-09-03; WCAG relative luminance per the Understanding
1.4.11 key terms; CIEDE2000 and Machado 2009 CVD via colour-science.
"""
import numpy as np, colour
from colour.blindness import matrix_cvd_Machado2009

M1 = np.array([[0.4122214708, 0.5363325363, 0.0514459929],
               [0.2119034982, 0.6806995451, 0.1073969566],
               [0.0883024619, 0.2817188376, 0.6299787005]])
M2 = np.array([[0.2104542553, 0.7936177850, -0.0040720468],
               [1.9779984951, -2.4285922050, 0.4505937099],
               [0.0259040371, 0.7827717662, -0.8086757660]])

def lin(c):
    c = np.asarray(c, float)
    return np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)
def enc(c):
    c = np.clip(np.asarray(c, float), 0, 1)
    return np.where(c <= 0.0031308, 12.92 * c, 1.055 * c ** (1 / 2.4) - 0.055)
def hex_to_srgb(h):
    h = h.lstrip('#'); return np.array([int(h[i:i+2], 16) / 255 for i in (0, 2, 4)])
def srgb_to_oklab(s):
    l, m, s_ = np.cbrt(M1 @ lin(s)); return M2 @ np.array([l, m, s_])
def oklab_to_linear(lab):
    lms = (np.linalg.inv(M2) @ lab) ** 3; return np.linalg.inv(M1) @ lms
def oklch_to_linear(L, C, h):
    return oklab_to_linear(np.array([L, C * np.cos(np.radians(h)), C * np.sin(np.radians(h))]))
def in_gamut(rgb_lin, eps=1e-6): return np.all(rgb_lin >= -eps) and np.all(rgb_lin <= 1 + eps)
def max_chroma(L, h):
    lo, hi = 0.0, 0.4
    for _ in range(40):
        mid = (lo + hi) / 2
        if in_gamut(oklch_to_linear(L, mid, h)): lo = mid
        else: hi = mid
    return lo
def Y(rgb_lin): return 0.2126 * rgb_lin[0] + 0.7152 * rgb_lin[1] + 0.0722 * rgb_lin[2]
def wcag(a_lin, b_lin):
    ya, yb = Y(a_lin), Y(b_lin); hi, lo = max(ya, yb), min(ya, yb); return (hi + 0.05) / (lo + 0.05)
def to_hex(rgb_lin): return '#%02X%02X%02X' % tuple(int(round(v * 255)) for v in enc(rgb_lin))

OKABE = {'orange': '#E69F00', 'sky blue': '#56B4E9', 'bluish green': '#009E73', 'yellow': '#F0E442',
         'blue': '#0072B2', 'vermilion': '#D55E00', 'reddish purple': '#CC79A7'}
SURF = {'light #FFFFFF': '#FFFFFF', 'dark #2B415F': '#2B415F'}

print('Okabe-Ito colours in OKLCH and WCAG contrast against the brief surfaces')
hues = {}
for name, hx in OKABE.items():
    lab = srgb_to_oklab(hex_to_srgb(hx)); L = lab[0]; C = np.hypot(lab[1], lab[2]); h = np.degrees(np.arctan2(lab[2], lab[1])) % 360
    hues[name] = h
    rl = lin(hex_to_srgb(hx))
    print(f"  {name:15s} {hx}  L={L:.3f} C={C:.3f} h={h:6.1f}   vs white {wcag(rl, lin(hex_to_srgb('#FFFFFF'))):.2f}:1   vs #2B415F {wcag(rl, lin(hex_to_srgb('#2B415F'))):.2f}:1")

def solve_L(h, C_cap, surf_lin, target, light_surface):
    # find L where contrast == target; chroma is min(C_cap, max in gamut at that L)
    lo, hi = 0.05, 0.98
    for _ in range(60):
        mid = (lo + hi) / 2
        c = min(C_cap, max_chroma(mid, h))
        r = wcag(oklch_to_linear(mid, c, h), surf_lin)
        # on a light surface contrast falls as L rises; on a dark surface it rises with L
        ok = r >= target
        if light_surface:
            if ok: lo = mid
            else: hi = mid
        else:
            if ok: hi = mid
            else: lo = mid
    return lo if light_surface else hi

for C_cap in (0.10, 0.12, 0.14):
    print(f"\nChroma cap {C_cap}: lightness L at which each hue just reaches the target ratio")
    for sname, shx in SURF.items():
        s_lin = lin(hex_to_srgb(shx)); light = sname.startswith('light')
        for target in (3.0, 4.5):
            row = []
            for name, h in hues.items():
                L = solve_L(h, C_cap, s_lin, target, light); row.append((name, L))
            worst = min(row, key=lambda t: t[1]) if light else max(row, key=lambda t: t[1])
            print(f"  {sname:14s} target {target}:1  " + '  '.join(f"{n.split()[0]}={L:.2f}" for n, L in row) + f"   binding: {worst[0]} (L {worst[1]:.2f})")

def build(L, C_cap, names):
    out = {}
    for n in names:
        h = hues[n]; c = min(C_cap, max_chroma(L, h)); out[n] = oklch_to_linear(L, c, h)
    return out

def lab_of(rgb_lin):
    XYZ = colour.sRGB_to_XYZ(enc(rgb_lin)); return colour.XYZ_to_Lab(XYZ)
def cvd(rgb_lin, kind):
    M = matrix_cvd_Machado2009(kind, 1.0); return np.clip(M @ rgb_lin, 0, 1)
def min_pair(cols, kind=None):
    names = list(cols); best = (1e9, None)
    for i in range(len(names)):
        for j in range(i + 1, len(names)):
            a, b = cols[names[i]], cols[names[j]]
            if kind: a, b = cvd(a, kind), cvd(b, kind)
            d = float(colour.delta_E(lab_of(a), lab_of(b), method='CIE 2000'))
            if d < best[0]: best = (d, (names[i], names[j]))
    return best

six = ['orange', 'sky blue', 'bluish green', 'blue', 'vermilion', 'reddish purple']
for theme, L, C_cap, shx in (('light', 0.55, 0.12, '#FFFFFF'), ('light', 0.52, 0.12, '#FFFFFF'),
                             ('dark', 0.78, 0.12, '#2B415F'), ('dark', 0.80, 0.12, '#2B415F')):
    cols = build(L, C_cap, six); s_lin = lin(hex_to_srgb(shx))
    print(f"\nCandidate {theme} theme, L={L}, C cap {C_cap}, surface {shx}")
    for n, rgb in cols.items():
        print(f"  {n:15s} {to_hex(rgb)}  contrast {wcag(rgb, s_lin):.2f}:1")
    print("  min pairwise CIEDE2000: normal %.1f %s" % min_pair(cols))
    for kind in ('Protanomaly', 'Deuteranomaly', 'Tritanomaly'):
        d, pair = min_pair(cols, kind); print(f"    {kind:13s} (severity 1.0) {d:.1f} {pair}")
    print("  min CIEDE2000 to surface: %.1f" % min(float(colour.delta_E(lab_of(rgb), lab_of(s_lin), method='CIE 2000')) for rgb in cols.values()))
