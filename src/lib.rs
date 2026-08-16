//! `dioxus-cropper` — a host-agnostic, pure-Rust image cropper component for
//! Dioxus.
//!
//! The component takes a ready-to-render image source (a URL or a data URI)
//! and renders it inside a fixed, clipped viewport, positioned by a
//! caller-controlled [`geometry::ViewTransform`] (offset, zoom, rotation). A
//! [`geometry::Stencil`] marks the crop window inside the viewport, dimmed
//! everywhere else. [`crop::crop_to_png`] turns the same view and stencil
//! into the actual cropped output.
//!
//! `Cropper` is a controlled renderer: it owns no positional state of its
//! own. `offset`, `zoom` and `rotation` are read fresh from `view` on every
//! render, and pan/zoom gestures are reported to the caller via `on_pan`/
//! `on_zoom` rather than applied internally. [`geometry::min_zoom_to_cover`]
//! and [`geometry::clamp_offset`] let a caller keep the stencil fully
//! covered by the image as it applies those deltas.
//!
//! # Controlled state
//!
//! The host owns a [`ViewTransform`] in its own state, passes it to
//! [`Cropper`] as `view`, and folds `on_pan`/`on_zoom` deltas back into it —
//! [`geometry::clamp_offset`] keeps the pan within the stencil-coverage
//! bound for the current zoom:
//!
//! ```
//! use dioxus::prelude::*;
//! use dioxus_cropper::geometry::{clamp_offset, contain_scale, Point, Size, Stencil, ViewTransform};
//! use dioxus_cropper::Cropper;
//!
//! fn app() -> Element {
//!     let natural_size = Size::new(1920.0, 1080.0);
//!     let viewport = Size::new(480.0, 480.0);
//!     let stencil = Stencil::square(240.0);
//!     let fit_scale = contain_scale(natural_size, viewport);
//!
//!     let mut view = use_signal(ViewTransform::default);
//!
//!     rsx! {
//!         Cropper {
//!             src: "data:image/png;base64,",
//!             natural_size,
//!             view: view(),
//!             stencil,
//!             viewport,
//!             on_pan: move |delta: Point| {
//!                 let mut v = view.write();
//!                 let panned = Point::new(v.offset.x + delta.x, v.offset.y + delta.y);
//!                 v.offset = clamp_offset(panned, natural_size, fit_scale, v.zoom, v.rotation, stencil);
//!             },
//!             on_zoom: move |delta: f32| {
//!                 let mut v = view.write();
//!                 v.zoom = (v.zoom + delta * 0.001).max(1.0);
//!             },
//!         }
//!     }
//! }
//!
//! // Rendered here with `dioxus-ssr` to keep the example self-contained and
//! // runnable; a host normally mounts `app` through its own renderer.
//! let mut dom = VirtualDom::new(app);
//! dom.rebuild_in_place();
//! let html = dioxus_ssr::render(&dom);
//! assert!(html.contains("overflow: hidden"));
//! ```
#![warn(missing_docs)]

use dioxus::html::input_data::MouseButton;
use dioxus::prelude::*;
use std::sync::Arc;

pub mod crop;
pub mod geometry;
pub use crop::{
    crop_decoded_to_png, crop_to_png, output_size, CropError, CroppedImage, DecodedSource,
    MAX_OUTPUT_PIXELS,
};
pub use geometry::{
    clamp_offset, contain_scale, min_zoom_to_cover, normalize_rotation, rotated_bounding_box,
    Point, Size, Stencil, StencilShape, ViewTransform,
};

/// The mouse cursor `Cropper` shows over its viewport while idle and while
/// dragging.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum CropperCursor {
    /// The four-direction "move" cursor. The default.
    #[default]
    Move,
    /// The classic open-hand "grab" cursor.
    Grab,
    /// `all-scroll` — also four arrows.
    AllScroll,
    /// A crosshair.
    Crosshair,
    /// No `cursor` declaration at all — lets a host style the cursor itself.
    None,
    /// A raw CSS `cursor` value, passed through verbatim.
    Custom(String),
}

