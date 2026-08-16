#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Coverage for `crop::crop_to_png`'s screen-to-source pixel mapping: the
//! combined scale is `fit_scale * zoom`, not bare `zoom`. Every expected
//! pixel location below is derived independently from the transform
//! documented in `crop.rs`'s module doc
//! (`screen = Rotate(rotation) * (scale * c) + offset`, scale =
//! `fit_scale * zoom`), not by re-running the implementation.

use dioxus_cropper::geometry::{Point, Size, Stencil, ViewTransform};
use dioxus_cropper::{crop_to_png, CropError};
use image::{ImageBuffer, ImageFormat, Rgba, RgbaImage};

/// Encodes a `width`x`height` RGBA image built by `pixel` into PNG bytes,
/// the shape `crop_to_png` expects as `source_bytes`.
fn encode_png(width: u32, height: u32, pixel: impl Fn(u32, u32) -> Rgba<u8>) -> Vec<u8> {
    let img: RgbaImage = ImageBuffer::from_fn(width, height, pixel);
    let mut bytes = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut bytes), ImageFormat::Png)
        .expect("encode PNG");
    bytes
}

fn decode(bytes: &[u8]) -> RgbaImage {
    image::load_from_memory(bytes)
        .expect("decode cropped PNG")
        .to_rgba8()
}

// ── Identity when fit_scale * zoom == 1 ───────────────────────────────

#[test]
fn identity_scale_reproduces_the_source_pixel_for_pixel() {
    // natural == viewport -> fit_scale = 1.0; zoom = 1.0 -> scale = 1.0.
    // stencil == viewport -> out dims == source dims, and (per the module
    // doc's derivation) the mapping collapses to sx = ox, sy = oy exactly.
    let source = encode_png(8, 8, |x, y| Rgba([(x * 30) as u8, (y * 30) as u8, 0, 255]));
    let view = ViewTransform {
        offset: Point::ZERO,
        zoom: 1.0,
        rotation: 0.0,
    };
    let stencil = Stencil::rectangle(8.0, 8.0);
    let viewport = Size::new(8.0, 8.0);

    let cropped = crop_to_png(&source, view, stencil, viewport).expect("crop succeeds");
    assert_eq!(cropped.width, 8);
    assert_eq!(cropped.height, 8);

    let expected = decode(&source);
    let actual = decode(&cropped.png_bytes);
    for y in 0..8 {
        for x in 0..8 {
            assert_eq!(
                actual.get_pixel(x, y),
                expected.get_pixel(x, y),
                "mismatch at ({x},{y})"
            );
        }
    }
}

// ── Output dimensions equal stencil / (fit_scale * zoom) ──────────────

#[test]
fn output_dimensions_equal_stencil_over_effective_scale() {
    let source = encode_png(1000, 1000, |_, _| Rgba([200, 200, 200, 255]));
    let view = ViewTransform {
        offset: Point::ZERO,
        zoom: 2.0,
        rotation: 0.0,
    };
    // fit_scale = 500/1000 = 0.5; scale = 0.5 * 2.0 = 1.0
    let stencil = Stencil::rectangle(300.0, 150.0);
    let viewport = Size::new(500.0, 500.0);

    let cropped = crop_to_png(&source, view, stencil, viewport).expect("crop succeeds");
    assert_eq!(cropped.width, 300);
    assert_eq!(cropped.height, 150);
}

// ── A known offset lands the expected source pixel at the output centre ─

#[test]
fn known_offset_lands_the_expected_source_pixel_at_output_centre() {
    let mut natural = RgbaImage::from_pixel(100, 100, Rgba([10, 10, 10, 255]));
    // Marker at the source pixel the derivation below predicts will land at
    // the output's exact centre.
    natural.put_pixel(40, 55, Rgba([255, 0, 0, 255]));
    let mut bytes = Vec::new();
    natural
        .write_to(&mut std::io::Cursor::new(&mut bytes), ImageFormat::Png)
        .expect("encode PNG");

    let view = ViewTransform {
        offset: Point::new(10.0, -5.0),
        zoom: 1.0,
        rotation: 0.0,
    };
    let stencil = Stencil::rectangle(40.0, 40.0);
    let viewport = Size::new(100.0, 100.0);

    let cropped = crop_to_png(&bytes, view, stencil, viewport).expect("crop succeeds");
    assert_eq!(cropped.width, 40);
    assert_eq!(cropped.height, 40);

    let out = decode(&cropped.png_bytes);
    // Output centre pixel (20,20) must be the red marker.
    assert_eq!(out.get_pixel(20, 20), &Rgba([255, 0, 0, 255]));
    // A neighbouring pixel must not be red, ruling out a trivially-passing
    // "everything is red" mistake in the test's own source image.
    assert_ne!(out.get_pixel(19, 20), &Rgba([255, 0, 0, 255]));
}

