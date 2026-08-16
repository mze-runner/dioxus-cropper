//! "Zoom" control group — minus / live percentage / plus.

use dioxus::prelude::*;

use crate::icons::{IconZoomIn, IconZoomOut};

#[derive(Props, Clone, PartialEq)]
pub struct ZoomGroupProps {
    pub zoom_pct: String,
    pub on_zoom_out: EventHandler<()>,
    pub on_zoom_in: EventHandler<()>,
}

#[component]
pub fn ZoomGroup(props: ZoomGroupProps) -> Element {
    rsx! {
        div { class: "cr-group",
            span { class: "cr-glabel", "Zoom" }
            div { class: "cr-zoomrow",
                button {
                    class: "cr-ctrl",
                    "aria-label": "Zoom out",
                    onclick: move |_| props.on_zoom_out.call(()),
                    IconZoomOut { size: 15 }
                }
                span { class: "cr-zoomval", "{props.zoom_pct}" }
                button {
                    class: "cr-ctrl",
                    "aria-label": "Zoom in",
                    onclick: move |_| props.on_zoom_in.call(()),
                    IconZoomIn { size: 15 }
                }
            }
        }
    }
}
