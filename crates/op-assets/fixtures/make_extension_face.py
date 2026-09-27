# /// script
# requires-python = ">=3.11"
# dependencies = ["fonttools==4.60.1"]
# ///
"""Build extension-face.ttf: a face whose layout rules all sit behind
extension lookups, for coverage.rs to be tested against.

Large fonts move their subtables behind extension lookups (GPOS type 9,
GSUB type 7) because a plain lookup's 16-bit offsets cannot reach far
enough. None of the faces this crate serves does that for a rule
coverage.rs reads, so without this face the unwrapping would go untested.
It carries one rule of each kind the module reads, each wrapped:

    pos A V -80        pair positioning, format 1 (a listed pair)
    pos [V] [A] -60    pair positioning, format 2 (a class pair)
    sub f i by f_i     ligature substitution

Written by fontTools' feature compiler, an implementation independent of
the reader under test. Rerun with `uv run make_extension_face.py` from this
directory; the output is byte identical from run to run (the timestamps
are pinned).
"""
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.feaLib.builder import addOpenTypeFeaturesFromString

GLYPHS = [".notdef", "space", "A", "V", "f", "i", "f_i"]
CMAP = {0x20: "space", 0x41: "A", 0x56: "V", 0x66: "f", 0x69: "i"}
FEATURES = """
languagesystem DFLT dflt;
lookup KERN_LISTED useExtension { pos A V -80; } KERN_LISTED;
lookup KERN_CLASSES useExtension { pos [V] [A] -60; } KERN_CLASSES;
lookup LIGA useExtension { sub f i by f_i; } LIGA;
feature kern { lookup KERN_LISTED; lookup KERN_CLASSES; } kern;
feature liga { lookup LIGA; } liga;
"""
# 2020-01-01T00:00:00Z in the Mac epoch the head table counts from
PINNED = 3660681600


def box():
    pen = TTGlyphPen(None)
    pen.moveTo((50, 0))
    pen.lineTo((50, 700))
    pen.lineTo((450, 700))
    pen.lineTo((450, 0))
    pen.closePath()
    return pen.glyph()


fb = FontBuilder(1000, isTTF=True)
fb.setupGlyphOrder(GLYPHS)
fb.setupCharacterMap(CMAP)
fb.setupGlyf({g: (TTGlyphPen(None).glyph() if g == "space" else box()) for g in GLYPHS})
fb.setupHorizontalMetrics({g: (500, 50) for g in GLYPHS})
fb.setupHorizontalHeader(ascent=800, descent=-200)
fb.setupNameTable({"familyName": "Extension Fixture", "styleName": "Regular"})
fb.setupOS2()
fb.setupPost()
fb.setupHead(created=PINNED, modified=PINNED)
addOpenTypeFeaturesFromString(fb.font, FEATURES)
fb.font.recalcTimestamp = False
fb.save("extension-face.ttf")
