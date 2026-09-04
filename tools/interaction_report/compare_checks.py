"""Compare two --checks-json dumps of the interaction report.

The synthetic clock's guarantee is that two runs take the same decisions and
measure the same quantities to within one frame, not that their text matches
byte for byte: a transition's last floating-point bit still lands either side
of a frame boundary now and then. So the check names, their outcomes and the
wording around each measurement must match exactly, while the numbers inside
a detail may differ by up to one frame (or one part in a thousand, whichever
is larger, for a big figure). Anything else is a regression.
"""
import json
import re
import sys

FRAME = 1.0 / 60.0
NUMBER = re.compile(r"-?\d+\.?\d*(?:e-?\d+)?")


def skeleton(detail: str) -> str:
    """The detail with every number replaced, so the wording can be compared."""
    return NUMBER.sub("#", detail)


def numbers(detail: str) -> list[str]:
    return [m.group() for m in NUMBER.finditer(detail)]


def tolerance(a: str, b: str) -> float:
    """How far two printings of one measurement may sit apart: one frame, plus
    the rounding of the coarser of the two, so 3.00 and 3.02 are one frame
    apart as printed. A number written without decimals is a count or a colour
    channel, and those must match exactly."""
    decimals = max(len(t.partition(".")[2]) for t in (a, b))
    if decimals == 0:
        return 0.0
    return FRAME + 10.0 ** -decimals


def detail_differs(a: str, b: str) -> str | None:
    if skeleton(a) != skeleton(b):
        return "wording"
    for x, y in zip(numbers(a), numbers(b)):
        span = abs(float(x) - float(y))
        if span > max(tolerance(x, y), abs(float(x)) / 1000.0):
            return f"{x} vs {y}, further apart than one frame"
    return None


def by_tag(path: str) -> dict:
    return {c["tag"]: c["checks"] for c in json.load(open(path))}


def main(first_path: str, again_path: str) -> int:
    first, again = by_tag(first_path), by_tag(again_path)
    bad = []
    for tag, checks in again.items():
        others = first.get(tag)
        if others is None:
            print(f"{tag}: absent from {first_path}")
            bad.append(tag)
            continue
        if [c["name"] for c in others] != [c["name"] for c in checks]:
            print(f"{tag}: different checks: {[c['name'] for c in others]} then {[c['name'] for c in checks]}")
            bad.append(tag)
            continue
        for a, b in zip(others, checks):
            if a["ok"] != b["ok"]:
                print(f"{tag}: {a['name']}: {a['ok']} then {b['ok']} ({a['detail']} | {b['detail']})")
                bad.append(tag)
            elif (why := detail_differs(a["detail"], b["detail"])):
                print(f"{tag}: {a['name']}: {why} ({a['detail']} | {b['detail']})")
                bad.append(tag)
    if bad:
        print(f"controls that did not reproduce: {sorted(set(bad))}")
        return 1
    total = sum(len(c) for c in again.values())
    print(f"{total} checks over {len(again)} controls reproduced: same outcomes, measurements within one frame")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1], sys.argv[2]))
