# Development

Everything is Rust. The page in `index.html` is composed of custom elements
whose behaviour lives in `crates/op-site`; `crates/op-webc` is the small bridge
that registers Rust types as Web Components (it carries the project's only
JavaScript, a shim emitted by wasm-bindgen). [Trunk](https://trunkrs.dev/)
builds the wasm, runs wasm-bindgen and wasm-opt, and assembles `dist/`.

## Prerequisites

- rustup: `rust-toolchain.toml` pins a dated nightly with the wasm32 target,
  rustfmt and clippy; `rustup toolchain install` fetches it.
- Trunk 0.21.14: prebuilt binaries with checksums are on the
  [releases page](https://github.com/trunk-rs/trunk/releases); the workflow
  shows the verified install. Trunk downloads the wasm-bindgen and wasm-opt
  versions pinned in `Trunk.toml` on first use.

## Commands

```
trunk serve            # dev server on http://127.0.0.1:8080 with rebuild on change
trunk build --release  # production build into dist/
trunk build --release --config Trunk.specimen.toml   # then the specimen into dist/specimen/
trunk serve --release --config Trunk.specimen.toml   # specimen dev server on http://127.0.0.1:8081/specimen/
cargo fmt --all --check
cargo clippy --workspace --all-targets --target wasm32-unknown-unknown -- -D warnings
cargo test --workspace # native tests, including the palette contrast checks
```

Warnings are errors (`.cargo/config.toml`).

## Layout

```
index.html                     the page: custom elements plus light-DOM content
specimen/index.html            palette specimen page, published at /specimen/
styles/theme.css               palette tokens (dark default, light, auto via data-theme)
crates/op-webc/                Web Component bridge (CustomElement trait, shim)
crates/op-site/src/main.rs     registers the elements when the module starts
crates/op-site/src/components  one file per element (op-theme-toggle, ...)
crates/op-site/src/theme.rs    Dark/Light/Auto selection and persistence
crates/op-site/src/colour.rs   sRGB luminance and WCAG contrast
crates/op-site/src/palette.rs  WCAG contrast tests over styles/theme.css
Trunk.toml                     build settings and pinned tool versions
Trunk.specimen.toml            same, for the specimen page (second Trunk target)
docs/research/                 research notes with sources
```

## Fonts

The wasm binary carries no font bytes: a Trunk post_build hook
(`crates/op-assets`) packs the woff2 files into a single content-hashed
container advertised by `<meta name="op-fonts">`, and the wasm fetches it
after first paint through the Cache Storage API, decodes it
(`crates/op-fontpack`) and registers every face with the CSS Font Loading API
(`crates/op-site/src/fonts.rs`). Until it arrives, metric-fitted `local()`
fallback faces keep layout stable, and the registration runs inside a view
transition where supported, so the change cross-fades instead of popping (and
is instant under prefers-reduced-motion). The fallback stylesheet is generated
by the same hook, which computes every size-adjust and box override from the
actual font tables; targets for the licensed originals are recorded constants,
since those files are not in the repository. There are no fetchable font URLs.
Only the chosen faces are embedded, all SIL
OFL (IBM Plex Sans, Iosevka SS08, Barlow Semi Condensed; provenance and
licences in `crates/op-site/assets/fonts/README.md`), and every font token
ends in a curated system tail for browsers where that path never runs. The licensed commercial faces
(PragmataPro, Sys 2.0) are kept out of the repository and deliberately out of
the font stacks; Iosevka SS08 and Barlow Semi Condensed, metric-fitted to
them, are what everyone sees. Candidate stacks are compared on
`/specimen/`; the site default is the `--op-font-*` tokens in
`styles/theme.css`.

## Palette

Dark (the default) is derived from the Worcester colours and light from the
Nottingham colours; the derivation, roles and contrast requirements are
documented at the top of `styles/theme.css` and enforced by `cargo test`.
`/specimen/` renders every token with its value and contrast ratios, and the
site's elements, in both themes side by side.

## Adding an element

1. Add `components/<name>.rs` with a struct implementing
   `op_webc::CustomElement` and a `DEFINITION` (tag, observed attributes,
   constructor).
2. List it in `components::DEFINITIONS` and use the tag in `index.html`; the
   tests check that every defined tag is used and every `op-` tag is defined.
3. Render into the shadow root with `shadow_root(&host)`; escape any text that
   comes from attributes with `html::escape`.

The same forwarding pattern the shim uses for custom elements (a JS object
whose methods call into a wasm-bindgen-exported struct) is how an A-Frame
component registration would be wired if 3D scenes are added later.

## Reproducible builds

`dist/` is meant to be byte-identical wherever it is built: compiler and
dependencies are pinned (`rust-toolchain.toml`, `Cargo.lock`), the release
profile uses nightly `trim-paths`, and Trunk pins wasm-bindgen and wasm-opt.
Each CI run prints `sha256sum` of every file in `dist/` in its job summary;
compare with `find dist -type f | sort | xargs sha256sum` locally.

## Deployment and DNS

Pages is published by the workflow (`build_type: workflow`) with custom domain
`www.openpower.tools`; the apex redirects. DNS at Namecheap: `www` CNAME to
`openpower-tools.github.io`, the four GitHub Pages A and four AAAA records at
the apex, and the `_github-pages-challenge-openpower-tools` TXT record that
verifies the domain for the organisation (keep it).
