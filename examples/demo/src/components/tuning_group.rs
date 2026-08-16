//! "Tuning" control group — the remaining `Cropper` props the demo exists
//! to exercise: viewport footprint, dim opacity, cursor, and whether panning
//! /zoom is restricted to keep the stencil covered. Pan direction lives with
//! the position d-pad instead (it is semantically about what those arrows
//! do), so it is not part of this group.
//!
//! Rendered as a compact readout list — label left, value right-aligned in
//! mono — rather than a stack of full-width controls. Viewport, cursor and
//! restrict stay fully functional: each value is itself a button that
//! cycles/toggles to the next option, reachable by repeated activation.

use dioxus::prelude::*;

use crate::types::{CursorChoice, ViewportPreset};

#[derive(Props, Clone, PartialEq)]
pub struct TuningGroupProps {
    pub viewport: ViewportPreset,
    pub on_viewport: EventHandler<ViewportPreset>,
    pub dim_alpha_pct: u32,
    pub on_dim_alpha_pct: EventHandler<u32>,
    pub cursor: CursorChoice,
    pub on_cursor: EventHandler<CursorChoice>,
    pub restrict: bool,
    pub on_restrict: EventHandler<bool>,
}

fn next_viewport(current: ViewportPreset) -> ViewportPreset {
    let all = ViewportPreset::ALL;
    let idx = all.iter().position(|p| *p == current).unwrap_or(0);
    all[(idx + 1) % all.len()]
}

fn next_cursor(current: CursorChoice) -> CursorChoice {
    let all = CursorChoice::ALL;
    let idx = all.iter().position(|c| *c == current).unwrap_or(0);
    all[(idx + 1) % all.len()]
}

#[component]
pub fn TuningGroup(props: TuningGroupProps) -> Element {
    rsx! {
        div { class: "cr-group",
            span { class: "cr-glabel", "Tuning" }

            button {
                class: "cr-tuning-row cr-tuning-row-btn",
                "aria-label": "Viewport size: {props.viewport.label()}. Activate to change.",
                onclick: move |_| props.on_viewport.call(next_viewport(props.viewport)),
                span { class: "cr-lab", "Viewport" }
                span { class: "cr-tune-val", "{props.viewport.label()}" }
            }

            div { class: "cr-tuning-row",
                span { class: "cr-lab", "Dim" }
                span { class: "cr-tune-val", "{props.dim_alpha_pct}%" }
            }
            input {
                class: "cr-range",
                r#type: "range",
                min: "0",
                max: "100",
                "aria-label": "Dim opacity",
                value: "{props.dim_alpha_pct}",
                oninput: move |evt| {
                    if let Ok(pct) = evt.value().parse::<u32>() {
                        props.on_dim_alpha_pct.call(pct);
                    }
                },
            }

            button {
                class: "cr-tuning-row cr-tuning-row-btn",
                "aria-label": "Cursor: {props.cursor.label()}. Activate to change.",
                onclick: move |_| props.on_cursor.call(next_cursor(props.cursor)),
                span { class: "cr-lab", "Cursor" }
                span { class: "cr-tune-val", "{props.cursor.label()}" }
            }

            button {
                class: "cr-tuning-row cr-tuning-row-btn",
                "aria-label": if props.restrict { "Position restriction: on. Activate to turn off." } else { "Position restriction: off. Activate to turn on." },
                onclick: move |_| props.on_restrict.call(!props.restrict),
                span { class: "cr-lab", "Restrict" }
                span {
                    class: if props.restrict { "cr-tune-val cr-tune-val-on" } else { "cr-tune-val" },
                    if props.restrict { "On" } else { "Off" }
                }
            }
        }
    }
}
