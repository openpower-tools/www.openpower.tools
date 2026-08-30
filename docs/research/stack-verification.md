# Stack verification notes (verified 2026-08-31)

All claims below were checked against the cited primary sources on 2026-08-31.
Four premises from the original planning notes were contradicted by the
documentation; each is flagged **PREMISE** inline.

## 1. Lit: TypeScript decorator tsconfig

lit.dev still recommends **experimental decorators** for production:

> "Our code samples are written for experimental decorators because we
> recommend them for production at the moment."
> "Compiler output for standard decorators is unfortunately large."

Exact recommended snippet:

```json
{
  "compilerOptions": {
    "experimentalDecorators": true,
    "useDefineForClassFields": false
  }
}
```

- "`useDefineForClassFields` ... is only required when `target` is set to
  `ES2022` or greater, but it is recommended to explicitly set this to
  `false`."
- "Enabling `emitDecoratorMetadata` is not required and not recommended."
- Standard-decorator alternative (requires the `accessor` keyword on decorated
  fields): `experimentalDecorators: false`, `useDefineForClassFields: true`.
  "The `accessor` keyword was introduced in TypeScript 4.9 and standard
  decorators with metadata require TypeScript >=5.2."

**PREMISE:** lit.dev publishes no Vite guide. "Building for production"
documents Rollup only; Vite is named once in passing under TypeScript tooling.
There is no published minimal tsconfig beyond the decorator snippets. The
official TS starter kit uses `target: es2021`,
`lib: ["es2021", "DOM", "DOM.Iterable"]`, `strict: true`,
`experimentalDecorators: true` (it predates the `useDefineForClassFields`
advice; add that yourself). Requirements page: "Lit is published as ES2021."

Sources:

- https://lit.dev/docs/components/decorators/
- https://lit.dev/docs/tools/production/
- https://lit.dev/docs/tools/development/
- https://lit.dev/docs/tools/requirements/
- https://lit.dev/docs/getting-started/
- https://raw.githubusercontent.com/lit/lit-element-starter-ts/main/tsconfig.json

## 2. Vite: `base`, `new URL(..., import.meta.url)`, top-level await

- `base` default is `/` ("Type: `string`, Default: `/`"). A root-of-custom-
  domain deploy needs no `base` setting; `defineConfig({})` suffices.
- `new URL(..., import.meta.url)` needs no plugin. Verbatim: "This works
  natively in modern browsers - in fact, Vite doesn't need to process this
  code at all during development!" and "During the production build, Vite
  will perform necessary transforms so that the URLs still point to the
  correct location even after bundling and asset hashing. However, the URL
  string must be static so it can be analyzed." Caveats: non-static strings
  are left untouched; template literals do not descend into subdirectories;
  "Does not work with SSR." wasm-bindgen `--target web` glue emits a static
  literal `new URL('name_bg.wasm', import.meta.url)`, so it is analyzable.
- Top-level await: `build.target` default is `'baseline-widely-available'`
  = `['chrome111', 'edge111', 'firefox114', 'safari16.4', 'ios16.4']`. TLA
  shipped in Chrome/Edge 89, Firefox 89, Safari 15, so **no override is
  needed** on the default target. Only set `build.target: 'es2022'` (or
  `'esnext'`) if you deliberately lower the target below ES2022. Vite's
  WebAssembly page confirms the constraint: a directly imported `.wasm`
  "behaves as an async module and requires top-level `await` support."
- This Vite major bundles with Rolldown and transpiles with Oxc (not
  Rollup/esbuild). `create-vite` ships a `lit-ts` template. Vite requires
  Node.js 20.19+ or 22.12+.

Sources:

- https://vite.dev/config/shared-options
- https://vite.dev/guide/assets
- https://vite.dev/config/build-options
- https://vite.dev/guide/build
- https://vite.dev/guide/features
- https://vite.dev/guide/
- https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Operators/await

