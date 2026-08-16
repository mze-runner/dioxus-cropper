//! "Position" control group — a 3x3 d-pad whose centre cell is the
//! `PanDirection` toggle. The pixel step per arrow press is the caller's
//! concern; this component only reports which direction was pressed and
//! when the centre toggle was activated.
//!
//! The centre glyph never reads as "recentre": it shows the CURRENT pan mode
//! (an image glyph for `Image`, a frame glyph for `Frame`) rather than
//! anything directional or reset-like, and a caption underneath states the
//! mode in words. The accessible name states both the current mode and what
//! activating the button does.

use dioxus::prelude::*;
use dioxus_cropper::PanDirection;

use crate::icons::{
    IconArrowDown, IconArrowLeft, IconArrowRight, IconArrowUp, IconFrame, IconImagePlaceholder,
};

#[derive(Props, Clone, PartialEq)]
pub struct PositionGroupProps {
    pub pan_direction: PanDirection,
    /// A unit direction, e.g. `(0.0, -1.0)` for "up" — the caller scales it
    /// by its own pixel step.
    pub on_nudge: EventHandler<(f32, f32)>,
    pub on_toggle_pan_direction: EventHandler<()>,
}

#[component]
pub fn PositionGroup(props: PositionGroupProps) -> Element {
    let (mode_label, other_label) = match props.pan_direction {
        PanDirection::Image => ("image", "frame"),
        PanDirection::Frame => ("frame", "image"),
    };
    let toggle_label =
        format!("Panning moves the {mode_label}. Activate to switch to moving the {other_label}.");

    rsx! {
        div { class: "cr-group",
            span { class: "cr-glabel", "Position" }
            div { class: "cr-dpad",
                span {}
                button {
                    class: "cr-ctrl",
                    "aria-label": "Move up",
                    onclick: move |_| props.on_nudge.call((0.0, -1.0)),
                    IconArrowUp { size: 14 }
                }
                span {}
                button {
                    class: "cr-ctrl",
                    "aria-label": "Move left",
                    onclick: move |_| props.on_nudge.call((-1.0, 0.0)),
                    IconArrowLeft { size: 14 }
                }
                button {
                    class: "cr-ctrl cr-ctrl-ghost",
                    "aria-label": "{toggle_label}",
                    onclick: move |_| props.on_toggle_pan_direction.call(()),
                    match props.pan_direction {
                        PanDirection::Image => rsx! {
                            IconImagePlaceholder { size: 13 }
                        },
                        PanDirection::Frame => rsx! {
                            IconFrame { size: 13 }
                        },
                    }
                }
                button {
                    class: "cr-ctrl",
                    "aria-label": "Move right",
                    onclick: move |_| props.on_nudge.call((1.0, 0.0)),
                    IconArrowRight { size: 14 }
                }
                span {}
                button {
                    class: "cr-ctrl",
                    "aria-label": "Move down",
                    onclick: move |_| props.on_nudge.call((0.0, 1.0)),
                    IconArrowDown { size: 14 }
                }
                span {}
            }
            span { class: "cr-cap", "Moving "
                b { "{mode_label}" }
            }
        }
    }
}
