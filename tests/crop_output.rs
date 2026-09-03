#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Coverage for `crop_decoded` and `output_size_with`: the default target
//! reproduces `crop_decoded_to_png` byte-for-byte, `SizeTarget::Native`
//! agrees with `output_size` on both `Ok` and `Err`, and each resampling
//! path (exact area average, supersampled box average, bilinear upscale) is
//! checked against an independently computed reference.

use dioxus_cropper::geometry::{contain_scale, Point, Size, Stencil, ViewTransform};
use dioxus_cropper::{
    crop_decoded, crop_decoded_to_png, output_size, output_size_with, CropError, CropOutput,
    CroppedFormat, DecodedSource, JpegOptions, OutputFormat, SizeTarget,
};
use image::{ImageBuffer, ImageFormat, Rgba, RgbaImage};

/// Encodes a `width`x`height` RGBA image built by `pixel` into PNG bytes
/// and decodes them into the `DecodedSource` the crop functions take.
fn decoded(width: u32, height: u32, pixel: impl Fn(u32, u32) -> Rgba<u8>) -> DecodedSource {
    let img: RgbaImage = ImageBuffer::from_fn(width, height, pixel);
    let mut bytes = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut bytes), ImageFormat::Png)
        .expect("encode PNG fixture");
    DecodedSource::decode(&bytes).expect("decode PNG fixture")
}

/// A gradient with varying alpha: every pixel differs from its neighbours,
/// and alpha varies so premultiplied averaging is actually exercised.
fn alpha_gradient(x: u32, y: u32) -> Rgba<u8> {
    Rgba([
        (x * 7 % 251) as u8,
        (y * 13 % 241) as u8,
        ((x + y) * 3 % 253) as u8,
        ((x * 31 + y * 17) % 256) as u8,
    ])
}

// ── 1. Default `CropOutput` reproduces `crop_decoded_to_png` ──────────

#[test]
fn default_output_matches_crop_decoded_to_png_byte_for_byte() {
    let source = decoded(64, 48, alpha_gradient);
    let cases = [
        (
            ViewTransform {
                offset: Point::ZERO,
                zoom: 1.0,
                rotation: 0.0,
            },
            Stencil::rectangle(16.0, 12.0),
            Size::new(32.0, 24.0),
        ),
        (
            ViewTransform {
                offset: Point::new(3.5, -2.25),
                zoom: 1.3,
                rotation: 37.0,
            },
            Stencil::square(20.0),
            Size::new(32.0, 32.0),
        ),
        (
            ViewTransform {
                offset: Point::new(-1.0, 4.0),
                zoom: 0.8,
                rotation: 90.0,
            },
            Stencil::circle(18.0),
            Size::new(30.0, 30.0),
        ),
    ];
    for (view, stencil, viewport) in cases {
        let png = crop_decoded_to_png(&source, view, stencil, viewport).expect("png crop");
        let out = crop_decoded(&source, view, stencil, viewport, CropOutput::default())
            .expect("default crop_decoded");
        assert_eq!(out.width, png.width);
        assert_eq!(out.height, png.height);
        assert_eq!(out.format, CroppedFormat::Png);
        assert_eq!(out.bytes, png.png_bytes, "rotation {}", view.rotation);
    }
}

// ── 2. A non-binding `MaxDimension` cap is identical to `Native` ──────

#[test]
fn non_binding_max_dimension_is_byte_identical_to_native() {
    let source = decoded(64, 48, alpha_gradient);
    let viewport = Size::new(32.0, 24.0);
    let stencil = Stencil::rectangle(20.0, 14.0);
    for rotation in [0.0f32, 23.0] {
        let view = ViewTransform {
            offset: Point::new(1.5, -0.5),
            zoom: 1.0,
            rotation,
        };
        let native = crop_decoded(&source, view, stencil, viewport, CropOutput::default())
            .expect("native crop");
        let capped = crop_decoded(
            &source,
            view,
            stencil,
            viewport,
            CropOutput {
                size: SizeTarget::MaxDimension(10_000),
                format: OutputFormat::Png,
            },
        )
        .expect("capped crop");
        assert_eq!(capped, native, "rotation {rotation}");
    }
}

