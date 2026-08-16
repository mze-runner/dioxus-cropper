#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Coverage for `output_size`: it must agree with the pixel dimensions
//! `crop_decoded_to_png` actually returns for the same inputs, for a
//! rectangle stencil, a square stencil, and a non-1.0 zoom.

use dioxus_cropper::geometry::{Point, Size, Stencil, ViewTransform};
use dioxus_cropper::{output_size, CropError, DecodedSource};
use image::{ImageBuffer, ImageFormat, Rgba, RgbaImage};

fn encode_png(width: u32, height: u32) -> Vec<u8> {
    let img: RgbaImage = ImageBuffer::from_fn(width, height, |_, _| Rgba([1, 2, 3, 255]));
    let mut bytes = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut bytes), ImageFormat::Png)
        .expect("encode PNG");
    bytes
}

#[test]
fn output_size_agrees_with_crop_decoded_to_png_for_a_rectangle_stencil() {
    let source = encode_png(2000, 1000);
    let decoded = DecodedSource::decode(&source).expect("decode succeeds");
    let natural = decoded.natural_size();
    let viewport = Size::new(640.0, 360.0);
    let stencil = Stencil::rectangle(320.0, 144.0);
    let view = ViewTransform {
        offset: Point::ZERO,
        zoom: 1.0,
        rotation: 0.0,
    };

    let (predicted_w, predicted_h) =
        output_size(natural, viewport, stencil, view.zoom).expect("output_size succeeds");
    let cropped = dioxus_cropper::crop_decoded_to_png(&decoded, view, stencil, viewport)
        .expect("crop succeeds");

    assert_eq!(predicted_w, cropped.width);
    assert_eq!(predicted_h, cropped.height);
}

#[test]
fn output_size_agrees_with_crop_decoded_to_png_for_a_square_stencil() {
    let source = encode_png(1920, 1920);
    let decoded = DecodedSource::decode(&source).expect("decode succeeds");
    let natural = decoded.natural_size();
    let viewport = Size::new(480.0, 480.0);
    let stencil = Stencil::square(240.0);
    let view = ViewTransform {
        offset: Point::ZERO,
        zoom: 1.0,
        rotation: 0.0,
    };

    let (predicted_w, predicted_h) =
        output_size(natural, viewport, stencil, view.zoom).expect("output_size succeeds");
    let cropped = dioxus_cropper::crop_decoded_to_png(&decoded, view, stencil, viewport)
        .expect("crop succeeds");

    assert_eq!(predicted_w, cropped.width);
    assert_eq!(predicted_h, cropped.height);
}

#[test]
fn output_size_agrees_with_crop_decoded_to_png_for_a_non_unit_zoom() {
    let source = encode_png(1000, 1000);
    let decoded = DecodedSource::decode(&source).expect("decode succeeds");
    let natural = decoded.natural_size();
    let viewport = Size::new(500.0, 500.0);
    let stencil = Stencil::rectangle(300.0, 150.0);
    let view = ViewTransform {
        offset: Point::ZERO,
        zoom: 2.0,
        rotation: 0.0,
    };

    let (predicted_w, predicted_h) =
        output_size(natural, viewport, stencil, view.zoom).expect("output_size succeeds");
    let cropped = dioxus_cropper::crop_decoded_to_png(&decoded, view, stencil, viewport)
        .expect("crop succeeds");

    assert_eq!(predicted_w, cropped.width);
    assert_eq!(predicted_h, cropped.height);
}

// ── Rotation does not change the predicted dimensions ─────────────────
// output_size takes no rotation parameter: crop_decoded_to_png's out_w/out_h
// formula does not use `rotation`. A non-zero rotation leaves the two
// functions in agreement.

#[test]
fn output_size_agrees_with_crop_decoded_to_png_under_rotation() {
    let source = encode_png(1000, 1000);
    let decoded = DecodedSource::decode(&source).expect("decode succeeds");
    let natural = decoded.natural_size();
    let viewport = Size::new(500.0, 500.0);
    let stencil = Stencil::square(200.0);
    let view = ViewTransform {
        offset: Point::ZERO,
        zoom: 1.0,
        rotation: 37.0,
    };

    let (predicted_w, predicted_h) =
        output_size(natural, viewport, stencil, view.zoom).expect("output_size succeeds");
    let cropped = dioxus_cropper::crop_decoded_to_png(&decoded, view, stencil, viewport)
        .expect("crop succeeds");

    assert_eq!(predicted_w, cropped.width);
    assert_eq!(predicted_h, cropped.height);
}

// ── Degenerate `natural` is rejected ───────────────────────────────────

