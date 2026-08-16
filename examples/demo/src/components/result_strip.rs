//! The result strip below the stage — thumbnail, dimensions, format, and
//! the download action. Rendered only once a crop has been produced.

use std::sync::Arc;

use dioxus::prelude::*;
use dioxus_cropper::geometry::StencilShape;

use crate::icons::IconDownload;

#[derive(Props, Clone, PartialEq)]
pub struct ResultStripProps {
    pub data_uri: Arc<str>,
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub size_label: String,
    /// The stencil shape the crop was taken with. `crop_decoded_to_png`
    /// returns a circle's unmasked square bounding box, for the caller to
    /// round with CSS at display time — this is that rounding.
    pub shape: StencilShape,
}

#[component]
pub fn ResultStrip(props: ResultStripProps) -> Element {
    let border_radius = match props.shape {
        StencilShape::Circle => "50%",
        StencilShape::Rectangle | StencilShape::Square => "6px",
    };

    rsx! {
        div { class: "cr-result",
            img {
                class: "cr-thumb",
                style: "border-radius: {border_radius}; object-fit: contain;",
                src: "{props.data_uri}",
                alt: "Cropped result",
            }
            div { class: "cr-result-meta",
                span { class: "section-title", "Result" }
                span { class: "cr-result-dims",
                    "{props.width} \u{d7} {props.height} \u{b7} {props.format} \u{b7} {props.size_label}"
                }
            }
            span { style: "flex: 1;" }
            // A plain `<a download>` — the browser handles saving a `data:`
            // URI natively, no JS/web_sys anchor-click dance needed.
            a {
                class: "cr-ctrl",
                style: "width: auto; padding: 0 12px; text-decoration: none;",
                href: "{props.data_uri}",
                "download": "cropped-image.png",
                IconDownload { size: 15 }
                "Download"
            }
        }
    }
}
