# Measuring text without a browser: survey and decisions

Written 2026-09-05, after Phase 5 of the chart element shipped its advance
tables. Decision 14 of `chart-element-2026-09.md` chose build-time advance
tables over shaping, on the reasoning that the chart's text is digits,
units and short names, where kerning barely applies. That reasoning was
sound but untested. This is what testing it found, and what the next
phase should be built on.

Everything below is either a measurement taken here, with the command
that produced it, or a sourced claim with its URL. Where a source could
not be verified, or a measurement was taken against a different version
than the claim is about, it says so.

## What was measured

**The tables are exact.** A De Bruijn sequence of order 2 over the 95
printable ASCII characters holds every ordered pair once in 9026
characters. Laid out as one SVG text node per face and read with
`getStartPositionOfChar`, it yields all 9025 kerns from one layout. With
kerning, ligatures and contextual alternates disabled in CSS, the browser
laid every step out at the table's own advance to within 0.00775 px,
inside the sixty-fourth of a pixel the engine rounds to. Zero steps
strayed past a sixty-fourth, in either weight.

**Kerning tightens far more than it loosens**, so the summed advances are
always the wider number. Over the 13 strings the site's charts actually
draw, the worst error is 0.296 px. A label placed by the sum is reserved
generously, never crowded. This is the property the placement rests on,
and it is a property of these faces rather than of fonts in general.

**A shaper reproduces the browser.** rustybuzz 0.20 shaping all 9025
pairs agreed with Chrome to 0.0074 px on the 9024 that shape to two
glyphs. The one that ligates, `fi`, agrees on the whole run's width to
0.0007 px; only the per-character split differs, and that split is an
engine convention for distributing one advance rather than a measurement.

**Hand-rolling the table read is a trap.** Extracting GPOS pair
positioning by expanding the class subtables and letting later matches
win mispredicted four pairs, all `j` before a capital, giving `jY` a kern
of -0.72 px where the browser applied -0.24. OpenType applies the first
matching subtable within a lookup. With that rule the font's own tables
reproduce Chrome exactly: same 1227 and 1244 kerned pairs, worst
disagreement 0.0074 px.

**Headless Chrome has two font paths and one destroys the measurement.**
`--font-render-hinting` defaults to full in headless and quantises the
advance `getComputedTextLength` returns to a whole pixel, not merely the
rasterisation. "office" in Plex Sans 400 at 12 px measures 31.00 px on
chrome-headless-shell's default path against 29.898 under `--headless=new`
at a device pixel ratio of 2, and 29.892 from the shaper.
`--font-render-hinting=none` or `--disable-font-subpixel-positioning`
recovers it. Skia's `isSubpixel()` flag is the mechanism: it gates
whether an advance comes back as a float or rounded to a whole pixel.

Sources for the last point: <https://issues.chromium.org/issues/40530343>,
<https://skia.googlesource.com/skia.git/+/HEAD/modules/skshaper/src/SkShaper_harfbuzz.cpp>.

## Which engines should agree, and which cannot

Blink uses HarfBuzz on every platform, with no per-platform shaper swap
(`harfbuzz_shaper.cc`). But it does not read advances through HarfBuzz's
own table reader: the base advance comes from Skia's per-platform
typeface via `SkFontGetGlyphWidthForHarfBuzz`, and on Apple platforms
Chromium specifically detects an AAT `trak` table and switches to
HarfBuzz-native advances to avoid CoreText's automatic tracking
(`harfbuzz_face.cc`). Shaping runs at 16.16 fixed precision; layout snaps
to LayoutUnit's 1/64 px, which is the grid measured above.

Firefox uses HarfBuzz everywhere, and `gfxHarfBuzzShaper` reads the
font's own `hmtx` unhinted whenever the platform does not override glyph
widths. On macOS it only overrides for variation or `sbix` fonts; on
Windows it does not override in the default subpixel mode either. On
Linux and Android `gfxFT2FontBase` overrides unconditionally and uses
FreeType's hinted advance, which shifts advances rather than quantising
them.

WebKit's `ComplexTextController` is CoreText on macOS and iOS and
HarfBuzz on the GTK and WPE ports. Playwright's macOS WebKit build runs
the real Cocoa and CoreText port; its Linux and Windows builds do not, so
only the macOS one is evidence about Safari.

So, against a table generated with HarfBuzz semantics:

- **Exact**, reading the same `hmtx`: Firefox on macOS, and on Windows in
  its default mode, for static non-`sbix` faces.
- **Equal to a rounding grid**: Chrome and Edge, grid 1/64 px.
- **Equal to a grid of unestablished size**: WebKit GTK and WPE. Their
  FreeType hinting default for advances was not verified.
- **Genuinely different**: Firefox on Linux and Android, because FreeType
  hints the advance. Safari where a face carries `trak`, because CoreText
  applies size-dependent tracking no table reproduces. Any variable font,
  unless the reader is HVAR-aware.

**None of the divergence cases arise for the faces this site serves.**
All twelve, IBM Plex Sans, Iosevka SS08 and Barlow Semi Condensed, are
static and carry none of `trak`, `HVAR`, `fvar`, `gvar`, `sbix`, `morx`
or `kerx`. The one genuine divergence we actually serve into is Firefox
on Linux and Android.

Sources:
<https://chromium.googlesource.com/chromium/src/third_party/+/master/blink/renderer/platform/fonts/shaping/harfbuzz_face.cc>,
<https://github.com/mozilla-firefox/firefox/blob/main/gfx/thebes/gfxFT2FontBase.cpp>,
<https://searchfox.org/mozilla-central/source/gfx/thebes/gfxHarfBuzzShaper.cpp>,
<https://trac.webkit.org/wiki/ComplexTextController>,
<https://developer.apple.com/fonts/TrueType-Reference-Manual/RM06/Chap6trak.html>,
<https://developer.chrome.com/blog/memory-safety-fonts>.

