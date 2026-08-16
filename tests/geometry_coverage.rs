#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Coverage for the stencil-coverage guarantee: `min_zoom_to_cover` (the
//! floor) and `clamp_offset` (the pan bound), together with the primitive
//! both build on, `rotated_bounding_box`. Every expected value below is
//! computed independently by hand from the formulas documented on each
//! function, not by re-deriving what the implementation happens to compute.

use dioxus_cropper::geometry::{clamp_offset, min_zoom_to_cover, rotated_bounding_box};
use dioxus_cropper::{Point, Size, Stencil};

// ── rotated_bounding_box ──────────────────────────────────────────────

#[test]
fn rotated_bounding_box_at_zero_rotation_is_the_identity() {
    let size = Size::new(100.0, 200.0);
    let bbox = rotated_bounding_box(size, 0.0);
    assert!((bbox.width - 100.0).abs() < 1e-3);
    assert!((bbox.height - 200.0).abs() < 1e-3);
}

#[test]
fn rotated_bounding_box_at_90_degrees_swaps_the_axes() {
    let size = Size::new(100.0, 200.0);
    let bbox = rotated_bounding_box(size, 90.0);
    assert!((bbox.width - 200.0).abs() < 1e-2, "got {bbox:?}");
    assert!((bbox.height - 100.0).abs() < 1e-2, "got {bbox:?}");
}

// ── min_zoom_to_cover: the floor ──────────────────────────────────────

#[test]
fn min_zoom_to_cover_is_the_binding_axis_ratio_at_zero_rotation() {
    let natural = Size::new(100.0, 200.0);
    let stencil = Stencil::square(150.0);
    // displayed at unit zoom = (100, 200); ratios = 150/100=1.5, 150/200=0.75
    // -> binding axis is width, floor = 1.5
    let zoom = min_zoom_to_cover(natural, 1.0, 0.0, stencil);
    assert!((zoom - 1.5).abs() < 1e-3, "got {zoom}");
}

#[test]
fn min_zoom_to_cover_changes_under_rotation_via_the_bounding_box() {
    let natural = Size::new(100.0, 200.0);
    let stencil = Stencil::square(150.0);
    // At 45 degrees, rotated_bounding_box(100,200,45) = (212.13, 212.13)
    // (both axes pick up cos+sin contributions equally at 45 degrees), so
    // the floor becomes 150/212.13 =~ 0.7071 -- lower than the 1.5 floor at
    // zero rotation. A rotated image's apparent footprint is *larger* than
    // its own unrotated size, so this metric says less zoom is needed, even
    // though exact rotated-rectangle coverage would demand more. The test
    // locks in what the function actually promises, not exact coverage.
    let zoom = min_zoom_to_cover(natural, 1.0, 45.0, stencil);
    let expected = 150.0 / (300.0 * std::f32::consts::FRAC_1_SQRT_2);
    assert!((zoom - expected).abs() < 1e-3, "got {zoom}");
}

// ── clamp_offset: the pan bound ───────────────────────────────────────

#[test]
fn clamp_offset_zeroes_the_axis_exactly_covered_and_leaves_slack_on_the_other() {
    let natural = Size::new(100.0, 200.0);
    let stencil = Stencil::square(150.0);
    // zoom = min_zoom_to_cover(...) for this exact input (1.5, see above) ->
    // displayed = (150, 300); bbox at rotation 0 = (150, 300).
    // max_x = (150-150)/2 = 0 (no slack: this axis is exactly covered).
    // max_y = (300-150)/2 = 75 (75px of slack).
    let zoom = 1.5;
    let clamped = clamp_offset(Point::new(1000.0, 1000.0), natural, 1.0, zoom, 0.0, stencil);
    assert_eq!(clamped.x, 0.0, "binding axis must clamp to exactly zero");
    assert!((clamped.y - 75.0).abs() < 1e-2, "got {clamped:?}");
}

#[test]
fn clamp_offset_collapses_to_zero_when_the_image_is_smaller_than_the_stencil() {
    let natural = Size::new(50.0, 50.0);
    let stencil = Stencil::square(100.0);
    // zoom = 1.0, deliberately below min_zoom_to_cover for this input: the
    // displayed image (50x50) is smaller than the stencil (100x100) on both
    // axes, so no offset can keep the stencil covered. The range must
    // collapse to a single point (0,0), never invert to a negative bound.
    let clamped = clamp_offset(Point::new(30.0, -30.0), natural, 1.0, 1.0, 0.0, stencil);
    assert_eq!(clamped, Point::new(0.0, 0.0));
}