// ── 3. `output_size_with(Native)` ≡ `output_size`, Ok and Err ─────────

#[test]
fn output_size_with_native_agrees_with_output_size() {
    let cases: [(Size, Size, Stencil, f32); 8] = [
        // Ok cases.
        (
            Size::new(2000.0, 1000.0),
            Size::new(640.0, 360.0),
            Stencil::rectangle(320.0, 144.0),
            1.0,
        ),
        (
            Size::new(1920.0, 1920.0),
            Size::new(480.0, 480.0),
            Stencil::square(240.0),
            1.7,
        ),
        (
            Size::new(64.0, 48.0),
            Size::new(32.0, 24.0),
            Stencil::circle(13.0),
            0.6,
        ),
        // Err cases: non-finite zoom, non-positive zoom, empty stencil,
        // empty viewport, output over the pixel limit.
        (
            Size::new(100.0, 100.0),
            Size::new(100.0, 100.0),
            Stencil::square(50.0),
            f32::NAN,
        ),
        (
            Size::new(100.0, 100.0),
            Size::new(100.0, 100.0),
            Stencil::square(50.0),
            0.0,
        ),
        (
            Size::new(100.0, 100.0),
            Size::new(100.0, 100.0),
            Stencil::rectangle(0.0, 10.0),
            1.0,
        ),
        (
            Size::new(100.0, 100.0),
            Size::new(0.0, 100.0),
            Stencil::square(50.0),
            1.0,
        ),
        (
            Size::new(100.0, 100.0),
            Size::new(100.0, 100.0),
            Stencil::square(100.0),
            0.001,
        ),
    ];
    for (natural, viewport, stencil, zoom) in cases {
        let plain = output_size(natural, viewport, stencil, zoom);
        let with = output_size_with(natural, viewport, stencil, zoom, SizeTarget::Native);
        assert_eq!(
            format!("{plain:?}"),
            format!("{with:?}"),
            "diverged for natural {}x{}, zoom {zoom}",
            natural.width,
            natural.height,
        );
    }
}

// ── 4. Rotation-0 area average vs a brute-force reference ─────────────

/// Exact area average of the materialised `nat_w`x`nat_h` native crop down
/// to `out_w`x`out_h`, with the same loop structure, weights and
/// accumulation order as the implementation. Out-of-source cells are the
/// zeroed (transparent) pixels the native buffer already holds.
fn area_reference(native: &[u8], nat_w: u32, nat_h: u32, out_w: u32, out_h: u32) -> Vec<u8> {
    let rx = f64::from(nat_w) / f64::from(out_w);
    let ry = f64::from(nat_h) / f64::from(out_h);
    let mut pixels = vec![0u8; out_w as usize * out_h as usize * 4];
    for oy in 0..out_h {
        let y0 = f64::from(oy) * ry;
        let y1 = f64::from(oy + 1) * ry;
        for ox in 0..out_w {
            let x0 = f64::from(ox) * rx;
            let x1 = f64::from(ox + 1) * rx;
            let mut acc = [0.0f64; 4];
            for ny in (y0.floor() as i64)..(y1.ceil() as i64) {
                let wy = y1.min((ny + 1) as f64) - y0.max(ny as f64);
                if wy <= 0.0 {
                    continue;
                }
                for nx in (x0.floor() as i64)..(x1.ceil() as i64) {
                    let wx = x1.min((nx + 1) as f64) - x0.max(nx as f64);
                    if wx <= 0.0 {
                        continue;
                    }
                    if nx >= 0 && ny >= 0 && (nx as u32) < nat_w && (ny as u32) < nat_h {
                        let at = (ny as usize * nat_w as usize + nx as usize) * 4;
                        let px = &native[at..at + 4];
                        let w = wx * wy;
                        let a = f64::from(px[3]);
                        acc[0] += w * f64::from(px[0]) * a;
                        acc[1] += w * f64::from(px[1]) * a;
                        acc[2] += w * f64::from(px[2]) * a;
                        acc[3] += w * a;
                    }
                }
            }
            let at = (oy as usize * out_w as usize + ox as usize) * 4;
            pixels[at + 3] = (acc[3] / ((x1 - x0) * (y1 - y0))).round() as u8;
            if acc[3] > 0.0 {
                pixels[at] = (acc[0] / acc[3]).round() as u8;
                pixels[at + 1] = (acc[1] / acc[3]).round() as u8;
                pixels[at + 2] = (acc[2] / acc[3]).round() as u8;
            }
        }
    }
    pixels
}

