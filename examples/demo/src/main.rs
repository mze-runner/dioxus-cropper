//! Browser demo for `dioxus-cropper`: pick a local image, then pan, zoom,
//! rotate and crop it. Every configurable prop of `Cropper` is exercised —
//! most from the rail on the right, `classes` wired to a visible cosmetic
//! hook in `Stage`. Plain Dioxus, one hand-written stylesheet, no network
//! requests — works offline.

mod components;
mod icons;
mod object_url;
mod paint;
mod types;

use std::io::Cursor;
use std::sync::{Arc, OnceLock};

use dioxus::prelude::*;
use dioxus_cropper::geometry::{
    clamp_offset, contain_scale, min_zoom_to_cover, normalize_rotation, Point, Size, Stencil,
    ViewTransform,
};
use dioxus_cropper::{
    crop_decoded_to_png, output_size, CropError, DecodedSource, PanDirection, MAX_OUTPUT_PIXELS,
};

use components::{
    PositionGroup, ResultStrip, RotateGroup, ShapeGroup, SourceGroup, Stage, StageReadout,
    StageSource, TimingReadout, TuningGroup, ZoomGroup,
};
use icons::IconCrop;
use types::{CursorChoice, ShapeKind, ViewportPreset};

const DEMO_CSS: Asset = asset!("/assets/demo.css");

/// A pixel step used by the pan buttons — an arbitrary, readable amount of
/// on-screen movement per click, not a value the crate prescribes.
const PAN_STEP: f32 = 24.0;

/// A zoom step used by the zoom buttons.
const ZOOM_STEP: f32 = 0.1;

/// A zoom floor applied regardless of the "position restriction" toggle —
/// guards against `zoom` collapsing to (near) zero, independent of whether
/// stencil coverage is enforced.
const MIN_SAFE_ZOOM: f32 = 0.05;

/// The highest zoom this demo allows — guards against sustained wheel input
/// driving `zoom` arbitrarily high and asking the browser to rasterise an
/// arbitrarily large scaled element. The demo's own choice; the crate places
/// no ceiling on `zoom` itself.
const MAX_ZOOM: f32 = 8.0;

/// The multiplier applied to the raw wheel `delta` `Cropper` reports via
/// `on_zoom` before folding it into `zoom`. `Cropper`'s own doc is explicit
/// that wheel calibration is the caller's to own — this is the demo's own
/// arbitrary choice, not a value the crate prescribes.
const WHEEL_ZOOM_STEP: f32 = 0.001;

/// Every field but `file_name` is an `Arc`, so cloning this whole struct
/// out of the `image` signal — the read pattern every callback below uses —
/// is a few refcount bumps plus one heap allocation for
/// `file_name: String`, not a copy of the picked file's bytes or pixels.
#[derive(Clone)]
struct LoadedImage {
    file_name: String,
    /// Object URL over a `Blob` of the ORIGINAL file bytes — what the
    /// browser renders in the stage. Revoked when the next pick replaces
    /// it.
    src_url: Arc<str>,
    /// The picked file's raw, undecoded bytes, kept for the deferred
    /// pixel decode on the first Crop press.
    bytes: Arc<[u8]>,
    /// From the image header alone (`ImageReader::into_dimensions`) — no
    /// pixel decode happens at pick time.
    natural_size: Size,
    /// Filled by the first Crop press that decodes successfully and reused
    /// by every press after it, per the crate's own guidance
    /// (`DecodedSource::decode`'s doc comment). Empty until then — picking
    /// a file costs a header probe, not a full decode.
    decoded: Arc<OnceLock<DecodedSource>>,
}

#[derive(Clone)]
struct CroppedResult {
    /// Object URL over the crop's PNG bytes. Revoked when the next crop
    /// replaces it or the next pick clears it.
    url: Arc<str>,
    width: u32,
    height: u32,
    size_bytes: usize,
}

/// The demo's single busy state, covering both spans of work it drives:
/// reading and header-probing a picked file, and running the crop — which
/// on the first press also performs the deferred pixel decode. One signal
/// for both — the file picker and the Crop button never run at the same
/// time, so there is only ever one thing to be busy with.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Busy {
    #[default]
    Idle,
    Loading,
    Cropping,
}

impl Busy {
    fn is_busy(self) -> bool {
        self != Busy::Idle
    }
}