impl CropperCursor {
    /// The CSS `cursor` declaration this variant renders as, or `None` for
    /// [`CropperCursor::None`] — omitted entirely rather than rendered empty.
    fn css_declaration(&self) -> Option<String> {
        let value = match self {
            Self::Move => "move",
            Self::Grab => "grab",
            Self::AllScroll => "all-scroll",
            Self::Crosshair => "crosshair",
            Self::None => return None,
            Self::Custom(value) => value,
        };
        Some(format!("cursor: {value};"))
    }
}

/// Which of the two things "reachable by a pan gesture" a delta is read as
/// moving: the image, or the framing around it. Interaction semantics, not
/// geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PanDirection {
    /// A pan moves the image. The default.
    #[default]
    Image,
    /// A pan moves the framing; the image slides the opposite way.
    Frame,
}

impl PanDirection {
    /// Adjusts a raw pan delta to this direction's convention: unchanged for
    /// [`PanDirection::Image`], negated on both axes for
    /// [`PanDirection::Frame`].
    pub fn apply(self, delta: Point) -> Point {
        match self {
            Self::Image => delta,
            Self::Frame => Point::new(-delta.x, -delta.y),
        }
    }
}

/// Cosmetic class hooks a host attaches to the elements [`Cropper`] renders.
/// Every field is empty by default; this crate never applies a class of its
/// own and ships no CSS.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CropperClasses {
    /// The outer, fixed-size clipped viewport `div`.
    pub viewport: String,
    /// The `<img>` being positioned inside the viewport.
    pub image: String,
    /// The stencil overlay `div` (the dimmed mask plus its clear window).
    pub stencil: String,
}

/// Ephemeral mouse-drag bookkeeping, private to [`Cropper`]. Holds only the
/// pointer's previous client-space position, and never mirrors `offset`,
/// `zoom` or `rotation`, which stay entirely the caller's.
#[derive(Debug, Clone, Copy, PartialEq)]
enum DragState {
    /// No drag in progress.
    Idle,
    /// A drag is in progress; `last` is the pointer's client-space position
    /// as of the most recently handled `mousedown`/`mousemove`.
    Dragging { last: Point },
}

