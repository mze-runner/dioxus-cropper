//! "Rotate" control group — four buttons, +-45 deg and +-90 deg. Each shows
//! its amount alongside the glyph since the icon alone cannot distinguish
//! them.

use dioxus::prelude::*;

use crate::icons::{IconRotateCcw, IconRotateCw};

#[derive(Props, Clone, PartialEq)]
pub struct RotateGroupProps {
    pub on_rotate: EventHandler<f32>,
}

#[component]
pub fn RotateGroup(props: RotateGroupProps) -> Element {
    rsx! {
        div { class: "cr-group",
            span { class: "cr-glabel", "Rotate" }
            div { class: "cr-row-4",
                button {
                    class: "cr-ctrl",
                    "aria-label": "Rotate 90 degrees left",
                    onclick: move |_| props.on_rotate.call(-90.0),
                    IconRotateCcw { size: 14 }
                    span { class: "cr-lab", "90" }
                }
                button {
                    class: "cr-ctrl",
                    "aria-label": "Rotate 45 degrees left",
                    onclick: move |_| props.on_rotate.call(-45.0),
                    IconRotateCcw { size: 14 }
                    span { class: "cr-lab", "45" }
                }
                button {
                    class: "cr-ctrl",
                    "aria-label": "Rotate 45 degrees right",
                    onclick: move |_| props.on_rotate.call(45.0),
                    IconRotateCw { size: 14 }
                    span { class: "cr-lab", "45" }
                }
                button {
                    class: "cr-ctrl",
                    "aria-label": "Rotate 90 degrees right",
                    onclick: move |_| props.on_rotate.call(90.0),
                    IconRotateCw { size: 14 }
                    span { class: "cr-lab", "90" }
                }
            }
        }
    }
}