/// Wall-clock stage durations for the timing readout under the stage.
/// Pick-time stages (`read_ms`, `probe_ms`) are written by the file
/// picker, crop-time stages by the Crop press; `None` means the stage has
/// not run for the current image and the readout shows only what has.
#[derive(Clone, Copy, Default)]
struct StageTimings {
    read_ms: Option<f64>,
    probe_ms: Option<f64>,
    decode: Option<DecodeTiming>,
    crop_ms: Option<f64>,
    url_ms: Option<f64>,
}

/// The decode stage runs at most once per picked file — after that a press
/// hits the cache, which is worth showing as such rather than as a
/// suspicious 0 ms.
#[derive(Clone, Copy)]
enum DecodeTiming {
    Ran(f64),
    Cached,
}

/// Formats a stage duration: one decimal below 10 ms so a sub-millisecond
/// header probe doesn't read as the misleading "0 ms", whole milliseconds
/// above.
fn format_ms(ms: f64) -> String {
    if ms < 10.0 {
        format!("{ms:.1}")
    } else {
        format!("{ms:.0}")
    }
}

/// The one-line stage summary, e.g.
/// `read 12 ms · probe 0.4 ms · decode 840 ms · crop 130 ms` — only stages
/// that have run appear. `None` until a file is picked.
fn timing_line(timings: &StageTimings) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(ms) = timings.read_ms {
        parts.push(format!("read {} ms", format_ms(ms)));
    }
    if let Some(ms) = timings.probe_ms {
        parts.push(format!("probe {} ms", format_ms(ms)));
    }
    match timings.decode {
        Some(DecodeTiming::Ran(ms)) => parts.push(format!("decode {} ms", format_ms(ms))),
        Some(DecodeTiming::Cached) => parts.push("decode cached".to_string()),
        None => {}
    }
    if let Some(ms) = timings.crop_ms {
        parts.push(format!("crop {} ms", format_ms(ms)));
    }
    if let Some(ms) = timings.url_ms {
        parts.push(format!("url {} ms", format_ms(ms)));
    }
    (!parts.is_empty()).then(|| parts.join(" \u{b7} "))
}

/// Formats a byte count for the result readout — whole KiB once the value
/// reaches one, otherwise the exact byte count so a sub-1-KiB PNG (a small
/// crop, or a solid-colour source that compresses hard) doesn't read as the
/// misleading "0 KB".
fn format_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    }
}

/// The lowest zoom this demo allows for `natural_size` at `rotation_deg`,
/// under `viewport`/`stencil` — the crate's own coverage floor
/// (`min_zoom_to_cover`) when `restrict` is on, `MIN_SAFE_ZOOM` alone when
/// it's off. Off legitimately lets the stencil frame empty space, per
/// `crop_decoded_to_png`'s own doc on out-of-source samples.
fn effective_min_zoom(
    natural: Size,
    viewport: Size,
    stencil: Stencil,
    rotation_deg: f32,
    restrict: bool,
) -> f32 {
    if !restrict {
        return MIN_SAFE_ZOOM;
    }
    let fit_scale = contain_scale(natural, viewport);
    min_zoom_to_cover(natural, fit_scale, rotation_deg, stencil).max(MIN_SAFE_ZOOM)
}

/// Clamps `zoom` to be at least `floor` (the coverage/safety minimum for the
/// current image/config) and at most [`MAX_ZOOM`]. `floor` wins if the two
/// conflict — a degenerate rotation/coverage case where `floor` exceeds
/// `MAX_ZOOM` — since violating stencil coverage is worse than exceeding the
/// zoom ceiling.
fn clamp_zoom(zoom: f32, floor: f32) -> f32 {
    zoom.max(floor).min(MAX_ZOOM.max(floor))
}

/// Clamps `view`'s offset in place against `natural_size` — a no-op when
/// `restrict` is off. The same repair every mutation site (pan, zoom,
/// rotate, config change) applies before the value is rendered.
fn reclamp(
    view: &mut ViewTransform,
    natural: Size,
    viewport: Size,
    stencil: Stencil,
    restrict: bool,
) {
    if !restrict {
        return;
    }
    let fit_scale = contain_scale(natural, viewport);
    view.offset = clamp_offset(
        view.offset,
        natural,
        fit_scale,
        view.zoom,
        view.rotation,
        stencil,
    );
}

