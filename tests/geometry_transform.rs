#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Coverage for the small pure geometry primitives that are not the
//! screen-to-source mapping or the coverage clamp (those have their own
//! files): [`Point`], [`ViewTransform::default`], [`PanDirection::apply`],
//! [`normalize_rotation`], the [`Stencil`] constructors, and
//! [`contain_scale`] under a non-square viewport.

use dioxus_cropper::geometry::{normalize_rotation, Point, Size, Stencil, ViewTransform};
use dioxus_cropper::{contain_scale, PanDirection};

// ── Point ──────────────────────────────────────────────────────────────

#[test]
fn point_zero_is_origin() {
    assert_eq!(Point::ZERO, Point::new(0.0, 0.0));
}

// ── ViewTransform::default ────────────────────────────────────────────

#[test]
fn view_transform_default_is_identity() {
    let view = ViewTransform::default();
    assert_eq!(view.offset, Point::ZERO);
    assert_eq!(view.zoom, 1.0);
    assert_eq!(view.rotation, 0.0);
}

// ── PanDirection::apply ───────────────────────────────────────────────

#[test]
fn pan_direction_image_leaves_delta_unchanged() {
    let delta = Point::new(12.0, -7.0);
    assert_eq!(PanDirection::Image.apply(delta), delta);
}

#[test]
fn pan_direction_frame_negates_both_axes() {
    let delta = Point::new(12.0, -7.0);
    assert_eq!(PanDirection::Frame.apply(delta), Point::new(-12.0, 7.0));
}

// ── normalize_rotation ────────────────────────────────────────────────

#[test]
fn normalize_rotation_wraps_values_outside_0_360_into_range() {
    assert!((normalize_rotation(370.0) - 10.0).abs() < 1e-4);
    assert!((normalize_rotation(-10.0) - 350.0).abs() < 1e-4);
    assert!((normalize_rotation(720.0) - 0.0).abs() < 1e-4);
}

#[test]
fn normalize_rotation_has_no_drift_over_repeated_quarter_turns() {
    let start = 37.5_f32;
    let mut angle = start;
    for _ in 0..4 {
        angle = normalize_rotation(angle + 90.0);
    }
    // A full turn back to the start must be exact, not merely close —
    // rem_euclid on an already-in-range value is the identity, so four
    // 90-degree steps must land on precisely the starting angle with zero
    // accumulated float error.
    assert_eq!(angle, start);
}

// ── Stencil constructors ──────────────────────────────────────────────

#[test]
fn stencil_square_is_always_equal_sided() {
    let s = Stencil::square(150.0);
    assert_eq!(s.width(), s.height());
    assert_eq!(s.width(), 150.0);
}

#[test]
fn stencil_circle_is_always_equal_sided() {
    let c = Stencil::circle(80.0);
    assert_eq!(c.width(), c.height());
    assert_eq!(c.width(), 80.0);
}

#[test]
fn stencil_rectangle_permits_unequal_sides() {
    let r = Stencil::rectangle(300.0, 150.0);
    assert_eq!(r.width(), 300.0);
    assert_eq!(r.height(), 150.0);
}

// ── contain_scale under a non-square viewport ────────────────────────
//
// These two cases pick a viewport (640x360) and a natural size whose width-
// and height-constrained branches disagree, so a swapped axis produces a
// different, wrong number rather than an accidentally-equal one.

#[test]
fn contain_scale_is_width_constrained_under_a_wide_viewport() {
    let natural = Size::new(1000.0, 500.0);
    let viewport = Size::new(640.0, 360.0);
    // width ratio = 640/1000 = 0.64, height ratio = 360/500 = 0.72 -> min = 0.64
    let scale = contain_scale(natural, viewport);
    assert!((scale - 0.64).abs() < 1e-4, "got {scale}");
}

#[test]
fn contain_scale_is_height_constrained_under_a_wide_viewport() {
    let natural = Size::new(500.0, 1000.0);
    let viewport = Size::new(640.0, 360.0);
    // width ratio = 640/500 = 1.28, height ratio = 360/1000 = 0.36 -> min = 0.36
    let scale = contain_scale(natural, viewport);
    assert!((scale - 0.36).abs() < 1e-4, "got {scale}");
}
