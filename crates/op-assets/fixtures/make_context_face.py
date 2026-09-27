# /// script
# requires-python = ">=3.11"
# dependencies = ["fonttools==4.60.1", "uharfbuzz==0.51.1"]
# ///
"""Build context-face.ttf and context-face.txt: a face with one rule of
every kind coverage.rs reads, and the text that shows each rule firing.

The served faces do not use every format: Iosevka's calt has no plain
contextual format 1, no chaining format 1, and Plex none of it. So a
reader that dropped one of those formats would pass every test built on
them. This face carries each, one rule apiece, each writing its own glyph:

    ctx1   contextual, format 1 (glyph sequence)      a b     a -> a1
    ctx2   contextual, format 2 (classes)             c d     d -> d1
    ctx3   contextual, format 3 (coverages)           e f     e -> e1
    chain1 chaining, format 1                         x [g] h g -> g1
    chain2 chaining, format 2, class 0 lookahead      y [i] * i -> i1
    chain3 chaining, format 3                         z [j] k j -> j1
    rev    reverse chaining                           n [m] o m -> m1
    step   a variant only a rule writes, needed by    s [p]   p -> p1
           a second rule, so its context must travel  p1 [r]  r -> r1
    liga   a ligature a rule then needs               u v -> uv, uv [w] w -> w1
    dlig   a rule under a feature no browser applies  q [t] t -> t1
    order  rules that read what only a later lookup   M1 [N]  N -> N1
           writes, which a shaper never lets them     OP [Q]  Q -> Q1
           see: M -> M1 before R, and the ligature
           O P -> OP, both come in lookups after them

Single substitutions come in both formats: p -> p1 alone is a constant
delta (format 1), d -> d1 with e -> e1 is not (format 2). The ligature
lookup also holds a one-glyph ligature (K -> k_lig) and one whose second
component no character maps to (l + hidden), neither of which is a
sequence text can type. GPOS carries four pairs that each move by one
value field only (A B x placement, C D y placement, E F x advance, G H y
advance) and one pair that moves nothing (I J), which is not a kern.

The rules are written with fontTools' feature compiler and then rebuilt
by hand in the format named, since the compiler picks formats itself.
context-face.txt lists, for each rule, the text that shows it and the
glyph it brings in, each confirmed with HarfBuzz under the default
features; the dlig rule is listed as not firing there.

Run from this directory: `uv run make_context_face.py`. The output is byte
identical from run to run (the timestamps are pinned).
"""
import io

import uharfbuzz as hb
from fontTools.feaLib.builder import addOpenTypeFeaturesFromString
from fontTools.fontBuilder import FontBuilder
from fontTools.otlLib import builder as otl
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib.tables import otTables as ot

LETTERS = list("abcdefghijklmnopqrstuvwxyz")
CAPITALS = list("ABCDEFGHIJKMNOPQR")
VARIANTS = ["a1", "d1", "g1", "i1", "j1", "m1", "p1", "r1", "w1", "t1", "uv", "k_lig", "l_hidden", "hidden",
            "M1", "N1", "Q1", "OP"]
# e1 sits apart from d1 so d -> d1, e -> e1 cannot share a delta
GLYPHS = [".notdef", "space"] + LETTERS + CAPITALS + VARIANTS + ["e1"]
CMAP = {0x20: "space", **{ord(c): c for c in LETTERS + CAPITALS}}
PINNED = 3660681600