/// Renders `src` inside a fixed, clipped viewport, positioned by `view`
/// (offset from centre, zoom, and rotation), with `stencil` masked out as
/// the crop window and everything else dimmed by `dim_alpha` (`0.0`
/// transparent, `1.0` opaque black; out-of-range values are clamped). The
/// viewport clips with `overflow: hidden`.
///
/// `natural_size` must be `src`'s real decoded pixel dimensions (see
/// [`crop::DecodedSource::decode`]). At `zoom = 1.0` the image is fit to the
/// viewport via `object-fit: contain` semantics ([`geometry::contain_scale`]);
/// `zoom` then scales up from that fit, not from natural size. The `<img>`'s
/// `width`/`height`/`max-width`/`max-height` are set inline so a host
/// stylesheet reset cannot resize the element out from under this fit.
///
/// `on_pan` and `on_zoom` are raw gesture deltas, not a new `ViewTransform` —
/// applying them (adding to the caller's `offset`, running the result
/// through `clamp_offset`/`min_zoom_to_cover`) is the caller's job, not this
/// component's. `on_pan` fires once per `mousemove` while a drag is in
/// progress, with the delta already adjusted for `pan_direction` — the
/// caller always just adds it to `offset`, with no conditional negation on
/// the receiving side regardless of which `pan_direction` is configured.
/// `on_zoom` fires once per `wheel` event with the vertical component of the
/// browser's `WheelEvent.deltaY`, sign flipped so a positive value means
/// "zoom in"; magnitude is passed through exactly as the browser reported it
/// (pixels, lines, or pages depending on the browser) — a caller scaling the
/// raw value into a zoom step owns that calibration.
///
/// `classes` attaches host-supplied cosmetic classes to the viewport, image
/// and stencil elements; nothing about the transform, clipping, or pointer
/// handling is reachable through them. The viewport also carries a
/// `data-dragging="true"/"false"` attribute so a host can write
/// state-dependent cosmetic CSS.
///
/// `viewport` is the fixed footprint this component renders at, defaulting
/// to `Size::new(480.0, 480.0)`. Whatever value is passed here MUST be
/// passed again, unchanged, to
/// [`crop::crop_to_png`]/[`crop::crop_decoded_to_png`]'s own required
/// `viewport` parameter for any `view` rendered against it.
#[component]
pub fn Cropper(
    #[props(into)] src: Arc<str>,
    natural_size: Size,
    view: ViewTransform,
    stencil: Stencil,
    #[props(default = Size::new(480.0, 480.0))] viewport: Size,
    #[props(default = 0.5)] dim_alpha: f32,
    #[props(default)] cursor: CropperCursor,
    #[props(default)] pan_direction: PanDirection,
    #[props(default)] classes: CropperClasses,
    on_pan: EventHandler<Point>,
    on_zoom: EventHandler<f32>,
) -> Element {
    let ViewTransform {
        offset,
        zoom,
        rotation,
    } = view;

    let alpha = dim_alpha.clamp(0.0, 1.0);
    let border_radius = match stencil.shape() {
        StencilShape::Circle => "50%",
        StencilShape::Rectangle | StencilShape::Square => "0",
    };

    let fit_scale = contain_scale(natural_size, viewport);
    let scale = fit_scale * zoom;

    let mut drag = use_signal(|| DragState::Idle);
    let dragging = matches!(*drag.read(), DragState::Dragging { .. });
    let cursor_declaration = cursor.css_declaration().unwrap_or_default();

    rsx! {
        div {
            class: "{classes.viewport}",
            "data-dragging": "{dragging}",
            style: "width: {viewport.width}px; height: {viewport.height}px; overflow: hidden; position: relative; {cursor_declaration}",
            onmousedown: move |e: MouseEvent| {
                if e.trigger_button() == Some(MouseButton::Primary) {
                    let c = e.client_coordinates();
                    drag.set(DragState::Dragging {
                        last: Point::new(c.x as f32, c.y as f32),
                    });
                }
            },
            onwheel: move |e: WheelEvent| {
                // The event listener is not registered passive, so
                // `prevent_default` suppresses the page's own scroll.
                e.prevent_default();
                let dy = e.delta().strip_units().y as f32;
                on_zoom.call(-dy);
            },
            img {
                class: "{classes.image}",
                src: "{src}",
                draggable: false,
                style: "position: absolute; top: 50%; left: 50%; width: {natural_size.width}px; height: {natural_size.height}px; max-width: none; max-height: none; user-select: none; -webkit-user-drag: none; transform: translate(calc(-50% + {offset.x}px), calc(-50% + {offset.y}px)) rotate({rotation}deg) scale({scale});",
                ondragstart: move |e: DragEvent| e.prevent_default(),
            }
            div {
                class: "{classes.stencil}",
                style: "position: absolute; top: 50%; left: 50%; width: {stencil.width()}px; height: {stencil.height()}px; transform: translate(-50%, -50%); border-radius: {border_radius}; box-shadow: 0 0 0 9999px rgba(0, 0, 0, {alpha}); pointer-events: none;",
            }
        }

        // A full-viewport capture layer, present only while dragging, since
        // there is no pointer-capture API available here: `mouseup`/
        // `mouseleave` on this layer end the drag from anywhere on the page,
        // and a `mousemove` that arrives with the primary button no longer
        // held (released outside the window) ends it too, rather than
        // continuing to pan on stale input.
        if dragging {
            div {
                style: "position: fixed; inset: 0; z-index: 2147483647; {cursor_declaration}",
                onmousemove: move |e: MouseEvent| {
                    let DragState::Dragging { last } = *drag.read() else {
                        return;
                    };
                    if !e.held_buttons().contains(MouseButton::Primary) {
                        drag.set(DragState::Idle);
                        return;
                    }
                    let c = e.client_coordinates();
                    let current = Point::new(c.x as f32, c.y as f32);
                    let raw_delta = Point::new(current.x - last.x, current.y - last.y);
                    on_pan.call(pan_direction.apply(raw_delta));
                    drag.set(DragState::Dragging { last: current });
                },
                onmouseup: move |_| drag.set(DragState::Idle),
                onmouseleave: move |_| drag.set(DragState::Idle),
            }
        }
    }
}
