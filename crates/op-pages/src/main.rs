//! Trunk `post_build` hook, second in the chain after `op-assets`: emits
//! every registered page (and every data-generated page) into the
//! staging directory as `<slug>/index.html`, reusing the staged home
//! page's `<head>` so all pages share the same hashed script, style and
//! font assets. Page bodies are namespaced XML, validated and lowered
//! before emission; a validation failure fails the build loudly.

use std::path::PathBuf;

fn head_of(html: &str) -> &str {
    let start = html.find("<head>").expect("<head> in staged index.html") + "<head>".len();
    let end = html.find("</head>").expect("</head> in staged index.html");
    &html[start..end]
}

/// Removes the page-specific tags from the shared head: title, description
/// and canonical are re-emitted per page.
fn strip_page_specific(head: &str) -> String {
    let mut out = String::with_capacity(head.len());
    let mut rest = head;
    while let Some(start) = rest.find("<title>") {
        let end = rest[start..].find("</title>").expect("closing title") + start + "</title>".len();
        out.push_str(&rest[..start]);
        rest = &rest[end..];
    }
    out.push_str(rest);
    let mut cleaned = String::with_capacity(out.len());
    for chunk in out.split_inclusive('>') {
        let is_description = chunk.contains("name=\"description\"");
        let is_canonical = chunk.contains("rel=\"canonical\"");
        if !(is_description || is_canonical) {
            cleaned.push_str(chunk);
        }
    }
    cleaned
}

fn render_page(
    shared_head: &str,
    nav: &str,
    slug: &str,
    title: &str,
    description: &str,
    body: &str,
) -> String {
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<title>{title}: openpower.tools</title>\n<meta name=\"description\" content=\"{description}\" />\n<link rel=\"canonical\" href=\"https://www.openpower.tools/{slug}/\" />{head}</head>\n<body>\n<opt-theme-toggle></opt-theme-toggle>\n{nav}\n<opt-site-header heading=\"{title}\" tagline=\"{description}\"></opt-site-header>\n<main>\n{body}</main>\n<opt-site-footer></opt-site-footer>\n<noscript><p>This page is rendered by WebAssembly; it needs JavaScript enabled.</p></noscript>\n</body>\n</html>\n",
        head = shared_head,
    )
}

fn lowered_or_exit(slug: &str, source: &str) -> String {
    match op_pages::lower(source) {
        Ok(body) => body,
        Err(errors) => {
            eprintln!("op-pages: page {slug} failed validation:");
            for e in &errors {
                eprintln!("  {e}");
            }
            std::process::exit(1);
        }
    }
}

fn main() {
    let staging = std::env::var_os("TRUNK_STAGING_DIR")
        .map(PathBuf::from)
        .expect("TRUNK_STAGING_DIR is set by Trunk for hooks");
    let index = staging.join("index.html");
    let home = std::fs::read_to_string(&index).expect("read staged index.html");
    let shared_head = strip_page_specific(head_of(&home));
    let nav = op_pages::nav_markup();
    assert!(
        home.contains("<opt-site-nav>"),
        "home page lacks the shared navigation; keep index.html in step with op_pages::home_nav_markup()"
    );
    let mut emitted = 0usize;
    for page in op_pages::PAGES {
        let body = lowered_or_exit(page.slug, page.body);
        let dir = staging.join(page.slug);
        std::fs::create_dir_all(&dir).expect("page dir");
        std::fs::write(
            dir.join("index.html"),
            render_page(
                &shared_head,
                &nav,
                page.slug,
                page.title,
                page.description,
                &body,
            ),
        )
        .expect("write page");
        emitted += 1;
    }
    for page in op_pages::generated_pages() {
        let body = lowered_or_exit(&page.slug, &page.body_xml);
        let dir = staging.join(&page.slug);
        std::fs::create_dir_all(&dir).expect("page dir");
        std::fs::write(
            dir.join("index.html"),
            render_page(
                &shared_head,
                &nav,
                &page.slug,
                &page.title,
                &page.description,
                &body,
            ),
        )
        .expect("write page");
        emitted += 1;
    }
    println!("op-pages: emitted {emitted} pages");
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "<!doctype html><html lang=\"en\"><head><title>x</title>\n<meta name=\"description\" content=\"d\" />\n<link rel=\"canonical\" href=\"https://example/\" />\n<script>s</script><link rel=\"stylesheet\" href=\"/theme-abc.css\"/><meta name=\"op-fonts\" content=\"/fonts-abc.pack\" /></head><body>b</body></html>";

    #[test]
    fn head_extraction_keeps_assets_and_drops_page_specific_tags() {
        let head = strip_page_specific(head_of(SAMPLE));
        assert!(head.contains("<script>s</script>"));
        assert!(head.contains("theme-abc.css"));
        assert!(head.contains("op-fonts"));
        assert!(!head.contains("<title>"));
        assert!(!head.contains("description"));
        assert!(!head.contains("canonical"));
    }

    #[test]
    fn rendered_pages_carry_their_own_metadata_and_the_shared_assets() {
        let head = strip_page_specific(head_of(SAMPLE));
        let source = &op_pages::PAGES[0];
        let body = op_pages::lower(source.body).expect("page lowers");
        let html = render_page(
            &head,
            &op_pages::nav_markup(),
            source.slug,
            source.title,
            source.description,
            &body,
        );
        assert!(html.contains(&format!("<title>{}: openpower.tools</title>", source.title)));
        assert!(html.contains(&format!("https://www.openpower.tools/{}/", source.slug)));
        assert!(html.matches("<title>").count() == 1);
        assert!(html.contains("opt-site-nav"));
        assert!(html.contains("theme-abc.css"));
        assert!(html.contains(&body));
    }

    #[test]
    fn generated_pages_render_through_the_same_wrapper() {
        let head = strip_page_specific(head_of(SAMPLE));
        for page in op_pages::generated_pages() {
            let body = op_pages::lower(&page.body_xml).expect("generated page lowers");
            let html = render_page(
                &head,
                &op_pages::nav_markup(),
                &page.slug,
                &page.title,
                &page.description,
                &body,
            );
            assert!(html.contains(&format!("https://www.openpower.tools/{}/", page.slug)));
            assert!(html.contains("opt-machine-probes"));
        }
    }
}