FEATURES = """
languagesystem DFLT dflt;
lookup L_a { sub a by a1; } L_a;
lookup L_de { sub d by d1; sub e by e1; } L_de;
lookup L_g { sub g by g1; } L_g;
lookup L_i { sub i by i1; } L_i;
lookup L_j { sub j by j1; } L_j;
lookup L_p { sub p by p1; } L_p;
lookup L_r { sub r by r1; } L_r;
lookup L_w { sub w by w1; } L_w;
lookup L_t { sub t by t1; } L_t;
lookup L_M { sub M by M1; } L_M;
lookup L_N { sub N by N1; } L_N;
lookup L_Q { sub Q by Q1; } L_Q;
lookup LIGA { sub u v by uv; sub l hidden by l_hidden; } LIGA;
lookup CTX1 { sub a' lookup L_a b; } CTX1;
lookup CTX2 { sub c d' lookup L_de; } CTX2;
lookup CTX3 { sub e' lookup L_de f; } CTX3;
lookup CHAIN1 { sub x g' lookup L_g h; } CHAIN1;
lookup CHAIN2 { sub y i' lookup L_i; } CHAIN2;
lookup CHAIN3 { sub z j' lookup L_j k; } CHAIN3;
lookup STEP1 { sub s p' lookup L_p; } STEP1;
lookup STEP2 { sub p1 r' lookup L_r; } STEP2;
lookup AFTER_LIGA { sub uv w' lookup L_w; } AFTER_LIGA;
lookup DLIG { sub q t' lookup L_t; } DLIG;
lookup REV { rsub n m' o by m1; } REV;
lookup EARLY { sub M1 N' lookup L_N; } EARLY;
lookup EARLY2 { sub OP Q' lookup L_Q; } EARLY2;
lookup LATE { sub M' lookup L_M R; } LATE;
lookup LATE_LIGA { sub O P by OP; } LATE_LIGA;
feature liga { lookup LIGA; lookup LATE_LIGA; } liga;
feature calt {
    lookup CTX1; lookup CTX2; lookup CTX3;
    lookup CHAIN1; lookup CHAIN2; lookup CHAIN3;
    lookup STEP1; lookup STEP2; lookup AFTER_LIGA; lookup REV;
    lookup EARLY; lookup EARLY2; lookup LATE;
} calt;
feature dlig { lookup DLIG; } dlig;
feature kern {
    pos A B <10 0 0 0>;
    pos C D <0 10 0 0>;
    pos E F <0 0 10 0>;
    pos G H <0 0 0 10>;
    pos I J <0 0 0 0>;
} kern;
"""


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
fb.setupNameTable({"familyName": "Context Fixture", "styleName": "Regular"})
fb.setupOS2()
fb.setupPost()
fb.setupHead(created=PINNED, modified=PINNED)
addOpenTypeFeaturesFromString(fb.font, FEATURES)
font = fb.font
font.recalcTimestamp = False
glyph_map = font.getReverseGlyphMap()
gsub = font["GSUB"].table
lookups = gsub.LookupList.Lookup


def index_of(name):
    """The index of the lookup the feature file named `name`: feaLib keeps
    named lookups in the order they were declared."""
    order = ["L_a", "L_de", "L_g", "L_i", "L_j", "L_p", "L_r", "L_w", "L_t", "L_M", "L_N",
             "L_Q", "LIGA", "CTX1", "CTX2", "CTX3", "CHAIN1", "CHAIN2", "CHAIN3", "STEP1",
             "STEP2", "AFTER_LIGA", "DLIG", "REV", "EARLY", "EARLY2", "LATE", "LATE_LIGA"]
    return order.index(name)


def record(sequence_index, lookup_name):
    r = ot.SubstLookupRecord()
    r.SequenceIndex = sequence_index
    r.LookupListIndex = index_of(lookup_name)
    return r


def coverage(glyphs):
    return otl.buildCoverage(glyphs, glyph_map)


def class_def(classes):
    c = ot.ClassDef()
    c.classDefs = dict(classes)
    return c


def replace(name, lookup_type, subtable):
    lk = lookups[index_of(name)]
    lk.LookupType = lookup_type
    lk.SubTable = [subtable]
    lk.SubTableCount = 1


# ctx1: contextual format 1, a b, a -> a1
st = ot.ContextSubst()
st.Format = 1
st.Coverage = coverage(["a"])
rule = ot.SubRule()
rule.Input = ["b"]
rule.GlyphCount = 2
rule.SubstLookupRecord = [record(0, "L_a")]
rule.SubstCount = 1
rule_set = ot.SubRuleSet()
rule_set.SubRule = [rule]
rule_set.SubRuleCount = 1
st.SubRuleSet = [rule_set]
st.SubRuleSetCount = 1
replace("CTX1", 5, st)

# ctx2: contextual format 2, classes c = 1, d = 2; the coverage also holds
# b in class 3, which has no rule, so a reader must filter the coverage by
# class to find the first glyphs of the class-1 rule
st = ot.ContextSubst()
st.Format = 2
st.Coverage = coverage(["b", "c"])
st.ClassDef = class_def({"c": 1, "d": 2, "b": 3})
rule = ot.SubClassRule()
rule.Class = [2]
rule.GlyphCount = 2
rule.SubstLookupRecord = [record(1, "L_de")]
rule.SubstCount = 1
rule_set = ot.SubClassSet()
rule_set.SubClassRule = [rule]
rule_set.SubClassRuleCount = 1
st.SubClassSet = [None, rule_set, None, None]
st.SubClassSetCount = 4
replace("CTX2", 5, st)

# ctx3: contextual format 3, e f, e -> e1
st = ot.ContextSubst()
st.Format = 3
st.Coverage = [coverage(["e"]), coverage(["f"])]
st.GlyphCount = 2
st.SubstLookupRecord = [record(0, "L_de")]
st.SubstCount = 1
replace("CTX3", 5, st)