// ── Regression guard: a non-1.0 fit_scale must scale the output ───────

#[test]
fn non_unit_fit_scale_scales_the_output_correctly() {
    // The exact worked example from crop.rs's module doc: a 1920-wide
    // source, a 480px viewport, zoom = 1.0 -> fit_scale = 0.25. A 240px
    // stencil must yield 960 source pixels per side, not 240 -- 240 is
    // what a `scale = zoom` regression (ignoring fit_scale) would produce.
    let source = encode_png(1920, 1920, |x, y| {
        Rgba([(x % 256) as u8, (y % 256) as u8, 0, 255])
    });
    let view = ViewTransform {
        offset: Point::ZERO,
        zoom: 1.0,
        rotation: 0.0,
    };
    let stencil = Stencil::square(240.0);
    let viewport = Size::new(480.0, 480.0);

    let cropped = crop_to_png(&source, view, stencil, viewport).expect("crop succeeds");
    assert_eq!(cropped.width, 960);
    assert_eq!(cropped.height, 960);

    // Pixel-level check too: at scale 0.25, the output centre must sample
    // the source's own centre pixel (960, 960).
    let expected = decode(&source);
    let out = decode(&cropped.png_bytes);
    assert_eq!(out.get_pixel(480, 480), expected.get_pixel(960, 960));
}

// ── Non-square viewport: transposition would fail loudly ──────────────

#[test]
fn non_square_viewport_scales_width_and_height_independently() {
    let source = encode_png(2000, 1000, |_, _| Rgba([50, 50, 50, 255]));
    let view = ViewTransform {
        offset: Point::ZERO,
        zoom: 1.0,
        rotation: 0.0,
    };
    // fit_scale = min(640/2000, 360/1000) = min(0.32, 0.36) = 0.32
    // (width-constrained). A width/height transposition anywhere in the
    // pipeline would pick min(360/2000, 640/1000) = 0.18 instead.
    let stencil = Stencil::rectangle(320.0, 144.0);
    let viewport = Size::new(640.0, 360.0);

    let cropped = crop_to_png(&source, view, stencil, viewport).expect("crop succeeds");
    assert_eq!(cropped.width, 1000);
    assert_eq!(cropped.height, 450);
}

// ── Rotation ────────────────────────────────────────────────────────

#[test]
fn rotation_maps_a_source_point_to_the_documented_screen_position() {
    let mut natural = RgbaImage::from_pixel(100, 100, Rgba([10, 10, 10, 255]));
    // Source pixel (50, 40): 10px north of centre (50,50).
    natural.put_pixel(50, 40, Rgba([0, 255, 0, 255]));
    let mut bytes = Vec::new();
    natural
        .write_to(&mut std::io::Cursor::new(&mut bytes), ImageFormat::Png)
        .expect("encode PNG");

    let view = ViewTransform {
        offset: Point::ZERO,
        zoom: 1.0,
        rotation: 90.0,
    };
    let stencil = Stencil::rectangle(40.0, 40.0);
    let viewport = Size::new(100.0, 100.0);

    let cropped = crop_to_png(&bytes, view, stencil, viewport).expect("crop succeeds");
    let out = decode(&cropped.png_bytes);

    // Forward derivation (screen = Rotate(90) * c, c = (0,-10)):
    // Rotate(90) * (0,-10) = (cos90*0 - sin90*(-10), sin90*0 + cos90*(-10))
    //                      = (10, 0) -- 10px east of centre on screen.
    // Output pixel = screen / scale + out_dim/2 = (30, 20).
    assert_eq!(out.get_pixel(30, 20), &Rgba([0, 255, 0, 255]));
}

