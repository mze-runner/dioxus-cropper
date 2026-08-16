//! Browser demo for `dioxus-cropper`: pick a local image, then pan, zoom,
//! rotate and crop it. Every configurable prop of `Cropper` is exercised —
//! most from the rail on the right, `classes` wired to a visible cosmetic
//! hook in `Stage`. Plain Dioxus, one hand-written stylesheet, no network
//! requests — works offline.

mod components;
mod icons;
mod paint;
mod types;

use std::sync::Arc;

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
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
    StageSource, TuningGroup, ZoomGroup,
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

/// `data_uri` is `Arc<str>` and `decoded` is backed by an `Arc` internally,
/// so cloning this whole struct out of the `image` signal — the read
/// pattern every callback below uses — is a few refcount bumps plus one
/// heap allocation for `file_name: String`, not a copy of the picked file's
/// bytes or its base64 encoding.
#[derive(Clone)]
struct LoadedImage {
    file_name: String,
    /// `data:` URI built from the ORIGINAL file bytes, base64-encoded —
    /// never a re-encode of the decoded pixels.
    data_uri: Arc<str>,
    natural_size: Size,
    /// Decoded once and reused for every crop press, per the crate's own
    /// guidance (`DecodedSource::decode`'s doc comment).
    decoded: DecodedSource,
}

#[derive(Clone)]
struct CroppedResult {
    data_uri: Arc<str>,
    width: u32,
    height: u32,
    size_bytes: usize,
}

/// The demo's single busy state, covering both spans of synchronous,
/// CPU-bound work it drives: decoding a picked file and running the crop.
/// One signal for both — the file picker and the Crop button never run at
/// the same time, so there is only ever one thing to be busy with.
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

fn to_data_uri(mime: &str, bytes: &[u8]) -> Arc<str> {
    Arc::from(format!("data:{mime};base64,{}", STANDARD.encode(bytes)))
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

/// Runs the crop against the already-decoded source and writes the outcome.
fn do_crop_now(
    loaded: LoadedImage,
    view: ViewTransform,
    stencil: Stencil,
    viewport: Size,
    mut cropped: Signal<Option<CroppedResult>>,
    mut error: Signal<Option<String>>,
) {
    match crop_decoded_to_png(&loaded.decoded, view, stencil, viewport) {
        Ok(result) => {
            let data_uri = to_data_uri("image/png", &result.png_bytes);
            cropped.set(Some(CroppedResult {
                data_uri,
                width: result.width,
                height: result.height,
                size_bytes: result.png_bytes.len(),
            }));
            error.set(None);
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
        // previous pick is still decoding.
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

            // A failed read or decode leaves whatever image, view and result
            // were already loaded unchanged — e.g. an OS-offered
            // HEIC/AVIF/BMP/TIFF the crate's `image` build does not decode.
            let bytes = match file.read_bytes().await {
                Ok(bytes) => bytes,
                Err(e) => {
                    error.set(Some(format!("could not read \"{file_name}\": {e}")));
                    busy.set(Busy::Idle);
                    return;
                }
            };
            let decoded = match DecodedSource::decode(&bytes) {
                Ok(decoded) => decoded,
                Err(e) => {
                    error.set(Some(format!("could not decode \"{file_name}\": {e}")));
                    busy.set(Busy::Idle);
                    return;
                }
            };
            let natural_size = decoded.natural_size();
            let data_uri = to_data_uri(&content_type, &bytes);

            image.set(Some(LoadedImage {
                file_name,
                data_uri,
                natural_size,
                decoded,
            }));
            view.set(fresh_view(natural_size, vp_size(), stencil(), restrict()));
            cropped.set(None);
            error.set(None);
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
            // relabelled before the synchronous resample + PNG-encode work
            // below runs and blocks the thread.
            paint::wait_for_paint().await;
            do_crop_now(loaded, vw, st, vp, cropped, error);
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
        data_uri: l.data_uri.clone(),
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
                            data_uri: result.data_uri,
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
