//! Inline SVG glyphs for the demo controls. 24x24 viewBox, `currentColor`
//! stroke, Feather-style line icons.

use dioxus::prelude::*;

#[component]
fn Glyph(size: u32, children: Element) -> Element {
    rsx! {
        svg {
            xmlns: "http://www.w3.org/2000/svg",
            view_box: "0 0 24 24",
            width: "{size}",
            height: "{size}",
            fill: "none",
            stroke: "currentColor",
            "stroke-width": "2",
            "stroke-linecap": "round",
            "stroke-linejoin": "round",
            {children}
        }
    }
}

#[component]
pub fn IconArrowUp(#[props(default = 18)] size: u32) -> Element {
    rsx! {
        Glyph { size,
            line { x1: "12", y1: "19", x2: "12", y2: "5" }
            polyline { points: "5 12 12 5 19 12" }
        }
    }
}

#[component]
pub fn IconArrowDown(#[props(default = 18)] size: u32) -> Element {
    rsx! {
        Glyph { size,
            line { x1: "12", y1: "5", x2: "12", y2: "19" }
            polyline { points: "19 12 12 19 5 12" }
        }
    }
}

#[component]
pub fn IconArrowLeft(#[props(default = 18)] size: u32) -> Element {
    rsx! {
        Glyph { size,
            line { x1: "19", y1: "12", x2: "5", y2: "12" }
            polyline { points: "12 19 5 12 12 5" }
        }
    }
}

#[component]
pub fn IconArrowRight(#[props(default = 18)] size: u32) -> Element {
    rsx! {
        Glyph { size,
            line { x1: "5", y1: "12", x2: "19", y2: "12" }
            polyline { points: "12 5 19 12 12 19" }
        }
    }
}

#[component]
pub fn IconCrop(#[props(default = 18)] size: u32) -> Element {
    rsx! {
        Glyph { size,
            path { d: "M6 2v14a2 2 0 0 0 2 2h14" }
            path { d: "M2 6h14a2 2 0 0 1 2 2v14" }
        }
    }
}

#[component]
pub fn IconDownload(#[props(default = 18)] size: u32) -> Element {
    rsx! {
        Glyph { size,
            path { d: "M12 3v12" }
            path { d: "M7 11l5 5 5-5" }
            path { d: "M4 20h16" }
        }
    }
}

#[component]
pub fn IconFrame(#[props(default = 18)] size: u32) -> Element {
    rsx! {
        Glyph { size,
            rect {
                x: "3",
                y: "3",
                width: "18",
                height: "18",
                rx: "2",
                "stroke-dasharray": "4 3",
            }
        }
    }
}

#[component]
pub fn IconImagePlaceholder(#[props(default = 18)] size: u32) -> Element {
    rsx! {
        Glyph { size,
            rect { x: "3", y: "3", width: "18", height: "18", rx: "2" }
            circle { cx: "8.5", cy: "8.5", r: "1.5", fill: "currentColor", stroke: "none" }
            path { d: "M21 15l-5-5L5 21" }
        }
    }
}

#[component]
pub fn IconResetView(#[props(default = 18)] size: u32) -> Element {
    rsx! {
        Glyph { size,
            path { d: "M3 12a9 9 0 1 0 2.6-6.4" }
            path { d: "M3 3v5h5" }
        }
    }
}

#[component]
pub fn IconRotateCcw(#[props(default = 18)] size: u32) -> Element {
    rsx! {
        Glyph { size,
            path { d: "M3 12a9 9 0 1 0 3-6.7L3 8" }
            path { d: "M3 3v5h5" }
        }
    }
}

#[component]
pub fn IconRotateCw(#[props(default = 18)] size: u32) -> Element {
    rsx! {
        Glyph { size,
            path { d: "M21 12a9 9 0 1 1-3-6.7L21 8" }
            path { d: "M21 3v5h-5" }
        }
    }
}

#[component]
pub fn IconShapeCircle(#[props(default = 18)] size: u32) -> Element {
    rsx! {
        Glyph { size,
            circle { cx: "12", cy: "12", r: "8" }
        }
    }
}

#[component]
pub fn IconShapeRectangle(#[props(default = 18)] size: u32) -> Element {
    rsx! {
        Glyph { size,
            rect { x: "3", y: "6", width: "18", height: "12", rx: "1.5" }
        }
    }
}

#[component]
pub fn IconShapeSquare(#[props(default = 18)] size: u32) -> Element {
    rsx! {
        Glyph { size,
            rect { x: "5", y: "5", width: "14", height: "14", rx: "1.5" }
        }
    }
}

#[component]
pub fn IconZoomIn(#[props(default = 18)] size: u32) -> Element {
    rsx! {
        Glyph { size,
            circle { cx: "11", cy: "11", r: "7" }
            line { x1: "21", y1: "21", x2: "16.65", y2: "16.65" }
            line { x1: "11", y1: "8", x2: "11", y2: "14" }
            line { x1: "8", y1: "11", x2: "14", y2: "11" }
        }
    }
}

#[component]
pub fn IconZoomOut(#[props(default = 18)] size: u32) -> Element {
    rsx! {
        Glyph { size,
            circle { cx: "11", cy: "11", r: "7" }
            line { x1: "21", y1: "21", x2: "16.65", y2: "16.65" }
            line { x1: "8", y1: "11", x2: "14", y2: "11" }
        }
    }
}
