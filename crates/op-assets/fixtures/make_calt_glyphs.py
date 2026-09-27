# /// script
# requires-python = ">=3.11"
# dependencies = ["fonttools==4.60.1", "brotli==1.1.0", "uharfbuzz==0.51.1"]
# ///
"""Write calt-glyphs.txt: every glyph Iosevka's contextual alternates bring
in, as HarfBuzz shapes them, with the shortest string that shows each.

HarfBuzz is the shaper Chrome and Firefox use, and uharfbuzz binds the C
library itself, so this is independent of both the table reader under test
(read-fonts, via coverage.rs) and the Rust port the crate shapes with
(HarfRust). A browser cannot be the reference here the way it is for the
kern fixture: Iosevka is monospaced, so a contextual alternate changes a
glyph's drawing and never its width, and a width is all a page can measure.

The sweep shapes every string of one to four printable ASCII characters,
82,317,120 of them, twice: with the default features, and with calt
turned off. A glyph present in the first and absent from the second is one
calt brought in. Five characters would be 7.7 billion strings, which is out
of reach of a sweep and is the point of coverage.rs: its test holds the
sequences it derives from the rules to reaching every glyph listed here.

LONGER lists strings of five and six characters that bring in glyphs the
sweep cannot reach. coverage.rs's sequences found them; this script
confirms each with HarfBuzz and lists what it adds in a section of its own,
so that section is checked entry by entry, where the sweep is complete.

The strings are split among worker processes by their first character, and
each glyph keeps the shortest witness any worker found (the least, among
those of that length), so the output does not depend on how the work was
split. It is a few minutes' work on a machine with many cores and hours on
one core; it was run on atlas (144 threads).

Run from this directory: `uv run make_calt_glyphs.py`. The output depends
only on the face and the pinned versions above.
"""
import io
import itertools
import multiprocessing
import os

import uharfbuzz as hb
from fontTools.ttLib import TTFont

FACE = "../assets/iosevka-ss08/IosevkaSS08-Regular.woff2"
ALPHABET = [chr(c) for c in range(0x20, 0x7F)]
LONGEST = 4
# found by coverage.rs's sequences, each confirmed below; every one needs
# all of its characters, so none is within reach of the sweep
LONGER = [
    "+++++", "-----", "-<->>", "-<<->", "-<<->>", "<-->>", "<--->", "<<-->",
    "!====", "<===>", "<<~~>", "<~~~>", "<~~~>>",
]

font_file = TTFont(FACE)
font_file.flavor = None
raw = io.BytesIO()
font_file.save(raw)
font = hb.Font(hb.Face(hb.Blob(raw.getvalue())))
names = font_file.getGlyphOrder()
OFF = {"calt": False}


def glyphs(text, features=None):
    buf = hb.Buffer()
    buf.add_str(text)
    buf.guess_segment_properties()
    hb.shape(font, buf, features)
    return [g.codepoint for g in buf.glyph_infos]


def sweep(first):
    """Every glyph calt brings into the strings that start with `first`,
    each with its shortest and least witness among them."""
    found = {}
    for length in range(1, LONGEST + 1):
        for rest in itertools.product(ALPHABET, repeat=length - 1):
            text = first + "".join(rest)
            brought = set(glyphs(text)) - set(glyphs(text, OFF))
            for gid in brought:
                # strings come in length order, then character order, so the
                # first witness is the shortest and the least
                found.setdefault(gid, text)
    return found


def write(found, longer):
    with open("calt-glyphs.txt", "w", encoding="utf-8") as out:
        out.write(
            "# Every glyph Iosevka SS08's contextual alternates (calt) bring in,\n"
            "# as HarfBuzz " + hb.version_string() + " shapes it, over every string of one to four\n"
            "# printable ASCII characters. Written by make_calt_glyphs.py; see there\n"
            "# for why HarfBuzz and not a browser.\n"
            "#\n"
            "# Format: glyph id, a tab, glyph name, a tab, the shortest string that\n"
            "# brings it in, written between vertical bars so a leading or trailing\n"
            "# space survives.\n"
        )
        for gid in sorted(found):
            out.write(f"{gid}\t{names[gid]}\t|{found[gid]}|\n")
        out.write(
            "#\n"
            "# Beyond the sweep: glyphs brought in only by the longer strings in\n"
            "# make_calt_glyphs.py's LONGER, each confirmed by HarfBuzz there.\n"
        )
        for gid in sorted(longer):
            out.write(f"{gid}\t{names[gid]}\t|{longer[gid]}|\n")
    print(f"{len(found)} glyphs in the sweep, {len(longer)} beyond it")


if __name__ == "__main__":
    found = {}
    with multiprocessing.Pool(min(len(ALPHABET), os.cpu_count() or 1)) as pool:
        for part in pool.imap_unordered(sweep, ALPHABET):
            for gid, text in part.items():
                if gid not in found or (len(text), text) < (len(found[gid]), found[gid]):
                    found[gid] = text
    longer = {}
    for text in LONGER:
        new = (set(glyphs(text)) - set(glyphs(text, OFF))) - set(found)
        assert new, f"{text!r} brings in nothing the sweep did not find"
        for gid in new:
            if gid not in longer or (len(text), text) < (len(longer[gid]), longer[gid]):
                longer[gid] = text
    write(found, longer)