/// The centred, unrotated view for `natural_size` under the current
/// viewport/stencil/restriction — what a freshly loaded image converges to.
///
/// The neutral view is the contain-fit (`zoom = 1.0`), raised only if
/// `effective_min_zoom` demands a higher floor — never the floor itself,
/// which with restriction off is `MIN_SAFE_ZOOM` (a floor on user-driven
/// zoom-out, not the starting zoom).
fn fresh_view(natural: Size, viewport: Size, stencil: Stencil, restrict: bool) -> ViewTransform {
    let floor = effective_min_zoom(natural, viewport, stencil, 0.0, restrict);
    let mut view = ViewTransform {
        zoom: clamp_zoom(ViewTransform::default().zoom, floor),
        ..ViewTransform::default()
    };
    reclamp(&mut view, natural, viewport, stencil, restrict);
    view
}

/// The one protocol every view-changing callback follows: read `image`
/// (return if none loaded), read the live viewport/stencil/restrict-on
/// config, run `mutate` against a working copy of `view`, raise `zoom` to
/// that config's floor, reclamp the offset, then write the result back.
/// `mutate` receives `natural_size` since several callers need it (rotation
/// floor, offset clamp) without re-reading `image` themselves.
fn apply_view_edit(
    image: Signal<Option<LoadedImage>>,
    mut view: Signal<ViewTransform>,
    vp_size: Memo<Size>,
    stencil: Memo<Stencil>,
    restrict: Signal<bool>,
    mutate: impl FnOnce(&mut ViewTransform, Size),
) {
    let Some(loaded) = image.read().clone() else {
        return;
    };
    let vp = vp_size();
    let st = stencil();
    let restrict_on = restrict();
    let mut vw = view();
    mutate(&mut vw, loaded.natural_size);
    let floor = effective_min_zoom(loaded.natural_size, vp, st, vw.rotation, restrict_on);
    vw.zoom = clamp_zoom(vw.zoom, floor);
    reclamp(&mut vw, loaded.natural_size, vp, st, restrict_on);
    view.set(vw);
}

