# Trunk and Rust web-component notes (verified 2026-08-31)

All claims below were checked against the cited primary sources on 2026-08-31.
This covers the all-Rust alternative to the Vite/wasm-pack stack in
`stack-verification.md`: Trunk + Leptos CSR, deployed to GitHub Pages at the
root of `https://www.openpower.tools`. Contradicted assumptions are flagged
**PREMISE** inline.

## 0. Trunk's documentation domain is gone

**PREMISE** (`trunkrs.dev` is Trunk's site): no longer true, and actively
hostile.

- `https://trunkrs.dev/configuration/` returns Azerbaijani online-casino spam.
  A direct `curl https://trunkrs.dev/` returns **HTTP 403** behind a Cloudflare
  challenge. The domain has lapsed and is squatted.
- Canonical docs are now <https://trunk-rs.github.io/trunk/guide/> (HTTP 200),
  built as an mdbook from `guide/` in the repo.
  Source: <https://raw.githubusercontent.com/trunk-rs/trunk/main/README.md>
- Confirmed by the deploy workflow: `cname: trunkrs.dev` at tag `v0.21.14`
  versus `cname: trunk-rs.github.io/trunk` on `main`.
  <https://raw.githubusercontent.com/trunk-rs/trunk/main/.github/workflows/pages.yaml>
- The in-repo `site/content/*.md` pages are now redirect stubs
  (`{{ redirect_to_guide() }}`) pointing at the new guide.
- The Leptos book also links to <https://trunk-rs.github.io/trunk/>.

Do not link `trunkrs.dev` anywhere, and treat any cached copy of it as suspect.

## 1. Trunk: version, install, configuration, cache, warnings

### Version

- Latest **stable**: **0.21.14**, published 2025-05-08, MSRV 1.81.0, edition 2021.
  <https://crates.io/api/v1/crates/trunk/0.21.14>
- Stable-frozen for ~16 months. The only newer releases are prereleases:
  `0.22.0-beta.1` (2026-03-10) and `0.22.0-beta.2` (2026-07-24, MSRV 1.96.1,
  edition 2024). <https://api.github.com/repos/trunk-rs/trunk/releases>
- Release assets cover `{x86_64,aarch64}` x
  `{apple-darwin,pc-windows-msvc,unknown-linux-gnu,unknown-linux-musl}`,
  each with a `.sha256`.

### Install

From <https://raw.githubusercontent.com/trunk-rs/trunk/main/guide/src/getting-started/installation.md>:

```
cargo install --locked trunk     # build from crates.io
cargo binstall trunk             # prebuilt binary; explicitly supported
brew install trunk
sudo dnf install trunk           # Fedora 40+
nix-env -i trunk
```

Plus direct download from the GitHub releases page.

### CI actions

There is **no official Trunk GitHub Action** — the `trunk-rs` org contains only
the `trunk` repo. Options actually in use:

- `taiki-e/install-action` **v2.87.2** (2026-08-30). Its `TOOLS.md` lists both
  `trunk` and `wasm-bindgen`, installed from GitHub Releases into
  `$CARGO_HOME/bin`. <https://raw.githubusercontent.com/taiki-e/install-action/main/TOOLS.md>
- `jetli/trunk-action` **v0.5.1**, last pushed 2025-07-18, 37 stars. Its
  `action.yml` takes a `version` input defaulting to `latest`.
- Trunk's own docs workflow bootstraps via the `cargo-binstall` install script.

### (a) `[tools]` version pinning

Keys are snake_case; values are plain strings.

```toml
[tools]
sass = "1.69.5"           # dart-sass
wasm_bindgen = "0.2.127"  # bare semver, matches the wasm-bindgen release tag
wasm_opt = "version_132"  # binaryen release tag, literally "version_NNN"
tailwindcss = "3.3.5"
```

- The values in the docs are Trunk's *defaults*: `sass 1.69.5`,
  `wasm_bindgen 0.2.89`, `wasm_opt version_123`, `tailwindcss 3.3.5`
  (`src/tools.rs::default_version` at v0.21.14).
- Each has an env var (`TRUNK_TOOLS_SASS`, `TRUNK_TOOLS_WASM_BINDGEN`,
  `TRUNK_TOOLS_WASM_OPT`, `TRUNK_TOOLS_TAILWINDCSS`) and a CLI flag
  (`--wasm-bindgen`, `--wasm-opt`, ...). `src/config/models/tools.rs`.
- `wasm_opt = "version_132"` is valid: binaryen `version_132` released
  2026-08-12. <https://api.github.com/repos/WebAssembly/binaryen/releases>
  Trunk fetches
  `https://github.com/WebAssembly/binaryen/releases/download/{version}/binaryen-{version}-{arch}-{os}.tar.gz`.
- `wasm_bindgen = "0.2.127"` is valid: wasm-bindgen 0.2.127 published
  2026-08-08. <https://crates.io/api/v1/crates/wasm-bindgen>
- **If `wasm_bindgen` is unset**, Trunk resolves it from `Trunk.toml` ->
  `Cargo.lock` -> `Cargo.toml` -> hardcoded default
  (`find_wasm_bindgen_version`, `src/pipelines/rust/wasm_bindgen.rs`). Our
  `Cargo.lock` already pins 0.2.127, so it would pick that up automatically.
  Pinning explicitly is still clearer and offline-safe.
- **ppc64le caveat:** `Application::url()` `bail!`s with "unsupported target
  architecture" for anything other than x86_64/aarch64. Trunk cannot
  auto-download wasm-bindgen or wasm-opt on POWER; those tools must already be
  on `PATH` at the exact pinned version.

Guide source:
<https://raw.githubusercontent.com/trunk-rs/trunk/main/guide/src/configuration/index.md>

### (b) Release wasm-opt level

Settable **only per-asset in `index.html`**, not in `Trunk.toml`:

```html
<link data-trunk rel="rust" data-wasm-opt="z" />
```

- Accepted values: `0 1 2 3 4 s z` or empty (wasm-opt default). `0` disables
  the step entirely. Release-mode only. `src/pipelines/rust/wasm_opt.rs`.
- `data-wasm-opt-params` passes extra flags.
- There is **no** `[build]` key for the level. `[tools] wasm_opt` selects the
  *binary version*, not the optimisation level.

<https://raw.githubusercontent.com/trunk-rs/trunk/main/guide/src/assets/index.md>

### (c)-(e) `public_url`, `dist`, `filehash`

`[build]` defaults: `target = "index.html"`, `dist = "dist"`,
`public_url = "/"`, `filehash = true`, `minify = "never"`,
`inject_scripts = true`, `no_sri = false`.

**PREMISE** (a Pages site needs `--public-url <repo>`): only for *project*
pages at `username.github.io/repo_name`. For a **root custom domain** the
default `public_url = "/"` is correct — do not copy the Leptos book's
`--public-url "${GITHUB_REPOSITORY#*/}"`.

Fully annotated reference (every field with its default):
<https://github.com/trunk-rs/trunk/blob/v0.21.14/Trunk.toml>

`filehash = true` hashes pipeline outputs for cache control. `copy-file` and
`copy-dir` are the exception: never hashed, never minified.

### (f) CSS and static assets

- `<link data-trunk rel="css" href="styles/app.css" />` — copied verbatim,
  **hashed**, emitted as `<link rel="stylesheet">`. Options: `data-integrity`,
  `data-no-minify`, `data-target-path`.
- `<link data-trunk rel="copy-file" href="path/to/file" />` — copied exactly,
  **not hashed**.
- `<link data-trunk rel="copy-dir" href="path/to/dir" />` — recursive copy,
  **not hashed**.
- Also `rel="inline"` (inlines content; `type` inferred from extension, or one
  of `html|svg|css|js|mjs|module`) and `rel="icon"` (hashed).
- wasm-bindgen JS snippets (`#[wasm_bindgen(module = "/src/foo.js")]`) are
  copied and hashed automatically with **no config** — relevant to the custom
  element shim in section 3.

### (g) cargo invocation and rust-toolchain.toml

Yes on both counts. `src/pipelines/rust/mod.rs::cargo_build` builds:

```
cargo build --target=wasm32-unknown-unknown --manifest-path <path> [--release]
```

then appends `--profile <p>` if `data-cargo-profile*` is set (instead of
`--release`), plus `--offline` / `--frozen` / `--locked` / `--bin` /
`--example` / `--features` as configured. It shells out to plain `cargo` on
`PATH` via `common::run_command`, so:

- the rustup shim reads `rust-toolchain.toml` normally (our pinned
  `nightly-2026-08-30` and its `wasm32-unknown-unknown` target apply);
- `.cargo/config.toml` `rustflags` apply, including our
  `rustflags = ["-D", "warnings"]`;
- `cargo-features = ["trim-paths"]` and the `trim-paths = "all"` profile key
  work unchanged.

### (h) Tool cache location

`cache_dir()` = `directories::ProjectDirs::from("dev", "trunkrs", "trunk").cache_dir()`
(`src/tools.rs`), with `directories = "6"`. Per
<https://docs.rs/directories/latest/directories/struct.ProjectDirs.html>:

| Platform | Path |
| --- | --- |
| Linux | `$XDG_CACHE_HOME/trunk` -> `~/.cache/trunk` |
| macOS | `~/Library/Caches/dev.trunkrs.trunk` |
| Windows | `%LOCALAPPDATA%\trunkrs\trunk\cache` |

Tools land in `<cache>/{name}-{version}/`. There is **no environment override**.
Cache it in CI with `actions/cache` on `~/.cache/trunk`, keyed on the pinned
tool versions.

### (i) Inline `<script>` passthrough

Trunk rewrites **only** `link[data-trunk], script[data-trunk]`
(`src/pipelines/html.rs`). Everything else — including an inline
`<script>...</script>` or `<script type="module">` — passes through untouched.

**One hard trap:** `Document::new` scans *all* `<script>` tags and a
self-closing `<script ... />` is a **fatal error**, not a warning:

> "Self-closing script tag found. Replace the self-closing script tag
> (`<script .../>`) with a normally closed one such as
> `<script ...></script>`."

`--allow-self-closing-script` downgrades it to a warning. See
<https://github.com/trunk-rs/trunk/discussions/771>.

### (j) Warnings on a clean release build

Full `warn!` inventory in v0.21.14; the reachable ones for us:

- deprecated config fields: `clean.dist`, top-level `proxy` fields,
  `serve.address`;
- "Cargo profile from configuration (X) will be overridden with HTML file's
  more specific setting";
- "no rust project found" — emitted when there is no `rel="rust"` link and no
  sibling `Cargo.toml`;
- self-closing script (only when explicitly allowed);
- tool-archive extraction misses on version drift;
- "Accept Invalid Certificates is set to true".

`trunk build` sets `inject_autoloader: false` (`src/cmd/build.rs:202`), so the
dev-server WebSocket autoreload script is **not** present in release output —
only `trunk serve` injects it. Trunk's warnings go to stderr and **do not fail
the build**; treating them as errors requires grepping the output ourselves.

## 2. Leptos

### Version

- Latest **stable**: **0.8.20**, published 2026-06-25, MSRV 1.88, edition 2021.
- Latest overall: `0.9.0-beta` (2026-07-18).
- <https://crates.io/api/v1/crates/leptos>

### Minimal CSR setup

```toml
leptos = { version = "0.8", features = ["csr"] }
```

Book instructions: `cargo add leptos --features=csr` plus
`rustup target add wasm32-unknown-unknown`.
<https://raw.githubusercontent.com/leptos-rs/book/main/src/getting_started/README.md>

### The `nightly` feature

Exists: `nightly = ["leptos_macro/nightly", "reactive_graph/nightly", "tachys/nightly"]`
(crates.io feature map). What it changes, per the book: signals become
callable — `count()` instead of `count.get()`, and `set_count(v)` instead of
`set_count.set(v)`. That is the whole user-visible difference. Since we already
pin nightly, enabling it is free.
<https://raw.githubusercontent.com/leptos-rs/book/main/src/view/01_basic_component.md>

### Minimal `main.rs`

The book's exact text:

```rust
use leptos::prelude::*;

fn main() {
    leptos::mount::mount_to_body(|| view! { <p>"Hello, world!"</p> })
}
```

`leptos::mount::mount_to_body` is the module path; it is also re-exported from
`leptos::prelude::*`. The next chapter uses
`use leptos::mount::mount_to_body; mount_to_body(App);`.

### Signal + click handler

From the book, and identical in shape to `examples/counter` at tag `v0.8.19`:

```rust
use leptos::prelude::*;

#[component]
fn App() -> impl IntoView {
    let (count, set_count) = signal(0);
    view! {
        <button on:click=move |_| set_count.set(3)>"Click me: " {count}</button>
        <p>"Double count: " {move || count.get() * 2}</p>
    }
}
```

**PREMISE** (`create_signal`): renamed. It is `signal(...)` in 0.8; the old
name is gone.

The upstream Trunk `index.html` for that example is just:

```html
<!doctype html>
<html>
  <head>
    <link data-trunk rel="rust" data-wasm-opt="z" />
    <link data-trunk rel="icon" type="image/ico" href="/public/favicon.ico" />
  </head>
  <body></body>
</html>
```

<https://raw.githubusercontent.com/leptos-rs/leptos/v0.8.19/examples/counter/index.html>

### console_error_panic_hook

Yes, recommended. Book, "Leptos Developer Experience Improvements", item 1:
`cargo add console_error_panic_hook`, then `console_error_panic_hook::set_once();`
in `main`. Our `Cargo.lock` already carries 0.1.7.
<https://raw.githubusercontent.com/leptos-rs/book/main/src/getting_started/leptos_dx.md>

### Release profile

The book's binary-size chapter gives `opt-level = 'z'`, `lto = true`,
`codegen-units = 1`, and states:

> "For a pure client-rendered app without server considerations, just use the
> `[profile.wasm-release]` block as your `[profile.release]`."

`panic = "abort"` is prescribed **only** as part of the nightly `build-std`
path (`build-std = ["std","panic_abort","core","alloc"]` +
`build-std-features = ["panic_immediate_abort"]` in `.cargo/config.toml`),
where the book adds "Some further exploration is probably needed here."
<https://book.leptos.dev/deployment/binary_size.html>

The official `leptos-rs/start-csr` template uses `codegen-units = 1`,
`lto = true`, `opt-level = 'z'` and no `panic` key. Our existing
`[profile.release]` (`opt-level = "s"`, `lto`, `codegen-units = 1`,
`panic = "abort"`) is consistent with this guidance.

### Setting attributes on `<html>` (the `data-theme` case)

`leptos_meta` has a first-party `<Html/>` component, and the book's own example
uses `data-theme`:

```rust
<Html
    {..}
    lang="he"
    dir="rtl"
    data-theme="dark"
/>
```

Attributes after the spread operator `{..}` are applied directly to the real
`<html>` element. `<Body/>` does the same for `<body>`. Prefer this over
reaching for `document().document_element()` + `set_attribute` manually.
<https://raw.githubusercontent.com/leptos-rs/book/main/src/metadata.md>

### web-sys features

web-sys **0.3.104**, published 2026-08-08. Verified per-method on docs.rs and
against the feature graph in `crates/web-sys/Cargo.toml`:

| API | Required features |
| --- | --- |
| `web_sys::window()` / `Window` | `Window` |
| `Window::document()` | `Document`, `Window` |
| `Window::match_media()` -> `MediaQueryList` | `MediaQueryList`, `Window` |
| `MediaQueryList::matches()` / `set_onchange()` | `MediaQueryList` |
| change payload `MediaQueryListEvent` | `MediaQueryListEvent` |
| `EventTarget::add_event_listener_with_callback` | `EventTarget` |
| `Window::local_storage()` -> `Storage` | `Storage`, `Window` |
| `Storage::get_item` / `set_item` | `Storage` |
| `Document::document_element()` | `Document`, `Element` |
| `Element::set_attribute()` | `Element` |
| `Window::custom_elements()` | `CustomElementRegistry`, `Window` |

Feature dependencies mean `EventTarget` and `Node` come free:
`Window = ["EventTarget"]`, `Element = ["EventTarget","Node"]`,
`Document = ["EventTarget","Node"]`, `MediaQueryList = ["EventTarget"]`,
`MediaQueryListEvent = ["Event"]`, `Storage = []`.

**What Leptos already enables for us:** `tachys` (a leptos dependency) turns on
`Window`, `Document`, `Element`, `HtmlElement`, `Node`, `Event`, `console`,
`DomTokenList`, `CssStyleDeclaration`, `ShadowRoot`, `HtmlHtmlElement` and all
the DOM event types. Via Cargo feature unification we therefore only need to
add the gaps: **`Storage`, `MediaQueryList`, `MediaQueryListEvent`**.
<https://raw.githubusercontent.com/leptos-rs/leptos/v0.8.19/tachys/Cargo.toml>

The book confirms this pattern and links that same file.
<https://raw.githubusercontent.com/leptos-rs/book/main/src/web_sys.md>

## 3. Custom elements from Rust, and the shim pattern

### The `custom-elements` crate: dormant but intact

- v0.2.1, last published **2024-01-27** (over 2.5 years ago).
- 16,577 downloads total, 2,300 recent. MIT-or-Apache.
- Author: Greg Johnston (`gbj`) — the Leptos author.
- Repo <https://github.com/gbj/custom-elements> is **not archived**, 0 open
  issues, 91 stars. Its last *code* commit is the 0.2.1 release; the
  2025-04-20 push added only a LICENSE file.
- Dependencies are loose (`wasm-bindgen = "0.2"`, `js-sys = "0.3"`,
  `web-sys = "0.3"`), so it should still resolve against 0.2.127.
- <https://crates.io/api/v1/crates/custom-elements>

Its README states the core constraint plainly:

> "Creating a Custom Element requires calling `customElements.define()` and
> passing it an ES2015 class that `extends HTMLElement`, which is not currently
> possible to do directly from Rust."

### wasm-bindgen still cannot subclass `HTMLElement`

Two distinct `extends` attributes exist; neither solves this.

1. **`extends` on JS imports** only generates `AsRef`/`From`/`Deref`/`Upcast`
   impls for a statically known hierarchy. A type-conversion convenience, not
   real subclassing.
   <https://raw.githubusercontent.com/wasm-bindgen/wasm-bindgen/main/guide/src/reference/attributes/on-js-imports/extends.md>

2. **`extends` on Rust exports** is newer (present in the 0.2.126 and 0.2.127
   tags) and does emit a genuine `class Child extends Parent` with a real
   prototype chain. But its Limitations section says:

   > "**Same module only.** The parent must be another `#[wasm_bindgen]` struct
   > exported from the same Rust crate (same wasm module). Extending imported
   > JS classes (e.g. `HTMLElement`) from a Rust-exported struct is a separate
   > feature."

   <https://raw.githubusercontent.com/wasm-bindgen/wasm-bindgen/main/guide/src/reference/attributes/on-rust-exports/extends.md>

Note the wasm-bindgen guide has also moved: `wasm-bindgen.github.io/wasm-bindgen/...`
returns 200 while `rustwasm.github.io/wasm-bindgen/...` 404s for newer pages.

### The canonical minimal shim

`custom-elements` bundles a shim and pulls it in with
`#[wasm_bindgen(module = "/src/make_custom_element.js")]` — so "without writing
any JavaScript" means *you* do not write it, not that none exists. Trunk copies
and hashes such snippets automatically (section 1f).

```js
export function make_custom_element(
  superclass, tag_name, shadow, constructor, observedAttributes, superclassTag
) {
  customElements.define(
    tag_name,
    class extends superclass {
      static get observedAttributes() { return observedAttributes; }

      constructor() {
        super();
        constructor(this);
        this._constructor(this);
        if (shadow) {
          this.attachShadow({ mode: "open" });
          this._injectChildren(this.shadowRoot);
        }
      }

      attributeChangedCallback(name, oldValue, newValue) {
        this._attributeChangedCallback(this, name, oldValue || "", newValue);
      }

      connectedCallback() {
        if (!this.hasSetup) {
          this.hasSetup = true;
          if (!shadow) this._injectChildren(this);
        }
        this._connectedCallback(this);
      }

      disconnectedCallback() { this._disconnectedCallback(this); }
      adoptedCallback() { this._adoptedCallback(this); }
    },
    superclassTag ? { extends: superclassTag } : undefined
  );
}
```

<https://raw.githubusercontent.com/gbj/custom-elements/main/src/make_custom_element.js>

Key constraints this encodes:

- The class passed to `customElements.define` must be a real JS class; a
  wasm-bindgen-exported struct cannot be one.
- The Rust side hangs per-instance closures (`_constructor`, `_injectChildren`,
  `_connectedCallback`, `_attributeChangedCallback`, ...) onto each element
  with `js_sys::Reflect::set`.
- `window.HTMLElement` is imported as a `js_sys::Function` and passed as
  `superclass`:
  `#[wasm_bindgen(js_name = HTMLElement, js_namespace = window)] pub static HtmlElementConstructor: js_sys::Function;`
- `super()` must run before `this` is touched, which is why the shim's
  constructor cannot be replaced by a `Reflect.construct` call from Rust.

### Leptos ships nothing first-party for *defining* custom elements

Maintainer response on leptos-rs/leptos#2752:

> "The framework does not provide any particular tools for creating web
> components."

*Using* them in `view!` works fine (`leptos::html::custom("foo-bar")`).
<https://github.com/leptos-rs/leptos/issues/2752>

### `leptos-webcomponent` is unvetted — do not adopt

`leptos-webcomponent` / `leptos-webcomponent-macro` v1.2.0 offer
`#[web_component("tag-name")]` over a Leptos `#[component]`, generating
`__mount_*` / `__update_*` / `__unmount_*` / `__meta_*` wasm-bindgen exports
consumed by a shipped generic `webcomponent-runtime.js`. However:

- first published **2026-08-20**, latest 2026-08-24 — eleven days old;
- **48 and 56 total downloads** respectively;
- single unknown author (`Samujalphukan228`), repo `Samujalphukan228/wc-bridge`;
- README reads as machine-generated self-certification ("Compiles clean",
  "production-ready", checkmark status tables).

<https://crates.io/api/v1/crates/leptos-webcomponent>

### Versions

- wasm-bindgen **0.2.127** (2026-08-08) — matches our `Cargo.lock`.
- binaryen / wasm-opt **version_132** (2026-08-12).

## 4. GitHub Actions CI snippet

### Current action versions

Verified against each repo's `releases/latest`:

| Action | Version | Published |
| --- | --- | --- |
| `actions/checkout` | v7.0.1 | 2026-07-20 |
| `actions/configure-pages` | v6.0.0 | 2026-03-25 |
| `actions/upload-pages-artifact` | v5.0.0 | 2026-04-10 |
| `actions/deploy-pages` | v5.0.0 | 2026-03-25 |
| `Swatinem/rust-cache` | v2.9.2 | 2026-08-06 |
| `taiki-e/install-action` | v2.87.2 | 2026-08-30 |

Our existing `.github/workflows/deploy.yml` already pins these majors correctly.

**PREMISE** (the Leptos book's Pages workflow is a good template): it is stale.
It pins Trunk **v0.18.4** and uses `checkout@v4`, `configure-pages@v5`,
`upload-pages-artifact@v3`, `deploy-pages@v4`. Read it for structure only.
<https://raw.githubusercontent.com/leptos-rs/book/main/src/deployment/csr.md>

### Custom domain: no CNAME file needed

GitHub docs, on adding a custom domain:

> "If you are publishing your site from a branch, this will create a commit
> that adds a `CNAME` file directly to the root of your source branch. If you
> are publishing from a custom GitHub Actions workflow, no `CNAME` file is
> created, and any existing `CNAME` file is ignored and is not required."

The domain lives in repo Settings -> Pages. `.nojekyll` is likewise
unnecessary: Jekyll does not run on artifact-based deployments. (This matches
the conclusion already recorded in `stack-verification.md` sections 5 and 6.)
<https://docs.github.com/en/pages/configuring-a-custom-domain-for-your-github-pages-site/managing-a-custom-domain-for-your-github-pages-site>

### Canonical build steps

```yaml
      - uses: actions/checkout@v7

      # rustup reads rust-toolchain.toml (pinned nightly + wasm32 target)
      - run: rustup toolchain install && rustup show active-toolchain

      - uses: Swatinem/rust-cache@v2

      - uses: taiki-e/install-action@v2
        with:
          tool: trunk@0.21.14        # pin; do not float

      - uses: actions/cache@v4
        with:
          path: ~/.cache/trunk       # Trunk's downloaded wasm-bindgen / wasm-opt
          key: trunk-tools-${{ hashFiles('Trunk.toml') }}

      # public_url defaults to "/" -- correct for a root custom domain
      - run: trunk build --release

      - uses: actions/configure-pages@v6
      - uses: actions/upload-pages-artifact@v5
        with:
          path: dist
```

Version-pinning practice: add `trunk-version = "=0.21.14"` at the **root** of
`Trunk.toml` so Trunk itself refuses to build under a different version.
Supported since 0.19.0-alpha.2; accepts Cargo-style version requirements,
including pre-release requirements.

## Summary of corrections to carry into the scaffold

1. `trunkrs.dev` is squatted spam. Use <https://trunk-rs.github.io/trunk/guide/>.
2. Trunk stable is 0.21.14 (2025-05-08) and has not moved in ~16 months; 0.22
   is beta only. Pin it, and pin `trunk-version` in `Trunk.toml`.
3. wasm-opt level is an `index.html` attribute (`data-wasm-opt="z"`), never a
   `Trunk.toml` key. `[tools] wasm_opt = "version_132"` selects the *binary*.
4. Keep `public_url` at its default `/`. The book's
   `--public-url "${GITHUB_REPOSITORY#*/}"` is for project pages and would
   break a root custom domain.
5. Leptos 0.8.20 uses `signal(...)`, not `create_signal`, and
   `leptos::mount::mount_to_body`.
6. Use `leptos_meta`'s `<Html {..} data-theme="..."/>` for `<html>` attributes
   rather than hand-rolled `set_attribute` calls.
7. Only `Storage`, `MediaQueryList`, `MediaQueryListEvent` need adding to
   web-sys; `tachys` already enables the rest.
8. wasm-bindgen still cannot subclass `HTMLElement`. A JS shim is mandatory for
   custom elements; `custom-elements` (dormant since Jan 2024) ships one.
   Do not adopt `leptos-webcomponent` (11 days old, ~50 downloads).
9. No `CNAME` file and no `.nojekyll` are needed with Actions-based deployment.
10. A self-closing `<script ... />` anywhere in `index.html` is a fatal Trunk
    error, not a warning.

## Not yet verified

None of the above has been build-tested. The Trunk + Leptos combination on our
pinned `nightly-2026-08-30` with `cargo-features = ["trim-paths"]` has not been
compiled, and `custom-elements` 0.2.1 has not been checked against
wasm-bindgen 0.2.127 in practice — only on paper via its version requirements.

Two blockers would surface first: `.github/workflows/deploy.yml` runs `npm ci`
but the repo has no `package.json`, and the root `Cargo.toml` declares
`members = ["wasm"]` against a nonexistent directory (the real, currently empty
crate dirs are `crates/op-site` and `crates/op-webc`).
