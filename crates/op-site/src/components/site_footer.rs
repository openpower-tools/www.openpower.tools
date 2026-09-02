//! `<opt-site-footer>`: independence statement, source and licence links.

use op_webc::{CustomElement, ElementDefinition};
use web_sys::HtmlElement;

use super::{BASE_CSS, shadow_root};

pub const DEFINITION: ElementDefinition = ElementDefinition {
    tag: "opt-site-footer",
    observed_attributes: &[],
    create: |host| Box::new(SiteFooter { host }),
};

struct SiteFooter {
    host: HtmlElement,
}

const REPO: &str = "https://github.com/openpower-tools/www.openpower.tools";

impl CustomElement for SiteFooter {
    fn connected(&mut self) {
        shadow_root(&self.host).set_inner_html(&format!(
            "<style>{BASE_CSS}
footer {{
  margin-top: 2rem;
  padding-top: 1rem;
  border-top: 1px solid var(--op-border);
  font-size: 0.875rem;
  color: var(--op-muted);
}}
</style>
<footer>
  <p>openpower.tools is an independent community project. It is not affiliated with or endorsed by the OpenPOWER Foundation, IBM, or Raptor Computing Systems.</p>
  <p>Source: <a href=\"{REPO}\">github.com/openpower-tools/www.openpower.tools</a></p>
  <p>Code is licensed under the <a href=\"https://www.gnu.org/licenses/gpl-3.0.html\">GPL-3.0-or-later</a>; content under <a href=\"https://creativecommons.org/licenses/by-sa/4.0/\">CC BY-SA 4.0</a>. Product and organisation names belong to their owners. See <a href=\"{REPO}/blob/main/LICENSE.md\">LICENSE.md</a>.</p>
</footer>"
        ));
    }
}