## 3. wasm-pack: maintainer, version, install, CLI

**PREMISE:** neither `rustwasm/wasm-pack` nor `drager/wasm-pack` is current.
`drager/wasm-pack` now contains only: "This repository has moved to
wasm-bindgen/wasm-pack." Canonical repo is
**https://github.com/wasm-bindgen/wasm-pack**; docs at
**https://wasm-bindgen.github.io/wasm-pack/**. The
`rustwasm.github.io/wasm-pack/book` mirror self-labels as "the unpublished
documentation".

- Latest release **0.15.0** (2026-05-15). `Cargo.toml` `version = "0.15.0"`;
  npm `wasm-pack@0.15.0` published the same day, repo field
  `git+https://github.com/wasm-bindgen/wasm-pack.git`.
- Release note for 0.15.0: "The 0.14.0 npm package shipped with the old
  `drager/wasm-pack` release URL and was never republished after the
  repository moved, so `npm install -g wasm-pack` failed with a 404." Do not
  pin 0.14.0 via npm.
- Documented installs: `curl https://wasm-bindgen.github.io/wasm-pack/installer/init.sh -sSf | sh`;
  `cargo install wasm-pack`; `npm install -g wasm-pack` /
  `yarn global add wasm-pack`; Windows `wasm-pack-init.exe`.
  `cargo binstall` is not documented and `Cargo.toml` contains no
  `[package.metadata.binstall]` section.
- CLI: `wasm-pack build [PATH] --target web --out-dir <dir> --release` is
  correct. "If none is supplied, then `--release` is used."
  `--out-name index` yields `index.js index_bg.wasm index.d.ts index_bg.d.ts
  package.json README.md`.
- wasm-bindgen CLI is auto-managed. Version is read from **`Cargo.lock`** at
  the workspace root (`Lockfile::wasm_bindgen_version()`; errors if the
  lockfile is missing). Resolution order: matching `wasm-bindgen` on `$PATH`,
  else download
  `https://github.com/wasm-bindgen/wasm-bindgen/releases/download/{version}/wasm-bindgen-{version}-{target}.tar.gz`,
  else `cargo install wasm-bindgen-cli --version {version}`.
- Binary cache: `Cache::new("wasm-pack")` -> `dirs_next::cache_dir()/.wasm-pack`
  (Linux: `~/.cache/.wasm-pack`), fallback `~/.wasm-pack`; overridable via
  the **`WASM_PACK_CACHE`** environment variable (useful as a CI cache path).
- `rust-toolchain.toml` is respected: `cargo_build_wasm` shells out to plain
  `Command::new("cargo")` with no `+toolchain` argument and sets
  `CARGO_BUILD_TARGET`. Exception: `--panic-unwind` forces `cargo +nightly`.
  On rustup setups wasm-pack runs `rustup target add wasm32-unknown-unknown`
  automatically.

Sources:

- https://github.com/drager/wasm-pack
- https://github.com/wasm-bindgen/wasm-pack
- https://github.com/wasm-bindgen/wasm-pack/releases
- https://www.npmjs.com/package/wasm-pack
- https://wasm-bindgen.github.io/wasm-pack/installer/
- https://wasm-bindgen.github.io/wasm-pack/book/quickstart.html
- https://rustwasm.github.io/wasm-pack/book/commands/build.html
- https://wasm-bindgen.github.io/wasm-pack/book/prerequisites/non-rustup-setups.html
- https://raw.githubusercontent.com/wasm-bindgen/wasm-pack/master/Cargo.toml
- https://raw.githubusercontent.com/wasm-bindgen/wasm-pack/master/src/cache.rs
- https://raw.githubusercontent.com/wasm-bindgen/wasm-pack/master/src/install/mod.rs
- https://raw.githubusercontent.com/wasm-bindgen/wasm-pack/master/src/lockfile.rs
- https://raw.githubusercontent.com/wasm-bindgen/wasm-pack/master/src/build/mod.rs
- https://docs.rs/binary-install/0.4.1/src/binary_install/lib.rs.html

