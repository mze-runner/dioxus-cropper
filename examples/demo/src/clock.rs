//! A millisecond clock for the stage timings: `Date.now()` in the browser,
//! where `std::time::Instant` is unavailable, and `Instant` on desktop.

/// Milliseconds since an arbitrary fixed origin.
#[cfg(target_arch = "wasm32")]
pub fn now_ms() -> f64 {
    js_sys::Date::now()
}

/// Milliseconds since an arbitrary fixed origin.
#[cfg(not(target_arch = "wasm32"))]
pub fn now_ms() -> f64 {
    use std::sync::OnceLock;
    use std::time::Instant;

    static ORIGIN: OnceLock<Instant> = OnceLock::new();
    ORIGIN.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.0
}