#[test]
fn output_size_rejects_zero_natural_width() {
    let natural = Size::new(0.0, 100.0);
    let result = output_size(
        natural,
        Size::new(480.0, 480.0),
        Stencil::square(240.0),
        1.0,
    );
    assert!(matches!(result, Err(CropError::EmptyNatural)));
}

#[test]
fn output_size_rejects_zero_natural_height() {
    let natural = Size::new(100.0, 0.0);
    let result = output_size(
        natural,
        Size::new(480.0, 480.0),
        Stencil::square(240.0),
        1.0,
    );
    assert!(matches!(result, Err(CropError::EmptyNatural)));
}

#[test]
fn output_size_rejects_negative_natural_width() {
    let natural = Size::new(-100.0, 100.0);
    let result = output_size(
        natural,
        Size::new(480.0, 480.0),
        Stencil::square(240.0),
        1.0,
    );
    assert!(matches!(result, Err(CropError::EmptyNatural)));
}

#[test]
fn output_size_rejects_negative_natural_height() {
    let natural = Size::new(100.0, -100.0);
    let result = output_size(
        natural,
        Size::new(480.0, 480.0),
        Stencil::square(240.0),
        1.0,
    );
    assert!(matches!(result, Err(CropError::EmptyNatural)));
}

#[test]
fn output_size_rejects_nan_natural_width() {
    let natural = Size::new(f32::NAN, 100.0);
    let result = output_size(
        natural,
        Size::new(480.0, 480.0),
        Stencil::square(240.0),
        1.0,
    );
    assert!(matches!(result, Err(CropError::EmptyNatural)));
}

#[test]
fn output_size_rejects_nan_natural_height() {
    let natural = Size::new(100.0, f32::NAN);
    let result = output_size(
        natural,
        Size::new(480.0, 480.0),
        Stencil::square(240.0),
        1.0,
    );
    assert!(matches!(result, Err(CropError::EmptyNatural)));
}

#[test]
fn output_size_rejects_infinite_natural_width() {
    let natural = Size::new(f32::INFINITY, 100.0);
    let result = output_size(
        natural,
        Size::new(480.0, 480.0),
        Stencil::square(240.0),
        1.0,
    );
    assert!(matches!(result, Err(CropError::EmptyNatural)));
}

#[test]
fn output_size_rejects_infinite_natural_height() {
    let natural = Size::new(100.0, f32::INFINITY);
    let result = output_size(
        natural,
        Size::new(480.0, 480.0),
        Stencil::square(240.0),
        1.0,
    );
    assert!(matches!(result, Err(CropError::EmptyNatural)));
}

// ── The `MAX_OUTPUT_PIXELS` limit ───────────────────────────────────────
//
// `output_size` computes `out_w = round(stencil.width / scale)`, `out_h`
// likewise. To land an output exactly at a known pixel area, pick a square
// stencil and a scale of 1.0 (natural == viewport, zoom == 1.0), so
// `out_w == out_h == stencil.width`.

#[test]
fn output_size_accepts_area_at_the_limit() {
    // 8192 * 8192 == 67_108_864 == MAX_OUTPUT_PIXELS exactly.
    let side = 8192.0;
    let natural = Size::new(side, side);
    let viewport = Size::new(side, side);
    let stencil = Stencil::square(side);
    let result = output_size(natural, viewport, stencil, 1.0).expect("output_size succeeds");
    assert_eq!(result, (8192, 8192));
}

#[test]
fn output_size_rejects_area_just_beyond_the_limit() {
    // 8193 * 8192 > MAX_OUTPUT_PIXELS.
    let natural = Size::new(8193.0, 8192.0);
    let viewport = Size::new(8193.0, 8192.0);
    let stencil = Stencil::rectangle(8193.0, 8192.0);
    let result = output_size(natural, viewport, stencil, 1.0);
    assert!(matches!(
        result,
        Err(CropError::OutputTooLarge {
            width: 8193,
            height: 8192
        })
    ));
}

#[test]
fn output_size_rejects_a_realistic_low_zoom_blowup() {
    // A 4032x3024 photo in a 480x480 viewport with a 240px stencil at
    // zoom = 0.05: fit_scale ~= 0.119, scale ~= 0.00595, output ~=
    // 40320x40320 -- far beyond MAX_OUTPUT_PIXELS.
    let natural = Size::new(4032.0, 3024.0);
    let viewport = Size::new(480.0, 480.0);
    let stencil = Stencil::square(240.0);
    let result = output_size(natural, viewport, stencil, 0.05);
    assert!(matches!(result, Err(CropError::OutputTooLarge { .. })));
}
