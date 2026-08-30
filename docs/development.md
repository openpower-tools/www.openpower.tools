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
cargo fmt --all --check
cargo clippy --workspace --all-targets --target wasm32-unknown-unknown -- -D warnings
cargo test --workspace # native tests, including the palette contrast checks
```

Warnings are errors (`.cargo/config.toml`).

## Layout

```
index.html                     the page: custom elements plus light-DOM content
styles/theme.css               palette tokens (light, dark, data-theme override)
crates/op-webc/                Web Component bridge (CustomElement trait, shim)
crates/op-site/src/main.rs     registers the elements when the module starts
crates/op-site/src/components  one file per element (op-theme-toggle, ...)
crates/op-site/src/theme.rs    Auto/Light/Dark selection and persistence
crates/op-site/src/palette.rs  WCAG contrast tests over styles/theme.css
Trunk.toml                     build settings and pinned tool versions
docs/research/                 research notes with sources
```

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
