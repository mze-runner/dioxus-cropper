//! A "wait until it has painted" yield sequence, used before synchronous,
//! CPU-bound work (`DecodedSource::decode`, `crop_decoded_to_png`) so a busy
//! state set just before the call has a chance to reach the screen.
//!
//! `setTimeout(0, ...)` alone returns control to the event loop but does not
//! guarantee a paint before the next task runs. `requestAnimationFrame`
//! fires immediately before the browser's next paint, so waiting for it
//! guarantees a pending DOM mutation has been queued for that paint; a
//! macrotask yield straight after gives that paint the rest of the
//! event-loop turn to land before the caller resumes.

use web_sys::wasm_bindgen::JsValue;

/// Resolves on the browser's next `requestAnimationFrame` callback. Resolves
/// immediately if `web_sys::window()` is unavailable (non-browser target).
async fn next_animation_frame() {
    let promise = js_sys::Promise::new(&mut |resolve, _reject| match web_sys::window() {
        Some(window) => {
            let _ = window.request_animation_frame(&resolve);
        }
        None => {
            let _ = resolve.call0(&JsValue::NULL);
        }
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}

/// Awaits this immediately after a signal write that flips on a busy state,
/// before starting the blocking work that state describes. Two yields:
/// `requestAnimationFrame` (the browser is about to paint) then one
/// macrotask turn (`gloo_timers::future::TimeoutFuture::new(0)`, the paint
/// lands).
pub async fn wait_for_paint() {
    next_animation_frame().await;
    gloo_timers::future::TimeoutFuture::new(0).await;
}
