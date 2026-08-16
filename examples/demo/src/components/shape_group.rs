//! "Shape" control group — the stencil-shape selector plus "Reset view".

use dioxus::prelude::*;

use crate::icons::{IconResetView, IconShapeCircle, IconShapeRectangle, IconShapeSquare};
use crate::types::ShapeKind;

#[derive(Props, Clone, PartialEq)]
pub struct ShapeGroupProps {
    pub active: ShapeKind,
    pub on_select: EventHandler<ShapeKind>,
    pub on_reset: EventHandler<()>,
}

#[component]
pub fn ShapeGroup(props: ShapeGroupProps) -> Element {
    let on_class = |shape: ShapeKind| {
        if props.active == shape {
            "cr-ctrl cr-ctrl-on"
        } else {
            "cr-ctrl"
        }
    };

    rsx! {
        div { class: "cr-group",
            span { class: "cr-glabel", "Shape" }
            div { class: "cr-row-3",
                button {
                    class: "{on_class(ShapeKind::Rectangle)}",
                    "aria-label": "{ShapeKind::Rectangle.label()}",
                    onclick: move |_| props.on_select.call(ShapeKind::Rectangle),
                    IconShapeRectangle { size: 15 }
                }
                button {
                    class: "{on_class(ShapeKind::Square)}",
                    "aria-label": "{ShapeKind::Square.label()}",
                    onclick: move |_| props.on_select.call(ShapeKind::Square),
                    IconShapeSquare { size: 15 }
                }
                button {
                    class: "{on_class(ShapeKind::Circle)}",
                    "aria-label": "{ShapeKind::Circle.label()}",
                    onclick: move |_| props.on_select.call(ShapeKind::Circle),
                    IconShapeCircle { size: 15 }
                }
            }
            button {
                class: "cr-ctrl cr-ctrl-wide",
                onclick: move |_| props.on_reset.call(()),
                IconResetView { size: 15 }
                "Reset view"
            }
        }
    }
}
