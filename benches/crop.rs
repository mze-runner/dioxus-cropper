#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Criterion benchmarks for the crop pipeline's three cost centres, measured
//! separately so a change to any one of them shows up in isolation:
//!
//! - `decode` — [`DecodedSource::decode`] on PNG-encoded sources, the cost
//!   `crop_to_png` pays on every call and `crop_decoded_to_png` amortises.
//! - `crop` — the full [`crop_decoded_to_png`] pass (per-pixel inverse
//!   sampling plus PNG encode) against pre-decoded sources, across source
//!   sizes and rotations. Rotation matters: `0.0` and `90.0` keep the
//!   inverse transform axis-aligned, while an oblique angle like `37.0`
//!   makes every sample a genuinely rotated lookup.
//! - `encode_png` — the same fast-compression/adaptive-filter `PngEncoder`
//!   call `crop_decoded_to_png` makes, on its own, so encode cost can be
//!   subtracted from the `crop` numbers to expose the sampling loop.
//!
//! All fixtures are synthetic and deterministic — no binary fixtures in the
//! repo, and identical pixel data on every run so numbers stay comparable
//! across machines and commits.

use std::hint::black_box;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use dioxus_cropper::geometry::{Point, Size, Stencil, ViewTransform};
use dioxus_cropper::{crop_decoded_to_png, DecodedSource};
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder, RgbaImage};

/// The source sizes the `decode` and `crop` groups run against: a typical
/// phone-camera photograph (~8 MP) and an 8K frame (~33 MP). The `crop`
/// group additionally prepends a small 1024x1024 source of its own.
const SOURCE_SIZES: [(u32, u32); 2] = [(3464, 2310), (7680, 4320)];

/// Builds a deterministic RGBA fixture: smooth per-axis gradients with
/// low-amplitude pseudo-noise XORed in. The mix matters for realism — a
/// flat fill deflates to almost nothing (unrealistically cheap PNG encode),
/// pure noise is incompressible (unrealistically expensive); gradient plus
/// mild noise sits in between, like a photograph. The noise comes from a
/// hand-rolled LCG (Knuth's MMIX multiplier) with a fixed seed, so the
/// pixels are bit-identical on every run without pulling in `rand`.
fn synthetic_rgba(width: u32, height: u32) -> RgbaImage {
    let mut data = Vec::with_capacity(width as usize * height as usize * 4);
    let mut state: u64 = 0x243F_6A88_85A3_08D3;
    for y in 0..height {
        for x in 0..width {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            // An LCG's high bits are its best-distributed ones — take three
            // distinct high bytes, one per channel, masked down so the noise
            // perturbs rather than swamps the gradients.
            let r = ((x as u64 * 255) / width as u64) as u8 ^ ((state >> 56) as u8 & 0x1F);
            let g = ((y as u64 * 255) / height as u64) as u8 ^ ((state >> 48) as u8 & 0x1F);
            let b = (((x + y) as u64 * 255) / (width + height) as u64) as u8
                ^ ((state >> 40) as u8 & 0x1F);
            data.extend_from_slice(&[r, g, b, 255]);
        }
    }
    RgbaImage::from_raw(width, height, data).expect("buffer sized to width * height * 4")
}

/// PNG-encodes `img` with the same encoder and settings as
/// `crop_decoded_to_png`; must stay in lockstep with `crop.rs`. Used only
/// by the `encode_png` group, which measures the library's own encode call.
fn encode_to_png(img: &RgbaImage) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(img.width() as usize * img.height() as usize);
    PngEncoder::new_with_quality(&mut bytes, CompressionType::Fast, FilterType::Adaptive)
        .write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            ExtendedColorType::Rgba8,
        )
        .expect("PNG encode of a valid RGBA buffer");
    bytes
}

/// PNG-encodes a decode/crop input fixture with pinned settings, independent
/// of the library's own encode configuration. Fixture bytes must not change
/// when the library's encoder settings do, or `decode` numbers stop being
/// comparable across versions; `Default` compression also matches the
/// densely-compressed files real sources are.
fn encode_fixture_png(img: &RgbaImage) -> Vec<u8> {
    let mut bytes = Vec::new();
    PngEncoder::new_with_quality(&mut bytes, CompressionType::Default, FilterType::Adaptive)
        .write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            ExtendedColorType::Rgba8,
        )
        .expect("PNG encode of a valid RGBA buffer");
    bytes
}

/// Applies the shared budget: these iterations run tens of milliseconds to
/// seconds each, so the statistical minimum of 10 samples over a ~5 s
/// window keeps a full `cargo bench` in minutes rather than hours.
fn configure(group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>) {
    group
        .sample_size(10)
        .warm_up_time(Duration::from_secs(2))
        .measurement_time(Duration::from_secs(5));
}

fn bench_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("decode");
    configure(&mut group);
    for (w, h) in SOURCE_SIZES {
        // Encode once here, outside the measured loop — the bench measures
        // decode only, against bytes that already exist.
        let png = encode_fixture_png(&synthetic_rgba(w, h));
        group.bench_function(format!("{w}x{h}"), |b| {
            b.iter(|| DecodedSource::decode(black_box(&png)).expect("decode synthetic PNG"));
        });
    }
    group.finish();
}

fn bench_crop(c: &mut Criterion) {
    let mut group = c.benchmark_group("crop");
    configure(&mut group);

    // The same fixed framing throughout: a 480x480 viewport with a centred
    // 240px square stencil at zoom 1.0. With `fit_scale = 480 / width`, the
    // output comes out at `240 / (fit_scale * zoom)` pixels per side —
    // 512, 1732 and 3840 respectively for the three sources — so the crop
    // cost scales with the source even though the framing never changes.
    let viewport = Size::new(480.0, 480.0);
    let stencil = Stencil::square(240.0);

    let mut sizes = vec![(1024, 1024)];
    sizes.extend(SOURCE_SIZES);
    for (w, h) in sizes {
        // Decode once per source, outside the measured loop — this group
        // measures sampling + encode, not decode (that's `decode`'s job).
        let decoded = DecodedSource::decode(&encode_fixture_png(&synthetic_rgba(w, h)))
            .expect("decode fixture");
        for rotation in [0.0_f32, 90.0, 37.0] {
            let view = ViewTransform {
                offset: Point::ZERO,
                zoom: 1.0,
                rotation,
            };
            group.bench_function(
                BenchmarkId::new(format!("{w}x{h}"), format!("rot{rotation}")),
                |b| {
                    b.iter(|| {
                        crop_decoded_to_png(black_box(&decoded), view, stencil, viewport)
                            .expect("valid crop inputs")
                    });
                },
            );
        }
    }
    group.finish();
}

fn bench_encode_png(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode_png");
    configure(&mut group);
    // The `crop` group's output sizes, so `crop` minus `encode_png` at the
    // matching size approximates the sampling loop alone.
    for side in [512_u32, 1732, 3840] {
        let img = synthetic_rgba(side, side);
        group.bench_function(format!("{side}x{side}"), |b| {
            b.iter(|| encode_to_png(black_box(&img)));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_decode, bench_crop, bench_encode_png);
criterion_main!(benches);
