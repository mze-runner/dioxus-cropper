//! The cropper stage — a single element that swaps between the empty-state
//! frame and the mounted `Cropper`, both framed by the same registration
//! corners. Pure props: no signal reads, no business logic — the caller
//! owns `view` and folds gesture deltas back into it.

use std::sync::Arc;

use dioxus::prelude::*;
use dioxus_cropper::geometry::{Point, Size, Stencil, ViewTransform};
use dioxus_cropper::{Cropper, CropperClasses, CropperCursor, PanDirection};

use crate::icons::IconImagePlaceholder;

/// The picked image's renderable source, as `Stage` needs it — a data URI
/// plus the decoded natural size `Cropper` requires for its own fit maths.
/// `data_uri` is `Arc<str>` so passing it down each render is a refcount
/// bump, not a copy of the base64 string.
#[derive(Debug, Clone, PartialEq)]
pub struct StageSource {
    pub data_uri: Arc<str>,
    pub natural_size: Size,
}

#[derive(Props, Clone, PartialEq)]
pub struct StageProps {
    pub loaded: Option<StageSource>,
    pub view: ViewTransform,
    pub stencil: Stencil,
    pub viewport: Size,
    pub dim_alpha: f32,
    pub cursor: CropperCursor,
    pub pan_direction: PanDirection,
    pub on_pan: EventHandler<Point>,
    pub on_zoom: EventHandler<f32>,
}

#[component]
pub fn Stage(props: StageProps) -> Element {
    let w = props.viewport.width;
    let h = props.viewport.height;

    rsx! {
        div {
            class: "cr-stage",
            style: "width: {w}px; height: {h}px;",

            if let Some(source) = props.loaded {
                Cropper {
                    src: source.data_uri,
                    natural_size: source.natural_size,
                    view: props.view,
                    stencil: props.stencil,
                    viewport: props.viewport,
                    dim_alpha: props.dim_alpha,
                    cursor: props.cursor,
                    pan_direction: props.pan_direction,
                    // Demonstrates the `classes` prop reaching the rendered
                    // DOM — `demo.css`'s `.cr-cropper-viewport` rule is the
                    // visible effect of this class hook.
                    classes: CropperClasses {
                        viewport: "cr-cropper-viewport".to_string(),
                        ..Default::default()
                    },
                    on_pan: props.on_pan,
                    on_zoom: props.on_zoom,
                }
            } else {
                div { class: "cr-empty",
                    IconImagePlaceholder { size: 30 }
                    p { "No image loaded" }
                    span { class: "cr-hint", "Choose a file to begin" }
                }
            }

            div { class: "cr-marks",
                div {}
                div {}
                div {}
                div {}
            }
        }
    }
}
