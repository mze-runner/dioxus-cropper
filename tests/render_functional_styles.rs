#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Locks in the *functional* inline styles `Cropper` sets on its rendered
//! elements — styles the component breaks without. Every pure-function test
//! in this crate would still pass if a refactor dropped any of these; only a
//! render-level assertion over the markup catches that.
//!
//! Builds a `VirtualDom` directly (no router/context needed — `Cropper`
//! takes no context), `rebuild_in_place`, then asserts over
//! `dioxus_ssr::render` output.

use dioxus::prelude::*;
use dioxus_cropper::geometry::{Point, Size, Stencil, ViewTransform};
use dioxus_cropper::Cropper;

fn root() -> Element {
    rsx! {
        Cropper {
            src: "data:image/png;base64,",
            natural_size: Size::new(200.0, 100.0),
            view: ViewTransform::default(),
            stencil: Stencil::square(150.0),
            on_pan: move |_: Point| {},
            on_zoom: move |_: f32| {},
        }
    }
}

fn render() -> String {
    let mut dom = VirtualDom::new(root);
    dom.rebuild_in_place();
    dioxus_ssr::render(&dom)
}

#[test]
fn image_carries_max_width_and_max_height_none() {
    let html = render();
    assert!(
        html.contains("max-width: none") && html.contains("max-height: none"),
        "expected the image's inline style to override a host's max-width/\
         max-height reset, got: {html}"
    );
}

#[test]
fn viewport_carries_overflow_hidden() {
    let html = render();
    assert!(
        html.contains("overflow: hidden"),
        "expected the viewport div to clip via overflow: hidden, got: {html}"
    );
}

#[test]
fn stencil_overlay_carries_pointer_events_none() {
    let html = render();
    assert!(
        html.contains("pointer-events: none"),
        "expected the dim overlay to be pointer-transparent, got: {html}"
    );
}

#[test]
fn image_is_not_natively_draggable_and_has_user_select_none() {
    let html = render();
    assert!(
        html.contains("draggable=false"),
        "expected draggable=\"false\" on the image, got: {html}"
    );
    assert!(
        html.contains("user-select: none"),
        "expected user-select: none on the image, got: {html}"
    );
}
