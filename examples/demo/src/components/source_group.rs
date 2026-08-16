//! "Source" control group — the file picker, plus the picked file's name
//! once one is loaded.

use dioxus::prelude::*;

use crate::icons::IconImagePlaceholder;

#[derive(Props, Clone, PartialEq)]
pub struct SourceGroupProps {
    pub file_name: Option<String>,
    /// True while the picked file's bytes are being read and decoded. The
    /// input is disabled and relabelled for the duration — the caller owns
    /// the busy state, this component only renders it.
    pub loading: bool,
    pub on_pick: EventHandler<FormEvent>,
}

#[component]
pub fn SourceGroup(props: SourceGroupProps) -> Element {
    let label = if props.loading {
        "Loading\u{2026}"
    } else if props.file_name.is_some() {
        "Change image"
    } else {
        "Choose image"
    };

    rsx! {
        div { class: "cr-group",
            span { class: "cr-glabel", "Source" }
            // The input is nested inside its own `label` and visually
            // hidden rather than `display: none` — it stays focusable and
            // in the tab order, which is the only way a keyboard user can
            // reach the file picker.
            label { class: "cr-ctrl cr-ctrl-wide", r#for: "cr-file-input",
                input {
                    id: "cr-file-input",
                    r#type: "file",
                    accept: "image/*",
                    class: "cr-visually-hidden",
                    disabled: props.loading,
                    onchange: move |evt| props.on_pick.call(evt),
                }
                IconImagePlaceholder { size: 15 }
                "{label}"
            }
            if let Some(name) = props.file_name {
                span { class: "cr-filename", "{name}" }
            }
        }
    }
}