## 4. GitHub Actions Pages workflow, dtolnay/rust-toolchain, Swatinem/rust-cache

**PREMISE:** `id-token: read` is wrong. The docs require **`id-token: write`**.

Canonical deploy-job skeleton from the docs:

```yaml
jobs:
  deploy:
    permissions:
      contents: read
      pages: write
      id-token: write
    runs-on: ubuntu-latest
    needs: build
    environment:
      name: github-pages
      url: ${{ steps.deployment.outputs.page_url }}
    steps:
      - name: Deploy artifact
        id: deployment
        uses: actions/deploy-pages@v4
```

Stated requirements: "The job must have a minimum of `pages: write` and
`id-token: write` permissions"; "The `needs` parameter must be set to the
`id` of the build step"; "An `environment` must be established ... The
default environment is `github-pages`." deploy-pages README: "The pages
permission relates to the `GITHUB_TOKEN` ... The id-token permission is
necessary to request the OIDC JWT token."

Build job: `actions/checkout`, build, `actions/upload-pages-artifact` with
`path: dist`. The `concurrency: { group: pages, cancel-in-progress: false }`
block is **not** on the docs page; it comes from the starter workflows at
https://github.com/actions/starter-workflows/tree/main/pages.

Version drift: docs examples show `configure-pages@v5`,
`upload-pages-artifact@v4`, `deploy-pages@v4`, `checkout@v6`. Current majors
are **configure-pages v6.0.0, upload-pages-artifact v5.0.0, deploy-pages
v5.0.0, checkout v7.0.1**. upload-pages-artifact v3+ requires deploy-pages
v4 or newer.

dtolnay/rust-toolchain:

- Toolchain is selected by the `@rev`: "`dtolnay/rust-toolchain@nightly`
  pulls in the nightly Rust toolchain."
