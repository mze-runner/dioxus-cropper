//! URLs for the two images the demo shows: the picked source file and the
//! crop result. In the browser these are `URL.createObjectURL` handles; on
//! desktop, paths served from memory by an asset handler (see
//! [`use_handler`]).
//!
//! An object URL is a constant-size handle to bytes the browser holds —
//! unlike a `data:` URI it puts no multi-megabyte base64 string in the DOM,
//! and building one costs one copy of the bytes into a `Blob` rather than
//! an encode of them.
//!
//! Both kinds of URL pin their bytes until revoked, so every call site of
//! [`create`] pairs it with a [`revoke`] of the URL it replaces.

#[cfg(target_arch = "wasm32")]
pub use web::{create, revoke};

#[cfg(not(target_arch = "wasm32"))]
pub use desktop::{create, revoke};

/// Registers the desktop asset handler that serves [`create`]'s URLs. Call
/// once, from the root component. Does nothing in the browser.
#[cfg(target_arch = "wasm32")]
pub fn use_handler() {}

#[cfg(not(target_arch = "wasm32"))]
pub use desktop::use_handler;

#[cfg(target_arch = "wasm32")]
mod web {
    use std::sync::Arc;

    use web_sys::wasm_bindgen::JsValue;

    /// Builds a `Blob` of `mime` type over a copy of `bytes` and returns an
    /// object URL for it.
    ///
    /// # Errors
    ///
    /// Returns the browser's own message when the `Blob` constructor or
    /// `URL.createObjectURL` throws — out of memory is the realistic cause.
    pub fn create(bytes: &[u8], mime: &str) -> Result<Arc<str>, String> {
        // A one-element BlobPart sequence. `Uint8Array::from` copies `bytes`
        // into the JS heap, which is what lets the browser own the blob's
        // lifetime independently of this Rust-side allocation.
        let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(bytes));
        let options = web_sys::BlobPropertyBag::new();
        options.set_type(mime);
        let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options)
            .map_err(describe)?;
        web_sys::Url::create_object_url_with_blob(&blob)
            .map(Arc::from)
            .map_err(describe)
    }

    /// Releases `url`'s blob. Failure is deliberately ignored: the only
    /// consequence of a failed (or repeated) revocation is a blob living until
    /// page unload, and there is no meaningful recovery to attempt.
    pub fn revoke(url: &str) {
        let _ = web_sys::Url::revoke_object_url(url);
    }

    /// A `JsValue` exception as display text for the demo's error readout.
    fn describe(e: JsValue) -> String {
        e.as_string().unwrap_or_else(|| format!("{e:?}"))
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod desktop {
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex, PoisonError};

    use dioxus::desktop::wry::http::Response;
    use dioxus::desktop::{use_asset_handler, AssetRequest, RequestAsyncResponder};

    /// The first path segment the asset handler answers.
    const ROUTE: &str = "demo-image";

    /// An image's bytes and MIME type.
    type Entry = (Arc<[u8]>, String);

    /// The held images per URL id, until [`revoke`].
    static ENTRIES: Mutex<BTreeMap<u64, Entry>> = Mutex::new(BTreeMap::new());
    static NEXT_ID: AtomicU64 = AtomicU64::new(0);

    /// Holds a copy of `bytes` and returns the path that serves it as `mime`.
    ///
    /// # Errors
    ///
    /// Never fails; the `Result` matches the browser implementation.
    pub fn create(bytes: &[u8], mime: &str) -> Result<Arc<str>, String> {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        ENTRIES
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id, (Arc::from(bytes), mime.to_string()));
        Ok(Arc::from(format!("/{ROUTE}/{id}")))
    }

    /// Releases the bytes behind `url`.
    pub fn revoke(url: &str) {
        if let Some(id) = parse_id(url) {
            ENTRIES
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&id);
        }
    }

    /// Registers the handler answering `/{ROUTE}/{id}` requests from the
    /// webview with the stored bytes, or 404 for an unknown id.
    pub fn use_handler() {
        use_asset_handler(
            ROUTE,
            |request: AssetRequest, responder: RequestAsyncResponder| {
                let entry = parse_id(request.uri().path()).and_then(|id| {
                    ENTRIES
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .get(&id)
                        .cloned()
                });
                let response = match entry {
                    Some((bytes, mime)) => Response::builder()
                        .header("Content-Type", mime)
                        .body(bytes.to_vec()),
                    None => Response::builder().status(404).body(Vec::new()),
                };
                if let Ok(response) = response {
                    responder.respond(response);
                }
            },
        );
    }

    /// The id in a `/{ROUTE}/{id}` path.
    fn parse_id(path: &str) -> Option<u64> {
        path.strip_prefix('/')?
            .strip_prefix(ROUTE)?
            .strip_prefix('/')?
            .parse()
            .ok()
    }
}
