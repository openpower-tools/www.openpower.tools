//! `<opt-machine-probes>`: live capability probes of the visitor's own
//! browser and machine - the "your machine today" panel beside the
//! can-i-use matrix. Everything is measured in-page, never guessed from
//! the user agent: this component is itself Rust compiled to
//! WebAssembly, so the first probe is its own existence. The remaining
//! probes mirror the feature set the benchmark protocol registered:
//! wasm SIMD (validated bytes), threads (cross-origin isolation and
//! SharedArrayBuffer), JSPI (WebAssembly.Suspending), WebGPU (adapter
//! request, resolved asynchronously), and timer granularity (measured).

use op_webc::{CustomElement, ElementDefinition};
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use web_sys::HtmlElement;

use super::{BASE_CSS, shadow_root};
use crate::html::escape;

pub const DEFINITION: ElementDefinition = ElementDefinition {
    source: op_webc::here!(),
    tag: "opt-machine-probes",
    observed_attributes: &[],
    create: |host| Box::new(MachineProbes { host }),
};

struct MachineProbes {
    host: HtmlElement,
}

/// Minimal wasm module using a SIMD opcode (i32.const 0; i8x16.splat;
/// drop) - the canonical validation probe for fixed-width SIMD.
const SIMD_PROBE: &[u8] = &[
    0, 97, 115, 109, 1, 0, 0, 0, 1, 5, 1, 96, 0, 1, 123, 3, 2, 1, 0, 10, 10, 1, 8, 0, 65, 0, 253,
    15, 26, 11,
];

struct Probe {
    id: &'static str,
    name: &'static str,
    variant: &'static str,
    value: String,
}

fn global_has(name: &str) -> bool {
    js_sys::Reflect::has(&js_sys::global(), &JsValue::from_str(name)).unwrap_or(false)
}

fn wasm_namespace() -> Option<js_sys::Object> {
    js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("WebAssembly"))
        .ok()
        .and_then(|v| v.dyn_into::<js_sys::Object>().ok())
}

fn timer_granularity_us() -> Option<f64> {
    let performance = web_sys::window()?.performance()?;
    let mut min = f64::MAX;
    let mut last = performance.now();
    for _ in 0..4000 {
        let t = performance.now();
        if t > last {
            min = min.min(t - last);
        }
        last = t;
    }
    (min < f64::MAX).then_some(min * 1000.0)
}

fn sync_probes() -> Vec<Probe> {
    let mut probes = Vec::new();

    probes.push(Probe {
        id: "wasm",
        name: "WebAssembly",
        variant: "ok",
        value: "running (this panel is Rust compiled to wasm)".to_owned(),
    });

    let simd = js_sys::WebAssembly::validate(&js_sys::Uint8Array::from(SIMD_PROBE).into())
        .unwrap_or(false);
    probes.push(Probe {
        id: "simd",
        name: "wasm SIMD",
        variant: if simd { "ok" } else { "danger" },
        value: if simd {
            "128-bit SIMD validates"
        } else {
            "not supported"
        }
        .to_owned(),
    });

    let isolated = web_sys::window()
        .map(|w| {
            js_sys::Reflect::get(&w, &JsValue::from_str("crossOriginIsolated"))
                .ok()
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        })
        .unwrap_or(false);
    let sab = global_has("SharedArrayBuffer");
    let (variant, value) = match (isolated, sab) {
        (true, true) => ("ok", "cross-origin isolated; SharedArrayBuffer available"),
        (false, _) => (
            "warning",
            "no cross-origin isolation here (host headers), so no shared-memory threads",
        ),
        (true, false) => ("danger", "isolated but SharedArrayBuffer missing"),
    };
    probes.push(Probe {
        id: "threads",
        name: "wasm threads",
        variant,
        value: value.to_owned(),
    });

    let jspi = wasm_namespace()
        .map(|ns| js_sys::Reflect::has(&ns, &JsValue::from_str("Suspending")).unwrap_or(false))
        .unwrap_or(false);
    probes.push(Probe {
        id: "jspi",
        name: "JSPI",
        variant: if jspi { "ok" } else { "warning" },
        value: if jspi {
            "WebAssembly.Suspending present".to_owned()
        } else {
            "absent (on POWER: present in the SpiderMonkey ppc64le JIT port; V8 ppc64 has none)"
                .to_owned()
        },
    });

    let gpu = web_sys::window()
        .map(|w| {
            js_sys::Reflect::get(&w.navigator(), &JsValue::from_str("gpu"))
                .map(|v| !v.is_undefined() && !v.is_null())
                .unwrap_or(false)
        })
        .unwrap_or(false);
    probes.push(Probe {
        id: "webgpu",
        name: "WebGPU",
        variant: if gpu { "info" } else { "warning" },
        value: if gpu {
            "navigator.gpu present; requesting adapter…"
        } else {
            "navigator.gpu absent"
        }
        .to_owned(),
    });

    let (variant, value) = match timer_granularity_us() {
        Some(us) if us <= 20.0 => ("ok", format!("{us:.1} µs granularity")),
        Some(us) => (
            "warning",
            format!("{us:.1} µs granularity (coarsened without isolation)"),
        ),
        None => ("warning", "performance.now unavailable".to_owned()),
    };
    probes.push(Probe {
        id: "timer",
        name: "timer",
        variant,
        value,
    });

    probes
}

fn render(probes: &[Probe]) -> String {
    let mut rows = String::new();
    for probe in probes {
        rows.push_str(&format!(
            "<div class=\"row\"><dt>{}</dt><dd><opt-badge variant=\"{}\">{}</opt-badge> \
<span data-probe=\"{}\">{}</span></dd></div>",
            escape(probe.name),
            probe.variant,
            escape(probe.variant),
            probe.id,
            escape(&probe.value),
        ));
    }
    format!(
        "<style>{BASE_CSS}
dl {{ margin: 0; }}
.row {{ display: flex; gap: 0.6rem; align-items: baseline; padding: 0.2rem 0; }}
dt {{ min-width: 8rem; font-weight: 600; }}
dd {{ margin: 0; }}
[data-probe] {{ color: var(--op-muted, inherit); }}
</style><dl>{rows}</dl>"
    )
}

impl CustomElement for MachineProbes {
    fn connected(&mut self) {
        let shadow = shadow_root(&self.host);
        shadow.set_inner_html(&render(&sync_probes()));

        // WebGPU adapter arrives asynchronously; update its row in place.
        let Some(window) = web_sys::window() else {
            return;
        };
        let gpu = match js_sys::Reflect::get(&window.navigator(), &JsValue::from_str("gpu")) {
            Ok(v) if !v.is_undefined() && !v.is_null() => v,
            _ => return,
        };
        let request = match js_sys::Reflect::get(&gpu, &JsValue::from_str("requestAdapter"))
            .ok()
            .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
        {
            Some(f) => f,
            None => return,
        };
        let promise = match request
            .call0(&gpu)
            .ok()
            .and_then(|p| p.dyn_into::<js_sys::Promise>().ok())
        {
            Some(p) => p,
            None => return,
        };
        let shadow = shadow.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let adapter = wasm_bindgen_futures::JsFuture::from(promise).await.ok();
            let got = adapter
                .map(|a| !a.is_undefined() && !a.is_null())
                .unwrap_or(false);
            if let Ok(Some(row)) = shadow.query_selector("[data-probe=webgpu]") {
                row.set_text_content(Some(if got {
                    "adapter acquired: WebGPU is live here"
                } else {
                    "navigator.gpu present but no adapter (driver/platform)"
                }));
            }
        });
    }
}
