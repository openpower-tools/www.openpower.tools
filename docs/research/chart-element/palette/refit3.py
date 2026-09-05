import itertools, numpy as np, colour, warnings
warnings.filterwarnings('ignore')
exec(open('refit.py').read().split("print('Okabe-Ito colours")[0])
exec(open('refit2.py').read().split('print("Baseline')[0].split("from colour.blindness import matrix_cvd_Machado2009",1)[1])
# APCA-W3 0.0.98G-4g constants as in the perceive-color apca.rs source read on 2026-09-03
def apca_y(rgb_lin):
    s = enc(rgb_lin); y = 0.2126729*s[0]**2.4 + 0.7151522*s[1]**2.4 + 0.0721750*s[2]**2.4
    return y + (0.022 - y)**1.414 if y < 0.022 else y
def apca(text_lin, bg_lin):
    yt, yb = apca_y(text_lin), apca_y(bg_lin)
    if yb > yt: sapc = (yb**0.56 - yt**0.57) * 1.14; lc = 0 if sapc < 0.1 else sapc - 0.027
    else: sapc = (yb**0.65 - yt**0.62) * 1.14; lc = 0 if sapc > -0.1 else sapc + 0.027
    return lc * 100
def report(cols, s_lin):
    for n, c in cols.items(): print(f"    {n:15s} {to_hex(c)}  WCAG {wcag(c, s_lin):.2f}:1  APCA Lc {abs(apca(c, s_lin)):.0f}")
    for k, (d, p) in score(cols).items(): print(f"    {k:13s} {d:5.1f} {p}")

for theme, surf, bands in (('light', '#FFFFFF', (0.55, 0.65)), ('dark', '#2B415F', (0.66, 0.76))):
    s_lin = lin(hex_to_srgb(surf))
    for C_cap in (0.14, 0.16):
        print(f"\n== {theme} theme, surface {surf}, bands {bands}, chroma cap {C_cap}")
        best = None
        for subset in itertools.combinations(list(OK), 6):
            for assign in itertools.product(bands, repeat=6):
                cols = {n: col(L, C_cap, OK[n]) for n, L in zip(subset, assign)}
                if min(wcag(c, s_lin) for c in cols.values()) < 3.0: continue
                sc = score(cols); worst = min(d for d, p in sc.values())
                if best is None or worst > best[0]: best = (worst, subset, assign, cols)
        worst, subset, assign, cols = best
        print(f"  six series, two bands, maximin worst-case {worst:.1f}:")
        for n, L in zip(subset, assign): print(f"    band L={L:.2f}: {n}")
        report(cols, s_lin)
        for k in (4, 5):
            best = None
            for subset in itertools.combinations(list(OK), k):
                for L in bands:
                    cols = {n: col(L, C_cap, OK[n]) for n in subset}
                    if min(wcag(c, s_lin) for c in cols.values()) < 3.0: continue
                    sc = score(cols); w = min(d for d, p in sc.values())
                    if best is None or w > best[0]: best = (w, subset, L, cols)
            w, subset, L, cols = best
            print(f"  best {k} series in ONE band (L={L:.2f}): worst-case {w:.1f}")
            report(cols, s_lin)