- Inputs: `toolchain`, `targets` ("Comma-separated string of additional
  targets to install e.g. `wasm32-unknown-unknown`"), `components`.
- Use `uses: dtolnay/rust-toolchain@nightly` with
  `with: { targets: wasm32-unknown-unknown }`. If passing `toolchain` as an
  input, use `dtolnay/rust-toolchain@master` as the revision.
- The README never mentions `rust-toolchain.toml`; assume it is not read.
  Outputs: `cachekey`, `name`.

Swatinem/rust-cache (latest v2.9.1):

- "selecting a toolchain either by action or manual `rustup` calls should
  happen before the plugin, as the cache uses the current rustc version as
  its cache key", then `uses: Swatinem/rust-cache@v2`.
- `workspaces` input: entries "have the form `$workspace -> $target`. The
  `$target` part is treated as a directory relative to the `$workspace` and
  defaults to `target`". Default `. -> target`. For a crate in `wasm/`, use
  `workspaces: wasm`.
- "Using it with Nightly Rust is less effective as it will throw away the
  cache every day, unless a specific nightly build is being pinned." Pin
  `nightly-YYYY-MM-DD`.
- The key hashes `Cargo.toml`/`Cargo.lock`, `rust-toolchain.toml`, and
  `.cargo/config.toml`. It sets `CARGO_INCREMENTAL=0`.

Sources:

- https://docs.github.com/en/pages/getting-started-with-github-pages/using-custom-workflows-with-github-pages
- https://docs.github.com/en/pages/getting-started-with-github-pages/configuring-a-publishing-source-for-your-github-pages-site
- https://github.com/actions/deploy-pages
- https://github.com/actions/deploy-pages/releases
- https://github.com/actions/upload-pages-artifact
- https://github.com/actions/upload-pages-artifact/releases
- https://github.com/actions/configure-pages
- https://github.com/actions/configure-pages/releases
- https://github.com/actions/checkout/releases
- https://github.com/dtolnay/rust-toolchain
- https://github.com/Swatinem/rust-cache

## 5. Custom domain: DNS, apex redirect, CNAME file, verified domains, HTTPS, REST API

### DNS records

For `www.openpower.tools`: one `CNAME` record, name `www.openpower.tools.`,
value `openpower-tools.github.io`. "The `CNAME` record should always point to
`USERNAME.github.io` or `ORGANIZATION.github.io`, excluding the repository
name."

Apex `A` records at `@` (quoted exactly):

```
185.199.108.153
185.199.109.153
185.199.110.153
185.199.111.153
```

Apex `AAAA` records at `@` (quoted exactly):

```
2606:50c0:8000::153
2606:50c0:8001::153
2606:50c0:8002::153
2606:50c0:8003::153
```

Alternative: a single `ALIAS`/`ANAME` at `@` pointing to
`openpower-tools.github.io`. Wildcard records "put you at an immediate risk
of domain takeovers, even if you verify the domain."

### Apex to www redirect

Automatic, but only when both record sets exist: "If you configure the
correct records for each domain type through your DNS provider, GitHub Pages
will automatically create redirects between the domains. For example, if you
configure `www.example.com` as the custom domain for your site, and you have
GitHub Pages DNS records set up for the apex and `www` domains, then
`example.com` will redirect to `www.example.com`." Also: "If you configure a
`www` subdomain, we automatically attempt to secure the associated apex
domain." Set the custom domain to `www.openpower.tools` and add all eight
apex records.

### CNAME file

Not needed and ignored with Actions publishing: "If you are publishing from a
custom GitHub Actions workflow, no `CNAME` file is created, and any existing
`CNAME` file is ignored and is not required." Also: "A `CNAME` file in your
repository file does not automatically add or remove a custom domain.
Instead, you must configure the custom domain through your repository
settings or through the API."

### Verified domains (organization)

"When you verify a custom domain for your organization, only repositories
owned by that organization may be used to publish a GitHub Pages site to the
verified custom domain or the domain's immediate subdomains." "Domain
takeovers can happen when you delete your repository, when your billing plan
is downgraded, or after any other change which unlinks the custom domain or
disables GitHub Pages while the domain remains configured for GitHub Pages
and is not verified." Mechanism: org Settings > Pages > Add a domain, then a
TXT record at `_github-pages-challenge-ORGANIZATION.openpower.tools`; keep
the TXT record permanently. Verifying `openpower.tools` also covers `www`.
Verify before adding the domain to the repository.

### HTTPS caveats

- Enforce HTTPS: "It can take up to 24 hours before this option is
  available."
- Provisioning: an automatic DNS check runs, then "GitHub queues a job to
  request a TLS certificate from Let's Encrypt."
- "It can take up to an hour for your site to become available over HTTPS
  after you configure your custom domain. After you update existing DNS
  settings, you may need to remove and re-add your custom domain."
- CAA: "at least one CAA record must exist with the value `letsencrypt.org`
  for your site to be accessible over HTTPS."
- Extra `A`/`AAAA`/`ALIAS`/`ANAME` at `@`, or stray `CNAME`s, "may prevent
  the HTTPS certificate from generating."
- Full domain name must be under 64 characters (RFC3280 CN limit). DNS
  propagation up to 24 hours.

### REST API

- `POST /repos/{owner}/{repo}/pages` accepts `build_type` = `legacy` |
  `workflow` (no `source` needed for `workflow`). Returns 201.
- `PUT /repos/{owner}/{repo}/pages` accepts `cname` ("Sending a null value
  will remove the custom domain"), `https_enforced` (boolean), `build_type`
  ("`workflow` means that the site is built by a custom GitHub Actions
  workflow"). Returns 204.
- Auth: "The authenticated user must be a repository administrator,
  maintainer, or have the 'manage GitHub Pages settings' permission."
- `GET /repos/{owner}/{repo}/pages/health` runs a DNS/CAA health check
  (`is_https_eligible`, `caa_error`, `is_cname_to_github_user_domain`).
- `GET /repos/{owner}/{repo}/pages` returns `protected_domain_state`
  (`pending` / `verified` / `unverified`) and `https_certificate.state`.

```
gh api -X POST repos/openpower-tools/www.openpower.tools/pages -f build_type=workflow
gh api -X PUT  repos/openpower-tools/www.openpower.tools/pages -f cname=www.openpower.tools -F https_enforced=true
gh api repos/openpower-tools/www.openpower.tools/pages/health
```

Sources:

- https://docs.github.com/en/pages/configuring-a-custom-domain-for-your-github-pages-site/about-custom-domains-and-github-pages
- https://docs.github.com/en/pages/configuring-a-custom-domain-for-your-github-pages-site/managing-a-custom-domain-for-your-github-pages-site
- https://docs.github.com/en/pages/configuring-a-custom-domain-for-your-github-pages-site/verifying-your-custom-domain-for-github-pages
- https://docs.github.com/en/pages/configuring-a-custom-domain-for-your-github-pages-site/troubleshooting-custom-domains-and-github-pages
- https://docs.github.com/en/pages/getting-started-with-github-pages/securing-your-github-pages-site-with-https
- https://docs.github.com/en/rest/pages/pages
- https://docs.github.com/en/pages/getting-started-with-github-pages/configuring-a-publishing-source-for-your-github-pages-site

## 6. `.nojekyll`

**PREMISE:** not needed. With `actions/upload-pages-artifact` +
`actions/deploy-pages`, Jekyll never runs; the tarball is deployed as-is.
GitHub frames `.nojekyll` as a branch-deploy convention: "Most external CI
workflows 'deploy' to GitHub Pages by committing the build output to the
`gh-pages` branch of the repository, and typically include a `.nojekyll`
file." Jekyll's exclusion of paths starting with `_`, `.`, or `#` only
applies when Jekyll builds the site.

Gotcha if one is added anyway: `upload-pages-artifact` v4+ input
`include-hidden-files` defaults to `false` ("Include hidden files and
directories (those starting with a dot) in the artifact. Excludes `.git` and
`.github` regardless."), so `dist/.nojekyll` is silently dropped unless that
input is set to `true`. Vite's default `assetsDir` is `assets`, so no output
path begins with `_` or `.`.

Sources:

- https://docs.github.com/en/pages/getting-started-with-github-pages/configuring-a-publishing-source-for-your-github-pages-site
- https://docs.github.com/en/pages/setting-up-a-github-pages-site-with-jekyll/about-github-pages-and-jekyll
- https://github.com/actions/upload-pages-artifact
- https://github.blog/news-insights/bypassing-jekyll-on-github-pages/

## Summary of corrections to carry into the scaffold

1. Deploy job permission is `id-token: write`, not `read`.
2. wasm-pack lives at `wasm-bindgen/wasm-pack`, v0.15.0. Avoid npm 0.14.0
   (404). No `cargo binstall` support. wasm-bindgen version comes from
   `Cargo.lock`; cache path overridable via `WASM_PACK_CACHE`.
3. `dtolnay/rust-toolchain` does not read `rust-toolchain.toml`; pin via
   `@nightly` (or a dated nightly) and pass `targets: wasm32-unknown-unknown`.
   Pin a dated nightly so `Swatinem/rust-cache` keys stay stable.
4. Lit: `experimentalDecorators: true` + `useDefineForClassFields: false`.
   lit.dev has no Vite guidance; Rollup is its documented bundler.
5. No `.nojekyll`; no `base` override; no `build.target` override needed for
   top-level await on the current Vite default target.
6. Set the Pages custom domain to `www.openpower.tools`, add the www CNAME
   plus all eight apex A/AAAA records, verify `openpower.tools` at the org
   level first, and configure `build_type=workflow` + `cname` +
   `https_enforced` through the REST API.
