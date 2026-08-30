# Webfonts

These files are packed at build time into a single content-hashed container
(`crates/op-assets`, listed in `crates/op-fontpack`'s manifest) that the site
fetches progressively and registers through the CSS Font Loading API
(`crates/op-site/src/fonts.rs`); nothing here is served as an individual
fetchable font URL. All faces below are under the SIL Open Font License
1.1, with each licence file alongside the fonts.

- plex-sans/ (@ibm/plex-sans 1.1.0): woff2 taken unmodified from the
  official npm package at https://github.com/IBM/plex.
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