#[test]
fn rotation_zero_downscale_is_an_exact_area_average() {
    let source = decoded(32, 24, alpha_gradient);
    // fit_scale = 1, zoom = 1; the stencil is larger than the source, so
    // the native crop carries transparent out-of-source bands, and the
    // integer offset keeps the view on the translation path.
    let viewport = Size::new(32.0, 24.0);
    let stencil = Stencil::rectangle(40.0, 30.0);
    for offset in [Point::ZERO, Point::new(5.0, -3.0)] {
        let view = ViewTransform {
            offset,
            zoom: 1.0,
            rotation: 0.0,
        };
        let native = crop_decoded(
            &source,
            view,
            stencil,
            viewport,
            CropOutput {
                size: SizeTarget::Native,
                format: OutputFormat::Rgba,
            },
        )
        .expect("native rgba crop");
        assert_eq!((native.width, native.height), (40, 30));

        let (out_w, out_h) = (16u32, 12u32);
        let produced = crop_decoded(
            &source,
            view,
            stencil,
            viewport,
            CropOutput {
                size: SizeTarget::Exact {
                    width: out_w,
                    height: out_h,
                },
                format: OutputFormat::Rgba,
            },
        )
        .expect("downscaled rgba crop");
        assert_eq!((produced.width, produced.height), (out_w, out_h));
        assert_eq!(produced.format, CroppedFormat::Rgba);

        let reference = area_reference(&native.bytes, 40, 30, out_w, out_h);
        assert_eq!(
            produced.bytes, reference,
            "offset ({}, {})",
            offset.x, offset.y
        );
    }
}

// ── 5. Rotated downscale vs a higher-k supersampled reference ─────────

/// Supersampled box average with a `k`x`k` subsample grid per output
/// pixel, nearest-neighbour reads, premultiplied averaging — the same
/// construction as the implementation, at a much higher `k`.
#[allow(clippy::too_many_arguments)]
fn supersample_reference(
    source: &RgbaImage,
    nat: (u32, u32),
    out: (u32, u32),
    scale: f64,
    offset: Point,
    rotation: f32,
    k: u32,
) -> Vec<u8> {
    let (nat_w, nat_h) = nat;
    let (out_w, out_h) = out;
    let rx = f64::from(nat_w) / f64::from(out_w);
    let ry = f64::from(nat_h) / f64::from(out_h);
    let (sin_inv, cos_inv) = (-f64::from(rotation).to_radians()).sin_cos();
    let mut pixels = vec![0u8; out_w as usize * out_h as usize * 4];
    for oy in 0..out_h {
        for ox in 0..out_w {
            let mut acc = [0.0f64; 4];
            for j in 0..k {
                let y = (f64::from(oy) + (f64::from(j) + 0.5) / f64::from(k)) * ry - 0.5;
                let dy = (y - f64::from(nat_h) / 2.0) * scale - f64::from(offset.y);
                for i in 0..k {
                    let x = (f64::from(ox) + (f64::from(i) + 0.5) / f64::from(k)) * rx - 0.5;
                    let dx = (x - f64::from(nat_w) / 2.0) * scale - f64::from(offset.x);
                    let sx =
                        (dx * cos_inv - dy * sin_inv) / scale + f64::from(source.width()) / 2.0;
                    let sy =
                        (dx * sin_inv + dy * cos_inv) / scale + f64::from(source.height()) / 2.0;
                    let (ix, iy) = (sx.round(), sy.round());
                    if ix >= 0.0 && iy >= 0.0 {
                        let (ix, iy) = (ix as u32, iy as u32);
                        if ix < source.width() && iy < source.height() {
                            let px = source.get_pixel(ix, iy).0;
                            let a = f64::from(px[3]);
                            acc[0] += f64::from(px[0]) * a;
                            acc[1] += f64::from(px[1]) * a;
                            acc[2] += f64::from(px[2]) * a;
                            acc[3] += a;
                        }
                    }
                }
            }
            let at = (oy as usize * out_w as usize + ox as usize) * 4;
            pixels[at + 3] = (acc[3] / f64::from(k * k)).round() as u8;
            if acc[3] > 0.0 {
                pixels[at] = (acc[0] / acc[3]).round() as u8;
                pixels[at + 1] = (acc[1] / acc[3]).round() as u8;
                pixels[at + 2] = (acc[2] / acc[3]).round() as u8;
            }
        }
    }
    pixels
}

