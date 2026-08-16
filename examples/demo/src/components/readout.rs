//! The live readout line under the stage — source size, output size, zoom,
//! rotation. Pure props: every value arrives pre-formatted so this stays
//! markup-out only.

use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct StageReadoutProps {
    /// `None` before an image is loaded — rendered as an em dash, same as
    /// the empty-state stage itself.
    pub source_dims: Option<String>,
    pub output_dims: String,
    pub zoom_pct: Option<String>,
    pub rotation_deg: Option<String>,
}

#[component]
pub fn StageReadout(props: StageReadoutProps) -> Element {
    rsx! {
        div { class: "cr-readout",
            b { "{props.source_dims.as_deref().unwrap_or(\"\u{2014}\")}" }
            " source"
            span { class: "cr-sep", "\u{b7}" }
            b { "{props.output_dims}" }
            " output"
            if let Some(zoom) = props.zoom_pct {
                span { class: "cr-sep", "\u{b7}" }
                b { "{zoom}" }
                " zoom"
            }
            if let Some(rotation) = props.rotation_deg {
                span { class: "cr-sep", "\u{b7}" }
                b { "{rotation}" }
            }
        }
    }
}
