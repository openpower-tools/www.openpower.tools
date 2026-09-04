"""Compare two --checks-json dumps of the interaction report: the controls
present in the second must carry exactly the checks of the first, name,
outcome and detail alike. Exit 1 with the first differences otherwise."""
import json
import sys


def by_tag(path: str) -> dict:
    return {c["tag"]: c["checks"] for c in json.load(open(path))}


def main(first_path: str, again_path: str) -> int:
    first, again = by_tag(first_path), by_tag(again_path)
    bad = []
    for tag, checks in again.items():
        if first.get(tag) == checks:
            continue
        bad.append(tag)
        for a, b in zip(first.get(tag, []), checks):
            if a != b:
                print(f"{tag}: first {a} | again {b}")
        if len(first.get(tag, [])) != len(checks):
            print(f"{tag}: {len(first.get(tag, []))} checks first, {len(checks)} again")
    if bad:
        print(f"non-deterministic controls: {bad}")
        return 1
    print("identical check tables for", sorted(again))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1], sys.argv[2]))