#[test]
fn rotated_downscale_tracks_a_higher_k_supersampled_reference() {
    // A smooth (locally linear) opaque gradient, cropped well inside the
    // source so every subsample lands in-bounds.
    let smooth = |x: u32, y: u32| Rgba([(x * 2) as u8, (y * 2) as u8, (x + y) as u8, 255]);
    let source = decoded(64, 64, smooth);
    let raw: RgbaImage = ImageBuffer::from_fn(64, 64, smooth);
    let viewport = Size::new(64.0, 64.0);
    let stencil = Stencil::square(24.0);
    let view = ViewTransform {
        offset: Point::ZERO,
        zoom: 1.0,
        rotation: 37.0,
    };
    let scale = f64::from(contain_scale(source.natural_size(), viewport));
    assert_eq!(scale, 1.0);

    let produced = crop_decoded(
        &source,
        view,
        stencil,
        viewport,
        CropOutput {
            size: SizeTarget::Exact {
                width: 8,
                height: 8,
            },
            format: OutputFormat::Rgba,
        },
    )
    .expect("rotated downscale");
    let reference = supersample_reference(&raw, (24, 24), (8, 8), scale, Point::ZERO, 37.0, 24);
    assert_eq!(produced.bytes.len(), reference.len());
    for (i, (p, r)) in produced.bytes.iter().zip(&reference).enumerate() {
        assert!(
            (i16::from(*p) - i16::from(*r)).abs() <= 2,
            "byte {i}: produced {p}, reference {r}"
        );
    }
}

// ── 6. Bilinear upscale of a 2x2 source ───────────────────────────────

#[test]
fn exact_upscale_is_bilinear_at_interior_pixels() {
    let source = decoded(2, 2, |x, y| match (x, y) {
        (0, 0) => Rgba([10, 20, 30, 255]),
        (1, 0) => Rgba([110, 20, 30, 255]),
        (0, 1) => Rgba([10, 120, 30, 255]),
        _ => Rgba([110, 120, 130, 255]),
    });
    let view = ViewTransform {
        offset: Point::ZERO,
        zoom: 1.0,
        rotation: 0.0,
    };
    let out = crop_decoded(
        &source,
        view,
        Stencil::rectangle(2.0, 2.0),
        Size::new(2.0, 2.0),
        CropOutput {
            size: SizeTarget::Exact {
                width: 4,
                height: 4,
            },
            format: OutputFormat::Rgba,
        },
    )
    .expect("upscaled crop");
    assert_eq!((out.width, out.height), (4, 4));

    // Output pixel (ox, oy) maps to source coordinate
    // ((ox + 0.5) * 0.5 - 0.5, (oy + 0.5) * 0.5 - 0.5); the interior
    // pixels land at fractional coordinates 0.25 and 0.75 on each axis and
    // blend all four source pixels bilinearly (hand-computed below).
    let expected: [((u32, u32), [u8; 4]); 4] = [
        ((1, 1), [35, 45, 36, 255]),
        ((2, 1), [85, 45, 49, 255]),
        ((1, 2), [35, 95, 49, 255]),
        ((2, 2), [85, 95, 86, 255]),
    ];
    for ((ox, oy), want) in expected {
        let at = (oy as usize * 4 + ox as usize) * 4;
        assert_eq!(&out.bytes[at..at + 4], &want, "pixel ({ox}, {oy})");
    }
}

