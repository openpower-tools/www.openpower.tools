//! Rewrites of the staged page head that make it the same on every build.
//!
//! Trunk writes a `<link rel="modulepreload">` for the app's script and one
//! for each wasm-bindgen inline snippet, and it does not write the snippets
//! in the same order from build to build: two CI builds of the same commit
//! swapped `op-site`'s and `op-webc`'s `inline0.js`, and that swap was the
//! only difference between them in all 38 pages. A preload is a fetch hint,
//! so its place among its neighbours means nothing to a browser, but it
//! changes the bytes of every page, which breaks the claim that the site
//! builds to the same bytes and makes every rebuild look like a change to
//! whatever is keyed on those bytes.

/// The start of the tags this orders.
const MODULE_PRELOAD: &str = "<link rel=\"modulepreload\"";

/// The href of a tag, which is what it is ordered by.
fn href(tag: &str) -> &str {
    tag.split_once("href=\"")
        .and_then(|(_, rest)| rest.split_once('"'))
        .map_or("", |(href, _)| href)
}

/// Every run of adjacent `<link rel="modulepreload">` tags, sorted by href.
/// Tags are adjacent when one begins where the last ends; anything between
/// two tags, whitespace included, ends a run, and runs are sorted each on
/// its own, so nothing moves across text it did not sit beside. The rest of
/// the page is left exactly as it was.
pub fn order_module_preloads(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(start) = rest.find(MODULE_PRELOAD) {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let mut run: Vec<&str> = Vec::new();
        while rest.starts_with(MODULE_PRELOAD) {
            let Some(end) = rest.find('>') else { break };
            run.push(&rest[..=end]);
            rest = &rest[end + 1..];
        }
        if run.is_empty() {
            // an unterminated tag: leave it and the rest as they are
            break;
        }
        run.sort_by(|a, b| href(a).cmp(href(b)).then_with(|| a.cmp(b)));
        out.extend(run);
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const APP: &str = r#"<link rel="modulepreload" href="/op-site-61e7.js" crossorigin="anonymous" integrity="sha384-A">"#;
    const SITE: &str = r#"<link rel="modulepreload" href="/snippets/op-site-faac/inline0.js" crossorigin="anonymous" integrity="sha384-B">"#;
    const WEBC: &str = r#"<link rel="modulepreload" href="/snippets/op-webc-2e9c/inline0.js" crossorigin="anonymous" integrity="sha384-C">"#;
    const WASM: &str = r#"<link rel="preload" href="/op-site-61e7_bg.wasm" as="fetch">"#;

    fn page(links: &[&str]) -> String {
        format!(
            "<head><script>go()</script>\n  {}{WASM}</head><body>b</body>",
            links.concat()
        )
    }

    /// The build that swapped the snippets, and every other order of the
    /// three tags, come out as one page: sorted by href, everything else
    /// untouched.
    #[test]
    fn every_order_of_the_preloads_writes_the_same_page() {
        let orders: [[&str; 3]; 6] = [
            [APP, SITE, WEBC],
            [APP, WEBC, SITE],
            [SITE, APP, WEBC],
            [SITE, WEBC, APP],
            [WEBC, APP, SITE],
            [WEBC, SITE, APP],
        ];
        let expected = page(&[APP, SITE, WEBC]);
        for order in orders {
            assert_eq!(order_module_preloads(&page(&order)), expected, "{order:?}");
        }
    }

    /// Ordering only reorders: the result holds the same characters as the
    /// input, and a page already in order comes back byte for byte.
    #[test]
    fn ordering_only_reorders() {
        let shuffled = page(&[WEBC, APP, SITE]);
        let ordered = order_module_preloads(&shuffled);
        let mut a: Vec<char> = shuffled.chars().collect();
        let mut b: Vec<char> = ordered.chars().collect();
        a.sort_unstable();
        b.sort_unstable();
        assert_eq!(a, b);
        assert_eq!(order_module_preloads(&ordered), ordered);
    }

    /// Text between two tags ends a run, and each run is sorted on its own,
    /// so no tag moves across text it did not sit beside.
    #[test]
    fn runs_are_sorted_each_on_its_own() {
        let html = format!("{WEBC}{SITE} <meta x>{WEBC}{APP}");
        assert_eq!(
            order_module_preloads(&html),
            format!("{SITE}{WEBC} <meta x>{APP}{WEBC}")
        );
    }

    /// A page with nothing to order, and one whose last tag never closes,
    /// come back unchanged.
    #[test]
    fn a_page_with_nothing_to_order_is_unchanged() {
        let plain = "<head><link rel=\"preload\" href=\"/a\"></head>";
        assert_eq!(order_module_preloads(plain), plain);
        let open = format!("<head>{SITE}<link rel=\"modulepreload\" href=\"/z\"");
        assert_eq!(order_module_preloads(&open), open);
    }
}
