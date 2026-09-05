import itertools, numpy as np, colour, warnings
warnings.filterwarnings('ignore')
exec(open('refit3.py').read().split("for theme, surf, bands in")[0])
def solve_L_apca(h, C_cap, s_lin, target, light):
    lo, hi = 0.05, 0.98
    for _ in range(60):
        mid = (lo + hi) / 2; c = min(C_cap, max_chroma(mid, h)); ok = abs(apca(oklch_to_linear(mid, c, h), s_lin)) >= target
        if light: (lo, hi) = (mid, hi) if ok else (lo, mid)
        else: (lo, hi) = (lo, mid) if ok else (mid, hi)
    return lo if light else hi
C_cap = 0.14
for surf in ('#FFFFFF', '#2B415F'):
    s_lin = lin(hex_to_srgb(surf)); light = surf == '#FFFFFF'
    print(f"\nSurface {surf}: L needed per hue (C cap {C_cap}) for APCA Lc targets, with the WCAG ratio that results")
    for target in (45, 60, 75):
        row = []
        for n, h in OK.items():
            L = solve_L_apca(h, C_cap, s_lin, target, light); c = col(L, C_cap, h); row.append((n, L, wcag(c, s_lin)))
        binding = min(row, key=lambda t: t[1]) if light else max(row, key=lambda t: t[1])
        print(f"  Lc {target}: " + '  '.join(f"{n.split()[0]} L={L:.2f} ({r:.1f}:1)" for n, L, r in row) + f"  binding {binding[0]}")
# dark theme: what six-hue two-band scheme is possible at Lc >= 45 and Lc >= 60 floors?
s_lin = lin(hex_to_srgb('#2B415F'))
for floor, bands in ((45, (0.75, 0.85)), (60, (0.83, 0.92))):
    print(f"\nDark theme two bands {bands} with APCA floor {floor} (chroma cap {C_cap})")
    best = None
    for subset in itertools.combinations(list(OK), 6):
        for assign in itertools.product(bands, repeat=6):
            cols = {n: col(L, C_cap, OK[n]) for n, L in zip(subset, assign)}
            if min(abs(apca(c, s_lin)) for c in cols.values()) < floor: continue
            sc = score(cols); w = min(d for d, p in sc.values())
            if best is None or w > best[0]: best = (w, subset, assign, cols)
    if best is None: print("  no six-hue assignment meets the floor"); continue
    w, subset, assign, cols = best
    print(f"  maximin worst-case {w:.1f}")
    for n, L in zip(subset, assign): print(f"    band L={L:.2f}: {n}")
    report(cols, s_lin)