## Coding fonts, which is where this gets hard

Iosevka SS08 is in this repository and is the local stress case: 3206
glyphs over only 3 distinct advance widths, more than 30 GSUB features
including per-language ligature sets, 188 chaining contextual lookups, 20
reverse chaining lookups, and a longest rule context of 5 glyphs.

It turns out to be the easy case for measurement, for a reason worth
naming. Its arrows are not many-to-one ligatures. Every ligature subtable
tops out at 2 characters and the long forms are built by contextual
single substitution, one piece per character, so glyph count survives and
the cell width is untouched. Measured: `->`, `===`, `<=>` and `<<=` are
each exactly n cells wide, with `calt`, `liga`, `dlig` and `clig` on or
off.

That gives three tiers:

- **Proportional text**, like Plex Sans: an order-2 sweep is exhaustive,
  one ligature aside.
- **Monospaced coding**, like Iosevka and PragmataPro: the invariant that
  a string of n characters is n cells wide is stronger and cheaper than
  any sweep, and is the property the font exists to guarantee.
- **Proportional coding**, like Sys: neither saves you. Many-to-one
  ligatures collapse the per-character positions and there is no width
  invariant, so whole-run comparison against rule-derived sequences is
  the only sound instrument.

The coverage consequence is decisive. An order-5 De Bruijn sequence over
95 symbols is 7.7 billion characters, so brute force is dead on arrival
for coding fonts, while Iosevka has only 216 chaining rules. **Derive the
test set from what the font's own lookups can reach; keep De Bruijn for
the pair layer.**

## Decisions for the next phase

1. **Shape with HarfRust, not rustybuzz.** rustybuzz is archived: its
   README says so and commit `9faca967` (2026-07-26) is "Deprecate in
   preference to HarfRust". HarfRust is maintained by the HarfBuzz
   organisation, tracks HarfBuzz 14.3.1, and HarfBuzz upstream itself
   carries an optional dependency on it behind a meson option. Run
   through HarfBuzz's own meson suite it passes aots 800/800 and
   text-rendering-tests 437/437. rustybuzz's measured agreement with
   Chrome above still stands as evidence that the approach works; it is
   the maintenance status, not the numbers, that decides this.

2. **Derive coverage with read-fonts, which HarfRust already brings.** It
   is the only one of the four candidates that enumerates PairPos in both
   directions, and `closure_lookups` (which lookups a glyph set can fire)
   is public there and private in HarfBuzz. `ClassDef::intersect_classes`
   and `intersected_class_glyphs` collapse PairPos Format 2 from the
   square of the glyph count to the product of the class counts.
   ttf-parser cannot enumerate either direction, which is what forced the
   probe-based extraction used in the survey above.

3. **Never ask HarfBuzz for a pair kern directly.**
   `hb_font_get_glyph_h_kerning` is public and undeprecated and returns
   zero while the same `hb_font_t` applies real kerning through shaping.
   It is a silent zero, not an error, which is the worst kind of thing to
   put in a validation harness. Measured against libharfbuzz 2.7.4; the
   bridge to 14.4.0 is source-level (`hb-ot-font.cc` has zero `kern`
   occurrences at both), not a direct run at 14.4.0.

4. **Key a cached table on the font file's content hash plus the feature
   configuration.** Version metadata is provably unreliable: the
   OpenType spec notes Windows evaluates the name table's version string
   rather than `head.fontRevision`, the two drift across four independent
   locations (fontbakery issue 601), and fontTools' `varLib.instancer`
   never bumps either when producing a static instance, even with
   `--update-name-table`. Both Plex weights here report the same
   `Version 3.005` and the same `fontRevision`, so a version key would
   collide across weights of one family. `op-assets` already computes a
   sha256.

5. **Refuse a face the tables cannot stand for.** The generator should
   fail rather than emit a table for a face carrying `trak` or `fvar`,
   since those are exactly the cases where an engine's advances diverge
   from what a table can say.

6. **Validate in the browser, generate from the font.** The sweep's job
   is to catch the case where the shaper and the engine disagree, which
   is precisely the class of bug the hand-rolled extraction hit. Run it
   on a path that does not quantise.

## On the De Bruijn construction

No prior art was found applying De Bruijn sequences to font or kerning
testing. That was checked by web search across many query variants, by
authenticated GitHub code and issue search against `harfbuzz/harfbuzz`
and `fonttools/fonttools` (zero hits, with a positive control confirming
the search worked), and by looking at what type-design QA tools actually
do: Hoefler's proofing method, Pangram Pangram's Kern King, Stringmaker,
KernCrasher, and behdad's own halfkern all use hand curation, Cartesian
products, or image overlap, never a minimal covering sequence.

This is not a claim of novelty in any strong sense. De Bruijn sequences
are long established in combinatorics on words, genome assembly and
elsewhere, and are the minimal-length special case of covering sequences
(arXiv 2404.13674). The standard software-testing framing of the same
idea is NIST's covering arrays and pairwise testing. Order 2 over an
alphabet is an Eulerian circuit on the complete directed graph over that
alphabet. All that is standard; only the application here appears not to
be written down, and absence of evidence in these searches is not proof
that nobody has done it.

## What is still open

- The rounding grid WebKit GTK and WPE use for advances, and whether
  their FreeType integration hints them the way Firefox's does.
- The size of CoreText's rounding grid, never established.
- Whether HarfRust's API is ergonomic for this in practice. Its own CI
  covers Ubuntu and macOS only, so its ppc64le and Windows support rests
  on the pure-Rust dependency graph and Rust's Tier 2 status for
  `powerpc64le-unknown-linux-gnu`, not on that project's own testing.
