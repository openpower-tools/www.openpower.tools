import itertools, numpy as np, colour, warnings
warnings.filterwarnings('ignore')
exec(open('refit.py').read().split("print('Okabe-Ito colours")[0])  # reuse helpers/matrices only
from colour.blindness import matrix_cvd_Machado2009
OK = {'orange':76.8,'sky blue':236.2,'bluish green':165.5,'yellow':105.0,'blue':244.0,'vermilion':47.5,'reddish purple':346.3}
def lab_of(rgb_lin): return colour.XYZ_to_Lab(colour.sRGB_to_XYZ(enc(rgb_lin)))
def cvd(rgb_lin, kind): return np.clip(matrix_cvd_Machado2009(kind, 1.0) @ rgb_lin, 0, 1)
def dE(a, b): return float(colour.delta_E(lab_of(a), lab_of(b), method='CIE 2000'))
def score(cols):
    names = list(cols); res = {}
    for kind in (None, 'Protanomaly', 'Deuteranomaly', 'Tritanomaly'):
        best = (1e9, None)
        for i, j in itertools.combinations(range(len(names)), 2):
            a, b = cols[names[i]], cols[names[j]]
            if kind: a, b = cvd(a, kind), cvd(b, kind)
            d = dE(a, b)
            if d < best[0]: best = (d, (names[i], names[j]))
        res[kind or 'normal'] = best
    return res
def col(L, C_cap, h): return oklch_to_linear(L, min(C_cap, max_chroma(L, h)), h)

print("Baseline: original Okabe-Ito seven, min pairwise CIEDE2000 (Machado severity 1.0)")
orig = {n: lin(hex_to_srgb(h)) for n, h in {'orange':'#E69F00','sky blue':'#56B4E9','bluish green':'#009E73','yellow':'#F0E442','blue':'#0072B2','vermilion':'#D55E00','reddish purple':'#CC79A7'}.items()}
for k, (d, p) in score(orig).items(): print(f"  {k:13s} {d:5.1f} {p}")

for theme, surf, bands, C_cap in (('light', '#FFFFFF', (0.55, 0.66), 0.14), ('dark', '#2B415F', (0.65, 0.76), 0.14)):
    s_lin = lin(hex_to_srgb(surf))
    print(f"\n{theme} theme, surface {surf}, bands L={bands}, chroma cap {C_cap}")
    # single band, six hues without sky blue
    six = [n for n in OK if n != 'sky blue']
    for L in bands:
        cols = {n: col(L, C_cap, OK[n]) for n in six}
        sc = score(cols)
        print(f"  single band L={L}, six hues (no sky blue): " + ', '.join(f"{k} {d:.1f}" for k, (d, p) in sc.items()) + f"; worst pair {sc['normal'][1]}; min contrast {min(wcag(c, s_lin) for c in cols.values()):.2f}:1")
    # two bands: search subsets of six hues and band assignments, maximise worst-case min distance across conditions
    best = None
    for subset in itertools.combinations(list(OK), 6):
        for assign in itertools.product(bands, repeat=6):
            cols = {n: col(L, C_cap, OK[n]) for n, L in zip(subset, assign)}
            sc = score(cols)
            worst = min(d for d, p in sc.values())
            if best is None or worst > best[0]: best = (worst, subset, assign, sc, cols)
    worst, subset, assign, sc, cols = best
    print(f"  best two-band set (maximin over normal+3 CVD): worst-case min distance {worst:.1f}")
    for n, L in zip(subset, assign):
        print(f"    {n:15s} L={L:.2f} {to_hex(cols[n])} contrast {wcag(cols[n], s_lin):.2f}:1")
    for k, (d, p) in sc.items(): print(f"    {k:13s} {d:5.1f} {p}")
