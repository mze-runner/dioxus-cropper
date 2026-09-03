//! `URL.createObjectURL` plumbing for the two images the demo shows: the
//! picked source file and the crop result. An object URL is a constant-size
//! handle to bytes the browser holds — unlike a `data:` URI it puts no
//! multi-megabyte base64 string in the DOM, and building one costs one copy
//! of the bytes into a `Blob` rather than an encode of them.
//!
//! An object URL pins its `Blob` until revoked or the page unloads, so
//! every call site of [`create`] pairs it with a [`revoke`] of the URL it
//! replaces.

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
