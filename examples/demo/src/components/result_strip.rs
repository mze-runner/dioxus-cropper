//! The result strip below the stage — thumbnail, dimensions, format, and
//! the download action: a download link in the browser, a save-to-Downloads
//! button on desktop. Rendered only once a crop has been produced.

use std::sync::Arc;

use dioxus::prelude::*;
use dioxus_cropper::geometry::StencilShape;

use crate::icons::IconDownload;

#[derive(Props, Clone, PartialEq)]
pub struct ResultStripProps {
    /// Image URL over the crop's encoded bytes — the thumbnail's `src` and
    /// the download link's `href`. The caller owns its revocation.
    pub url: Arc<str>,
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub size_label: String,
    /// The filename the download link saves as — extension matching
    /// `format`.
    pub download_name: String,
    /// The stencil shape the crop was taken with. `crop_in_browser` returns a
    /// circle's unmasked square bounding box, for the caller to round with
    /// CSS at display time — this is that rounding.
    pub shape: StencilShape,
    /// Desktop only: the path the result was last saved to.
    pub saved_to: Option<String>,
    /// Desktop only: saves the result to the Downloads folder.
    pub on_save: EventHandler<()>,
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
                src: "{props.url}",
                alt: "Cropped result",
            }
            div { class: "cr-result-meta",
                span { class: "section-title", "Result" }
                span { class: "cr-result-dims",
                    "{props.width} \u{d7} {props.height} \u{b7} {props.format} \u{b7} {props.size_label}"
                }
            }
            span { style: "flex: 1;" }
            {download_action(&props)}
        }
    }
}

/// A plain `<a download>` — the browser saves an object URL's blob
/// natively, no JS/web_sys anchor-click dance needed.
#[cfg(target_arch = "wasm32")]
fn download_action(props: &ResultStripProps) -> Element {
    rsx! {
        a {
            class: "cr-ctrl",
            style: "width: auto; padding: 0 12px; text-decoration: none;",
            href: "{props.url}",
            "download": "{props.download_name}",
            IconDownload { size: 15 }
            "Download"
        }
    }
}

/// A save button plus the last saved path.
#[cfg(not(target_arch = "wasm32"))]
fn download_action(props: &ResultStripProps) -> Element {
    let on_save = props.on_save;
    rsx! {
        if let Some(path) = &props.saved_to {
            span { class: "cr-result-dims", style: "margin-right: 8px;", "Saved to {path}" }
        }
        button {
            class: "cr-ctrl",
            style: "width: auto; padding: 0 12px;",
            onclick: move |_| on_save.call(()),
            IconDownload { size: 15 }
            "Save to Downloads"
        }
    }
}
