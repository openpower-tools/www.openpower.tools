//! Progressive webfont loading.
//!
//! The wasm binary carries no font bytes. After startup, [`install`] reads the
//! pack URL from `<meta name="op-fonts">` (injected at build time by the
//! `op-assets` Trunk hook), fetches the content-hashed pack through the Cache
//! Storage API when available (so revisits register fonts without a network
//! round trip even under short HTTP cache lifetimes), decodes it with
//! [`op_fontpack`] and registers every face through the CSS Font Loading API.
//!
//! Until the pack arrives, text renders in the metric-fitted `local()`
//! fallback faces declared in `styles/theme.css`, so the eventual swap changes
//! letterforms without moving layout. The stacks reference only embedded and
//! fallback families: locally installed fonts (including the licensed
//! originals) are deliberately never used, so rendering is identical on every
//! machine. There are still no fetchable font URLs: the pack is a single
//! opaque container, matching the embedded-fonts policy.

use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Cache, FontFace, FontFaceDescriptors, Response, Window};

/// Starts the asynchronous font installation; returns immediately.
pub fn install() {
    wasm_bindgen_futures::spawn_local(async {
        if let Err(error) = install_inner().await {
            web_sys::console::warn_2(&JsValue::from_str("op-fonts: pack not installed"), &error);
        }
    });
}

async fn install_inner() -> Result<(), JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
    let document = window
        .document()
        .ok_or_else(|| JsValue::from_str("no document"))?;
    let Some(meta) = document.query_selector("meta[name=\"op-fonts\"]")? else {
        return Err(JsValue::from_str("no op-fonts meta"));
    };
    let url = meta
        .get_attribute("content")
        .filter(|c| !c.is_empty())
        .ok_or_else(|| JsValue::from_str("empty op-fonts meta"))?;
    let bytes = fetch_with_cache(&window, &url).await?;
    let faces = op_fontpack::decode(&bytes).map_err(|e| JsValue::from_str(e.0))?;
    let fonts = document.fonts();
    for face in faces {
        let descriptors = FontFaceDescriptors::new();
        descriptors.set_weight(&face.weight);
        descriptors.set_style(&face.style);
        if let Some((size_adjust, ascent, descent)) = &face.metrics {
            // Metric descriptors are recent additions; set them on the
            // dictionary object directly so the web-sys version does not
            // matter.
            for (key, value) in [
                ("sizeAdjust", size_adjust.as_str()),
                ("ascentOverride", ascent.as_str()),
                ("descentOverride", descent.as_str()),
                ("lineGapOverride", "0%"),
            ] {
                let _ = js_sys::Reflect::set(
                    descriptors.as_ref(),
                    &JsValue::from_str(key),
                    &JsValue::from_str(value),
                );
            }
        }
        let font_face =
            FontFace::new_with_u8_array_and_descriptors(&face.family, &face.bytes, &descriptors)?;
        let _ = fonts.add(&font_face);
    }
    // Late listeners can re-probe availability once the pack is in.
    if let Ok(event) = web_sys::Event::new("op-fonts-installed") {
        let _ = document.dispatch_event(&event);
    }
    Ok(())
}

/// Fetches `url`, serving from and populating the `op-fonts` cache when the
/// Cache Storage API is available; the URL is content-hashed, so a cache hit
/// is always current.
async fn fetch_with_cache(window: &Window, url: &str) -> Result<Vec<u8>, JsValue> {
    let future = wasm_bindgen_futures::JsFuture::from;
    if let Ok(cache_storage) = window.caches() {
        let cache: Cache = future(cache_storage.open("op-fonts")).await?.dyn_into()?;
        if let Ok(hit) = future(cache.match_with_str(url)).await
            && !hit.is_undefined()
        {
            let response: Response = hit.dyn_into()?;
            return body_bytes(&response).await;
        }
        let response: Response = future(window.fetch_with_str(url)).await?.dyn_into()?;
        if response.ok()
            && let Ok(copy) = response.clone()
        {
            let _ = future(cache.put_with_str(url, &copy)).await;
        }
        return body_bytes(&response).await;
    }
    let response: Response = future(window.fetch_with_str(url)).await?.dyn_into()?;
    body_bytes(&response).await
}

async fn body_bytes(response: &Response) -> Result<Vec<u8>, JsValue> {
    if !response.ok() {
        return Err(JsValue::from_str(&format!("HTTP {}", response.status())));
    }
    let buffer = wasm_bindgen_futures::JsFuture::from(response.array_buffer()?).await?;
    Ok(js_sys::Uint8Array::new(&buffer).to_vec())
}