/// Runs the deferred pixel decode (first press only — cached thereafter)
/// and the crop, then publishes the result under a fresh object URL. Stage
/// wall times land in `timings` on success, joining the pick-time entries
/// already there.
fn do_crop_now(
    loaded: LoadedImage,
    view: ViewTransform,
    stencil: Stencil,
    viewport: Size,
    mut cropped: Signal<Option<CroppedResult>>,
    mut error: Signal<Option<String>>,
    mut timings: Signal<StageTimings>,
) {
    // The one-and-only decode of the picked file, deferred from pick time
    // to here. A failure is NOT cached — the `OnceLock` stays empty, so
    // pressing Crop again retries the decode instead of replaying a stale
    // error. This is also where a header-valid file with a corrupt body
    // (which the pick-time probe cannot see) surfaces.
    let decode;
    let decoded = match loaded.decoded.get() {
        Some(cached) => {
            decode = DecodeTiming::Cached;
            cached
        }
        None => {
            let started = js_sys::Date::now();
            match DecodedSource::decode(&loaded.bytes) {
                Ok(fresh) => {
                    decode = DecodeTiming::Ran(js_sys::Date::now() - started);
                    loaded.decoded.get_or_init(|| fresh)
                }
                Err(e) => {
                    error.set(Some(format!("crop failed: {e}")));
                    return;
                }
            }
        }
    };

    let started = js_sys::Date::now();
    match crop_decoded_to_png(decoded, view, stencil, viewport) {
        Ok(result) => {
            let crop_ms = js_sys::Date::now() - started;

            let started = js_sys::Date::now();
            let url = match object_url::create(&result.png_bytes, "image/png") {
                Ok(url) => url,
                Err(e) => {
                    error.set(Some(format!("could not create a URL for the result: {e}")));
                    return;
                }
            };
            let url_ms = js_sys::Date::now() - started;

            // Replacement-time release: the result strip re-renders off the
            // old URL in the same pass that adopts the new one, and a blob
            // URL's revocation only blocks NEW fetches — the old thumbnail
            // stays painted for the instant it remains on screen.
            if let Some(previous) = cropped.peek().as_ref() {
                object_url::revoke(&previous.url);
            }
            cropped.set(Some(CroppedResult {
                url,
                width: result.width,
                height: result.height,
                size_bytes: result.png_bytes.len(),
            }));
            error.set(None);

            // Extends the pick-time read/probe entries rather than
            // replacing them — one line tells the whole story of the
            // current image.
            let mut t = *timings.peek();
            t.decode = Some(decode);
            t.crop_ms = Some(crop_ms);
            t.url_ms = Some(url_ms);
            timings.set(t);
        }
        Err(e) => error.set(Some(format!("crop failed: {e}"))),
    }
}

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    let mut image = use_signal(|| Option::<LoadedImage>::None);
    let mut view = use_signal(ViewTransform::default);
    let mut cropped = use_signal(|| Option::<CroppedResult>::None);
    let mut error = use_signal(|| Option::<String>::None);
    let mut timings = use_signal(StageTimings::default);

    let mut shape = use_signal(ShapeKind::default);
    let mut viewport_preset = use_signal(ViewportPreset::default);
    let mut dim_alpha_pct = use_signal(|| 50u32);
    let mut cursor = use_signal(CursorChoice::default);
    let mut pan_direction = use_signal(PanDirection::default);
    let mut restrict = use_signal(|| true);

    let vp_size = use_memo(move || viewport_preset().size());
    let stencil = use_memo(move || shape().stencil());

    // Re-clamps the current view against whatever shape/viewport/restriction
    // is live right now — called after every config-changing signal write so
    // an existing pan/zoom never renders out of range for the new config.
    let fix_view_for_config = use_callback(move |_: ()| {
        apply_view_edit(image, view, vp_size, stencil, restrict, |_vw, _natural| {});
    });

    let mut busy = use_signal(Busy::default);

    let on_file_change = move |evt: FormEvent| {
        // Re-entrancy guard: the `disabled` attribute on the input is the
        // visible affordance, but it only applies once a busy render has
        // landed — this check is the correctness mechanism regardless of
        // whether that render has happened yet. Also covers the input
        // firing `onchange` again (e.g. an OS file-manager quirk) while a
        // previous pick is still being read.
        if busy.peek().is_busy() {
            return;
        }
        let Some(file) = evt.files().into_iter().next() else {
            return;
        };
        let content_type = file
            .content_type()
            .unwrap_or_else(|| "image/png".to_string());
        let file_name = file.name();
        spawn(async move {
            busy.set(Busy::Loading);
            paint::wait_for_paint().await;

            // A failed read, header probe or URL construction leaves
            // whatever image, view and result were already loaded
            // unchanged — e.g. an OS-offered HEIC/AVIF/BMP/TIFF this
            // demo's `image` build has no reader for.
            let started = js_sys::Date::now();
            let bytes = match file.read_bytes().await {
                Ok(bytes) => bytes,
                Err(e) => {
                    error.set(Some(format!("could not read \"{file_name}\": {e}")));
                    busy.set(Busy::Idle);
                    return;
                }
            };
            let read_ms = js_sys::Date::now() - started;

            // Header-only probe: `into_dimensions` reads just enough of
            // the container to learn the pixel dimensions — the full
            // decode is deferred to the first Crop press. A file whose
            // header lies about its body still passes here; that surfaces
            // at crop time as a decode error through the same readout.
            let started = js_sys::Date::now();
            let dimensions = image::ImageReader::new(Cursor::new(&bytes))
                .with_guessed_format()
                .map_err(|e| e.to_string())
                .and_then(|reader| reader.into_dimensions().map_err(|e| e.to_string()));
            let (width, height) = match dimensions {
                Ok(dims) => dims,
                Err(e) => {
                    error.set(Some(format!(
                        "could not read the image header of \"{file_name}\": {e}"
                    )));
                    busy.set(Busy::Idle);
                    return;
                }
            };
            let probe_ms = js_sys::Date::now() - started;
            let natural_size = Size::new(width as f32, height as f32);

            let src_url = match object_url::create(&bytes, &content_type) {
                Ok(url) => url,
                Err(e) => {
                    error.set(Some(format!(
                        "could not create a URL for \"{file_name}\": {e}"
                    )));
                    busy.set(Busy::Idle);
                    return;
                }
            };

            // Replacement-time release of the previous pick's URLs: the
            // stage and result strip re-render off them in the same pass
            // that adopts the new image, and a blob URL's revocation only
            // blocks NEW fetches — the old image stays painted for the
            // instant it remains on screen. This root component never
            // unmounts, so replacement is the one moment they can be
            // released.
            if let Some(previous) = image.peek().as_ref() {
                object_url::revoke(&previous.src_url);
            }
            if let Some(previous) = cropped.peek().as_ref() {
                object_url::revoke(&previous.url);
            }

            image.set(Some(LoadedImage {
                file_name,
                src_url,
                bytes: Arc::from(bytes.as_ref()),
                natural_size,
                decoded: Arc::new(OnceLock::new()),
            }));
            view.set(fresh_view(natural_size, vp_size(), stencil(), restrict()));
            cropped.set(None);
            error.set(None);
            timings.set(StageTimings {
                read_ms: Some(read_ms),
                probe_ms: Some(probe_ms),
                ..StageTimings::default()
            });
            busy.set(Busy::Idle);
        });
    };

    // Required Cropper props — the component always wires mouse-drag and
    // wheel gestures to these, regardless of the button controls below.
    let on_pan = use_callback(move |delta: Point| {
        // The crate applies `pan_direction` to the delta before emitting
        // `on_pan` — applying it again here would double it.
        apply_view_edit(image, view, vp_size, stencil, restrict, |vw, _natural| {
            vw.offset = Point::new(vw.offset.x + delta.x, vw.offset.y + delta.y);
        });
    });
    let on_zoom = use_callback(move |delta: f32| {
        apply_view_edit(image, view, vp_size, stencil, restrict, |vw, _natural| {
            vw.zoom += delta * WHEEL_ZOOM_STEP;
        });
    });

    // `use_callback` (not a plain closure) so the same handler can be wired
    // to several buttons — `Callback` is `Copy`, a plain `FnMut` closure is
    // not and can only be moved into one `onclick`.
    let pan_by = use_callback(move |(dx, dy): (f32, f32)| {
        // Originates its own delta (a button press, not a drag), so it must
        // apply `pan_direction` itself — the crate only adjusts deltas it
        // emits from `on_pan`.
        apply_view_edit(image, view, vp_size, stencil, restrict, |vw, _natural| {
            let d = pan_direction().apply(Point::new(dx, dy));
            vw.offset = Point::new(vw.offset.x + d.x, vw.offset.y + d.y);
        });
    });
    let zoom_by = use_callback(move |step: f32| {
        apply_view_edit(image, view, vp_size, stencil, restrict, |vw, _natural| {
            vw.zoom += step;
        });
    });
    let toggle_pan_direction = use_callback(move |_: ()| {
        pan_direction.set(match pan_direction() {
            PanDirection::Image => PanDirection::Frame,
            PanDirection::Frame => PanDirection::Image,
        });
    });
    let rotate_by = use_callback(move |delta: f32| {
        apply_view_edit(image, view, vp_size, stencil, restrict, |vw, _natural| {
            vw.rotation = normalize_rotation(vw.rotation + delta);
        });
    });

    let reset_view = move |_| {
        apply_view_edit(image, view, vp_size, stencil, restrict, |vw, _natural| {
            // The neutral view: the floor-raise step that follows only
            // lifts this above `ViewTransform::default()`'s `zoom = 1.0`
            // if the current config's coverage/safety floor demands it —
            // it never treats the floor itself as the target.
            *vw = ViewTransform::default();
        });
    };

    let do_crop = move |_| {
        // Re-entrancy guard — see `on_file_change`'s comment. Repeated clicks
        // that land before the `disabled` render must not queue up multiple
        // crop runs.
        if busy.peek().is_busy() {
            return;
        }
        let Some(loaded) = image.read().clone() else {
            return;
        };
        let vp = vp_size();
        let st = stencil();
        let vw = view();
        spawn(async move {
            busy.set(Busy::Cropping);
            // Yields so a render lands with the button disabled and
            // relabelled before the synchronous decode (first press only) +
            // resample + PNG-encode work below runs and blocks the thread.
            paint::wait_for_paint().await;
            do_crop_now(loaded, vw, st, vp, cropped, error, timings);
            busy.set(Busy::Idle);
        });
    };

    let loaded = image.read().clone();
    let err = error.read().clone();
    let result = cropped.read().clone();

    let current_shape = shape();
    let current_viewport = viewport_preset();
    let current_view = view();
    let current_stencil = stencil();
    let current_viewport_size = vp_size();

    let source_dims = loaded.as_ref().map(|l| {
        format!(
            "{}\u{d7}{}",
            l.natural_size.width as u32, l.natural_size.height as u32
        )
    });
    // Computed on every render so the state is visible before the "Crop"
    // press rather than surfacing as an error after it — the library itself
    // rejects a predicted output over `MAX_OUTPUT_PIXELS`.
    let output_check = loaded.as_ref().map(|loaded_image| {
        output_size(
            loaded_image.natural_size,
            current_viewport_size,
            current_stencil,
            current_view.zoom,
        )
    });
    let output_dims = match &output_check {
        Some(Ok((out_w, out_h))) => format!("{out_w}\u{d7}{out_h}"),
        _ => "\u{2014}".to_string(),
    };
    let crop_blocked_reason = match &output_check {
        Some(Err(CropError::OutputTooLarge { width, height })) => Some(format!(
            "output would be {width}\u{d7}{height} px, over the {MAX_OUTPUT_PIXELS}-pixel limit — zoom in to shrink it"
        )),
        _ => None,
    };
    let zoom_pct_display = loaded
        .as_ref()
        .map(|_| format!("{:.0}%", current_view.zoom * 100.0));
    let rotation_display = loaded
        .as_ref()
        .map(|_| format!("{:.0}\u{b0}", current_view.rotation));
    let zoom_pct_control = format!("{:.0}%", current_view.zoom * 100.0);

    let file_name = loaded.as_ref().map(|l| l.file_name.clone());
    let stage_source = loaded.as_ref().map(|l| StageSource {
        src_url: l.src_url.clone(),
        natural_size: l.natural_size,
    });

    rsx! {
        document::Stylesheet { href: DEMO_CSS }
        document::Title { "dioxus-cropper demo" }

        div { class: "cr-page",
            div { class: "cr-header",
                h1 { class: "title-text", "dioxus-cropper demo" }
            }
            p { class: "cr-sub", "Pick an image and exercise every configurable prop of the Cropper component." }

            if let Some(message) = err {
                div { class: "error-box", style: "margin-bottom: 4px;",
                    span { class: "error-text", "{message}" }
                }
            }

            div { class: "cr-grid",
                div { class: "cr-stage-col",
                    div { class: "cr-stage-wrap",
                        Stage {
                            loaded: stage_source,
                            view: current_view,
                            stencil: current_stencil,
                            viewport: current_viewport_size,
                            dim_alpha: dim_alpha_pct() as f32 / 100.0,
                            cursor: cursor().cursor(),
                            pan_direction: pan_direction(),
                            on_pan,
                            on_zoom,
                        }
                        StageReadout {
                            source_dims,
                            output_dims,
                            zoom_pct: zoom_pct_display,
                            rotation_deg: rotation_display,
                        }
                        if let Some(line) = timing_line(&timings.read()) {
                            TimingReadout { line }
                        }
                    }

                    if loaded.is_some() {
                        button {
                            class: "btn btn-primary cr-crop-btn",
                            disabled: busy().is_busy() || crop_blocked_reason.is_some(),
                            onclick: do_crop,
                            IconCrop { size: 16 }
                            if busy() == Busy::Cropping { "Cropping\u{2026}" } else { "Crop" }
                        }
                        if let Some(reason) = &crop_blocked_reason {
                            span { class: "cr-crop-blocked", "{reason}" }
                        }
                    }

                    if let Some(result) = result {
                        ResultStrip {
                            url: result.url,
                            width: result.width,
                            height: result.height,
                            format: "PNG".to_string(),
                            size_label: format_size(result.size_bytes),
                            shape: current_stencil.shape(),
                        }
                    }
                }

                div { class: "cr-rail",
                    SourceGroup {
                        file_name,
                        loading: busy() == Busy::Loading,
                        on_pick: on_file_change,
                    }

                    if loaded.is_some() {
                        PositionGroup {
                            pan_direction: pan_direction(),
                            on_nudge: move |(ux, uy): (f32, f32)| {
                                pan_by.call((ux * PAN_STEP, uy * PAN_STEP));
                            },
                            on_toggle_pan_direction: move |_| toggle_pan_direction.call(()),
                        }
                        ZoomGroup {
                            zoom_pct: zoom_pct_control,
                            on_zoom_out: move |_| zoom_by.call(-ZOOM_STEP),
                            on_zoom_in: move |_| zoom_by.call(ZOOM_STEP),
                        }
                        RotateGroup { on_rotate: move |delta| rotate_by.call(delta) }
                        ShapeGroup {
                            active: current_shape,
                            on_select: move |s| {
                                shape.set(s);
                                fix_view_for_config.call(());
                            },
                            on_reset: reset_view,
                        }
                        TuningGroup {
                            viewport: current_viewport,
                            on_viewport: move |v| {
                                viewport_preset.set(v);
                                fix_view_for_config.call(());
                            },
                            dim_alpha_pct: dim_alpha_pct(),
                            on_dim_alpha_pct: move |pct| dim_alpha_pct.set(pct),
                            cursor: cursor(),
                            on_cursor: move |c| cursor.set(c),
                            restrict: restrict(),
                            on_restrict: move |on| {
                                restrict.set(on);
                                fix_view_for_config.call(());
                            },
                        }
                    }
                }
            }
        }
    }
}
