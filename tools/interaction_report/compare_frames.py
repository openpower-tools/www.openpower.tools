# /// script
# requires-python = ">=3.11"
# dependencies = ["websocket-client>=1.8", "pillow>=10", "rangehttpserver>=1.4"]
# ///
"""Compare the images two interaction reports drew, perceptually.

A reproducible run is not a bit-identical one: a frame boundary falls where
it falls, and a transition caught a hundredth of a second later paints
slightly different pixels. So two runs' artefacts are held to what a reader
would notice instead. Each pair is reduced to a common small size, which is
also what removes differences no eye resolves, and compared in CIEDE2000:
the average difference must stay under one unit, roughly the smallest a
person can see side by side, and almost no pixel may exceed three.

The colour maths is the one the report itself uses, imported rather than
copied, so both hold to the same numbers (the import validates that port
against its reference pairs). Hence this script's dependencies are the
report's own.

    uv run tools/interaction_report/compare_frames.py DIR_A DIR_B
"""
import importlib.util
import pathlib
import sys

from PIL import Image

MAX_SIDE = 256  # the size both images are reduced to before they are compared
MEAN_LIMIT = 1.0  # average CIEDE2000 over the image
OUTLIER_LIMIT = 3.0  # no more than OUTLIER_SHARE of pixels may pass this
OUTLIER_SHARE = 0.01


def _report():
    """The report module, for its validated CIEDE2000 port."""
    path = pathlib.Path(__file__).with_name("report.py")
    spec = importlib.util.spec_from_file_location("op_report", path)
    module = importlib.util.module_from_spec(spec)
    # registered before it runs: a dataclass declared in the module looks its
    # own module up by name while it is being built
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


REPORT = _report()


def reduced(path: pathlib.Path, size: tuple[int, int]) -> list:
    im = Image.open(path).convert("RGB").resize(size, Image.LANCZOS)
    return list(im.getdata())


def difference(a: pathlib.Path, b: pathlib.Path) -> tuple[float, float, str]:
    """Mean CIEDE2000, the share of pixels past OUTLIER_LIMIT, and a note."""
    ia, ib = Image.open(a), Image.open(b)
    if ia.size != ib.size:
        return (float("inf"), 1.0, f"different sizes, {ia.size} and {ib.size}")
    side = max(ia.size)
    scale = min(1.0, MAX_SIDE / side)
    size = (max(1, round(ia.width * scale)), max(1, round(ia.height * scale)))
    pa, pb = reduced(a, size), reduced(b, size)
    total = 0.0
    outliers = 0
    for x, y in zip(pa, pb):
        if x == y:
            continue
        d = REPORT.ciede2000(REPORT._srgb_to_lab(x), REPORT._srgb_to_lab(y))
        total += d
        if d > OUTLIER_LIMIT:
            outliers += 1
    n = len(pa)
    return (total / n, outliers / n, f"{size[0]}x{size[1]} compared")


def pairs(first: pathlib.Path, again: pathlib.Path) -> list:
    """Every PNG the second report drew that the first drew too."""
    out = []
    for p in sorted(again.rglob("*.png")):
        other = first / p.relative_to(again)
        if other.is_file():
            out.append((other, p))
    return out


def main(first_path: str, again_path: str) -> int:
    first, again = pathlib.Path(first_path), pathlib.Path(again_path)
    found = pairs(first, again)
    if not found:
        print(f"no image in {again} has a counterpart in {first}")
        return 1
    bad = []
    worst = 0.0
    for a, b in found:
        mean, share, note = difference(a, b)
        worst = max(worst, mean)
        if mean > MEAN_LIMIT or share > OUTLIER_SHARE:
            bad.append(b)
            print(f"{b.relative_to(again)}: mean dE {mean:.2f}, {share * 100:.2f}% of pixels past {OUTLIER_LIMIT} ({note})")
    if bad:
        print(f"images a reader would see differ: {[str(p.relative_to(again)) for p in bad]}")
        return 1
    print(f"{len(found)} images redrawn indistinguishably, worst mean dE {worst:.2f} against a limit of {MEAN_LIMIT}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1], sys.argv[2]))