# chain1: chaining format 1, x [g] h, g -> g1
st = ot.ChainContextSubst()
st.Format = 1
st.Coverage = coverage(["g"])
rule = ot.ChainSubRule()
rule.Backtrack = ["x"]
rule.BacktrackGlyphCount = 1
rule.Input = []
rule.InputGlyphCount = 1
rule.LookAhead = ["h"]
rule.LookAheadGlyphCount = 1
rule.SubstLookupRecord = [record(0, "L_g")]
rule.SubstCount = 1
rule_set = ot.ChainSubRuleSet()
rule_set.ChainSubRule = [rule]
rule_set.ChainSubRuleCount = 1
st.ChainSubRuleSet = [rule_set]
st.ChainSubRuleSetCount = 1
replace("CHAIN1", 6, st)

# chain2: chaining format 2, backtrack class {y}, input class {i},
# lookahead class 0 (anything no class claims)
st = ot.ChainContextSubst()
st.Format = 2
st.Coverage = coverage(["i"])
st.BacktrackClassDef = class_def({"y": 1})
st.InputClassDef = class_def({"i": 1})
st.LookAheadClassDef = class_def({"y": 1})
rule = ot.ChainSubClassRule()
rule.Backtrack = [1]
rule.BacktrackGlyphCount = 1
rule.Input = []
rule.InputGlyphCount = 1
rule.LookAhead = [0]
rule.LookAheadGlyphCount = 1
rule.SubstLookupRecord = [record(0, "L_i")]
rule.SubstCount = 1
rule_set = ot.ChainSubClassSet()
rule_set.ChainSubClassRule = [rule]
rule_set.ChainSubClassRuleCount = 1
st.ChainSubClassSet = [None, rule_set]
st.ChainSubClassSetCount = 2
replace("CHAIN2", 6, st)

# chain3: chaining format 3, z [j] k, j -> j1
st = ot.ChainContextSubst()
st.Format = 3
st.BacktrackCoverage = [coverage(["z"])]
st.BacktrackGlyphCount = 1
st.InputCoverage = [coverage(["j"])]
st.InputGlyphCount = 1
st.LookAheadCoverage = [coverage(["k"])]
st.LookAheadGlyphCount = 1
st.SubstLookupRecord = [record(0, "L_j")]
st.SubstCount = 1
replace("CHAIN3", 6, st)

# the one-glyph ligature K -> k_lig, which the feature compiler will not
# write (it reads `sub K by k_lig` as a single substitution); K is used by
# nothing else, so the ligature cannot disturb another rule
liga = lookups[index_of("LIGA")].SubTable[0]
one = ot.Ligature()
one.LigGlyph = "k_lig"
one.Component = []
one.CompCount = 1
liga.ligatures.setdefault("K", []).append(one)

font.save("context-face.ttf")

# the witnesses, confirmed with HarfBuzz under the default features
WITNESSES = [
    ("ctx1", "ab", "a1", True),
    ("ctx2", "cd", "d1", True),
    ("ctx3", "ef", "e1", True),
    ("chain1", "xgh", "g1", True),
    ("chain2", "yia", "i1", True),
    ("chain3", "zjk", "j1", True),
    ("rev", "nmo", "m1", True),
    ("step", "spr", "r1", True),
    ("liga", "uvw", "w1", True),
    ("dlig", "qt", "t1", False),
    ("order", "MN", "N1", False),
    ("order-ligature", "OPQ", "Q1", False),
]
raw = io.BytesIO()
font.save(raw)
shaper = hb.Font(hb.Face(hb.Blob(raw.getvalue())))
with open("context-face.txt", "w", encoding="utf-8") as out:
    out.write(
        "# For each rule of context-face.ttf, the text that shows it and the\n"
        "# glyph it brings in, as HarfBuzz " + hb.version_string() + " shapes it with the default\n"
        "# features. Written by make_context_face.py; see there for the rules.\n"
        "#\n"
        "# Format: rule, text, glyph id, glyph name, and whether the default\n"
        "# features bring the glyph in (the dlig rule is not one of them, and\n"
        "# the order rules never fire, since what they read comes later).\n"
    )
    for rule_name, text, glyph, fires in WITNESSES:
        buf = hb.Buffer()
        buf.add_str(text)
        buf.guess_segment_properties()
        hb.shape(shaper, buf)
        gid = glyph_map[glyph]
        brought = gid in {g.codepoint for g in buf.glyph_infos}
        assert brought == fires, f"{rule_name}: {text!r} gives {[font.getGlyphName(g.codepoint) for g in buf.glyph_infos]}"
        out.write(f"{rule_name}\t{text}\t{gid}\t{glyph}\t{'fires' if fires else 'idle'}\n")
print("context-face.ttf and context-face.txt written")