// ── 7. JPEG: background compositing and quality validation ────────────

#[test]
fn jpeg_composites_fully_transparent_pixels_onto_the_background() {
    let source = decoded(8, 8, |_, _| Rgba([200, 50, 25, 0]));
    let view = ViewTransform::default();
    let stencil = Stencil::rectangle(8.0, 8.0);
    let viewport = Size::new(8.0, 8.0);
    let background = [30u8, 60, 90];
    let out = crop_decoded(
        &source,
        view,
        stencil,
        viewport,
        CropOutput {
            size: SizeTarget::Native,
            format: OutputFormat::Jpeg(JpegOptions {
                quality: 85,
                background,
            }),
        },
    )
    .expect("jpeg crop");
    assert_eq!(out.format, CroppedFormat::Jpeg);

    let round_tripped = image::load_from_memory(&out.bytes)
        .expect("decode produced JPEG")
        .to_rgba8();
    assert_eq!((round_tripped.width(), round_tripped.height()), (8, 8));
    for (x, y, px) in round_tripped.enumerate_pixels() {
        for (c, (&actual, &expected)) in px.0.iter().zip(background.iter()).enumerate() {
            assert!(
                (i16::from(actual) - i16::from(expected)).abs() <= 3,
                "channel {c} at ({x}, {y}): {actual} vs background {expected}"
            );
        }
    }
}

#[test]
fn jpeg_quality_out_of_range_is_an_invalid_output_target() {
    let source = decoded(8, 8, |_, _| Rgba([1, 2, 3, 255]));
    let view = ViewTransform::default();
    let stencil = Stencil::rectangle(8.0, 8.0);
    let viewport = Size::new(8.0, 8.0);
    for quality in [0u8, 101] {
        let result = crop_decoded(
            &source,
            view,
            stencil,
            viewport,
            CropOutput {
                size: SizeTarget::Native,
                format: OutputFormat::Jpeg(JpegOptions {
                    quality,
                    background: [0, 0, 0],
                }),
            },
        );
        assert!(
            matches!(result, Err(CropError::InvalidOutputTarget)),
            "quality {quality}: {result:?}"
        );
    }
}

// ── 8. 1x1 targets do not panic and produce sane output ───────────────

#[test]
fn one_by_one_targets_produce_sane_output() {
    let source = decoded(64, 48, |x, y| Rgba([(x * 3) as u8, (y * 5) as u8, 7, 255]));
    let view = ViewTransform::default();
    let stencil = Stencil::rectangle(32.0, 24.0);
    let viewport = Size::new(32.0, 24.0);
    // fit_scale = 0.5, zoom = 1: native dims are 64x48.
    assert_eq!(
        output_size_with(
            source.natural_size(),
            viewport,
            stencil,
            1.0,
            SizeTarget::MaxDimension(1)
        )
        .expect("capped size"),
        (1, 1)
    );
    for size in [
        SizeTarget::Exact {
            width: 1,
            height: 1,
        },
        SizeTarget::MaxDimension(1),
    ] {
        let out = crop_decoded(
            &source,
            view,
            stencil,
            viewport,
            CropOutput {
                size,
                format: OutputFormat::Rgba,
            },
        )
        .expect("1x1 crop");
        assert_eq!((out.width, out.height), (1, 1));
        assert_eq!(out.bytes.len(), 4);
        assert_eq!(out.bytes[3], 255, "opaque source must average opaque");
    }
}