// ── Out-of-source samples are transparent, not an error ───────────────

#[test]
fn samples_outside_the_source_are_transparent() {
    let source = encode_png(50, 50, |_, _| Rgba([255, 255, 255, 255]));
    let stencil = Stencil::rectangle(20.0, 20.0);
    let viewport = Size::new(50.0, 50.0);

    // A large positive offset pushes every sampled `sx`/`sy` negative --
    // exercises the below-source-bounds branch.
    let view_negative = ViewTransform {
        offset: Point::new(10_000.0, 10_000.0),
        zoom: 1.0,
        rotation: 0.0,
    };
    let cropped = crop_to_png(&source, view_negative, stencil, viewport).expect("crop succeeds");
    let out = decode(&cropped.png_bytes);
    for (_, _, pixel) in out.enumerate_pixels() {
        assert_eq!(pixel.0[3], 0, "expected fully transparent, got {pixel:?}");
    }

    // A large negative offset pushes every sampled `sx`/`sy` positive but
    // past the source's width/height -- exercises the above-source-bounds
    // branch, which a below-only bounds check would miss.
    let view_positive = ViewTransform {
        offset: Point::new(-10_000.0, -10_000.0),
        zoom: 1.0,
        rotation: 0.0,
    };
    let cropped = crop_to_png(&source, view_positive, stencil, viewport).expect("crop succeeds");
    let out = decode(&cropped.png_bytes);
    for (_, _, pixel) in out.enumerate_pixels() {
        assert_eq!(pixel.0[3], 0, "expected fully transparent, got {pixel:?}");
    }
}

// ── Degenerate inputs fail cleanly ─────────────────────────────────────

#[test]
fn non_finite_transform_is_rejected() {
    let source = encode_png(10, 10, |_, _| Rgba([1, 2, 3, 255]));
    let view = ViewTransform {
        offset: Point::new(f32::NAN, 0.0),
        zoom: 1.0,
        rotation: 0.0,
    };
    let result = crop_to_png(&source, view, Stencil::square(5.0), Size::new(10.0, 10.0));
    assert!(matches!(result, Err(CropError::NonFiniteTransform)));
}

#[test]
fn non_positive_zoom_is_rejected() {
    let source = encode_png(10, 10, |_, _| Rgba([1, 2, 3, 255]));
    let view = ViewTransform {
        offset: Point::ZERO,
        zoom: 0.0,
        rotation: 0.0,
    };
    let result = crop_to_png(&source, view, Stencil::square(5.0), Size::new(10.0, 10.0));
    assert!(matches!(result, Err(CropError::InvalidZoom)));
}

#[test]
fn empty_stencil_is_rejected() {
    let source = encode_png(10, 10, |_, _| Rgba([1, 2, 3, 255]));
    let view = ViewTransform::default();
    let result = crop_to_png(
        &source,
        view,
        Stencil::rectangle(0.0, 10.0),
        Size::new(10.0, 10.0),
    );
    assert!(matches!(result, Err(CropError::EmptyStencil)));
}

#[test]
fn empty_viewport_is_rejected() {
    let source = encode_png(10, 10, |_, _| Rgba([1, 2, 3, 255]));
    let view = ViewTransform::default();
    let result = crop_to_png(&source, view, Stencil::square(5.0), Size::new(0.0, 10.0));
    assert!(matches!(result, Err(CropError::EmptyViewport)));
}

// ── Circle stencil emits an unmasked square bounding box ──────────────

#[test]
fn circle_stencil_does_not_mask_the_output_corners() {
    let source = encode_png(40, 40, |_, _| Rgba([100, 150, 200, 255]));
    let view = ViewTransform::default();
    let stencil = Stencil::circle(40.0);
    let viewport = Size::new(40.0, 40.0);

    let cropped = crop_to_png(&source, view, stencil, viewport).expect("crop succeeds");
    let out = decode(&cropped.png_bytes);
    // A true circular mask would make the corners transparent; crop_to_png
    // deliberately does not apply the stencil's shape, so the corner must
    // still be fully opaque.
    assert_eq!(out.get_pixel(0, 0).0[3], 255);
    assert_eq!(out.get_pixel(39, 39).0[3], 255);
}
