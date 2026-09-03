//! "Output" control group — segmented selectors for the crop's size target
//! and byte format. These feed `crop_decoded`'s `CropOutput` at crop time;
//! neither affects the view transform.

use dioxus::prelude::*;

use crate::types::{FormatChoice, SizeChoice};

#[derive(Props, Clone, PartialEq)]
pub struct OutputGroupProps {
    pub size: SizeChoice,
    pub on_size: EventHandler<SizeChoice>,
    pub format: FormatChoice,
    pub on_format: EventHandler<FormatChoice>,
}

#[component]
pub fn OutputGroup(props: OutputGroupProps) -> Element {
    rsx! {
        div { class: "cr-group",
            span { class: "cr-glabel", "Output" }
            span { class: "cr-lab", "Size" }
            div { class: "cr-row-3",
                for choice in SizeChoice::ALL {
                    button {
                        class: if props.size == choice { "cr-ctrl cr-ctrl-on" } else { "cr-ctrl" },
                        "aria-label": "Output size: {choice.label()}",
                        onclick: move |_| props.on_size.call(choice),
                        "{choice.label()}"
                    }
                }
            }
            span { class: "cr-lab", "Format" }
            div { class: "cr-row-2",
                for choice in FormatChoice::ALL {
                    button {
                        class: if props.format == choice { "cr-ctrl cr-ctrl-on" } else { "cr-ctrl" },
                        "aria-label": "Output format: {choice.label()}",
                        onclick: move |_| props.on_format.call(choice),
                        "{choice.label()}"
                    }
                }
            }
        }
    }
}