// ── 9. Invalid size targets ───────────────────────────────────────────

#[test]
fn zero_size_targets_are_invalid_output_targets() {
    let source = decoded(8, 8, |_, _| Rgba([1, 2, 3, 255]));
    let natural = source.natural_size();
    let view = ViewTransform::default();
    let stencil = Stencil::rectangle(8.0, 8.0);
    let viewport = Size::new(8.0, 8.0);
    let targets = [
        SizeTarget::Exact {
            width: 0,
            height: 5,
        },
        SizeTarget::Exact {
            width: 5,
            height: 0,
        },
        SizeTarget::MaxDimension(0),
    ];
    for target in targets {
        let sized = output_size_with(natural, viewport, stencil, 1.0, target);
        assert!(
            matches!(sized, Err(CropError::InvalidOutputTarget)),
            "output_size_with, {target:?}: {sized:?}"
        );
        let cropped = crop_decoded(
            &source,
            view,
            stencil,
            viewport,
            CropOutput {
                size: target,
                format: OutputFormat::Png,
            },
        );
        assert!(
            matches!(cropped, Err(CropError::InvalidOutputTarget)),
            "crop_decoded, {target:?}: {cropped:?}"
        );
    }
}

// ── 10. `MAX_OUTPUT_PIXELS` gates the FINAL dimensions ────────────────

#[test]
fn over_limit_native_size_succeeds_under_a_shrinking_target() {
    // 64x64 source in a 64px viewport: fit_scale = 1.0, so zoom 0.005
    // yields a native crop of 64 / 0.005 = 12800 per side — 163.8 M px,
    // over `MAX_OUTPUT_PIXELS` — which `output_size` rejects.
    let source = decoded(64, 64, alpha_gradient);
    let natural = source.natural_size();
    let viewport = Size::new(64.0, 64.0);
    let stencil = Stencil::square(64.0);
    let view = ViewTransform {
        offset: Point::ZERO,
        zoom: 0.005,
        rotation: 0.0,
    };

    assert!(matches!(
        output_size(natural, viewport, stencil, view.zoom),
        Err(CropError::OutputTooLarge { .. })
    ));

    let (w, h) = output_size_with(
        natural,
        viewport,
        stencil,
        view.zoom,
        SizeTarget::MaxDimension(64),
    )
    .expect("capped size succeeds");
    assert_eq!((w, h), (64, 64));

    let out = crop_decoded(
        &source,
        view,
        stencil,
        viewport,
        CropOutput {
            size: SizeTarget::MaxDimension(64),
            format: OutputFormat::Rgba,
        },
    )
    .expect("capped crop succeeds");
    assert_eq!((out.width, out.height), (64, 64));
    assert_eq!(out.bytes.len(), 64 * 64 * 4);
}

#[test]
fn over_limit_exact_target_is_output_too_large() {
    let source = decoded(8, 8, alpha_gradient);
    let natural = source.natural_size();
    let viewport = Size::new(8.0, 8.0);
    let stencil = Stencil::square(8.0);
    let view = ViewTransform::default();
    // 16384 x 8192 = 134.2 M px, over the 67.1 M px limit.
    let target = SizeTarget::Exact {
        width: 16384,
        height: 8192,
    };

    assert!(matches!(
        output_size_with(natural, viewport, stencil, view.zoom, target),
        Err(CropError::OutputTooLarge { .. })
    ));
    assert!(matches!(
        crop_decoded(
            &source,
            view,
            stencil,
            viewport,
            CropOutput {
                size: target,
                format: OutputFormat::Rgba,
            },
        ),
        Err(CropError::OutputTooLarge { .. })
    ));
}
