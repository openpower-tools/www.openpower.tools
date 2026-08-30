# Embedded fonts

Every webfont is compiled into the wasm binary and registered at runtime with
the CSS Font Loading API (`crates/op-site/src/fonts.rs`); nothing here is
served as a fetchable URL. All faces below are under the SIL Open Font License
1.1, with each licence file alongside the fonts.

- b612/: B612 and B612 Mono, converted to woff2 with fonttools from the TTFs
  at https://github.com/polarsys/b612 (master, last push 2020-02-01). Designed
  for Airbus cockpit displays by Intactile Design with ENAC.
- plex-sans/ (@ibm/plex-sans 1.1.0), plex-mono/ (@ibm/plex-mono 2.5.0),
  plex-serif/ (@ibm/plex-serif 2.0.0): woff2 taken unmodified from the
  official npm packages at https://github.com/IBM/plex.
- iosevka-ss08/: Iosevka SS08, the "PragmataPro Style" stylistic set of
  Iosevka, official webfont build from https://github.com/be5invis/Iosevka
  (v34.8.1), subsetted to the site's ranges. Public stand-in for PragmataPro,
  which is licensed for the site but kept out of the repository for now; the
  font stacks list the commercial family first so it renders where installed
  locally, and registration applies PragmataPro's measured vertical metrics.
- barlow-semi-condensed/: Barlow Semi Condensed from
  https://github.com/google/fonts (ofl/barlowsemicondensed, converted with
  fonttools). Public stand-in for Sys 2.0 on the same terms; registration
  scales it 107% with Sys 2.0's measured ascent and descent so width,
  x-height and cap height land within about 2.5% of the original.
