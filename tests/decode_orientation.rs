#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Coverage for EXIF orientation: `DecodedSource::decode` and
//! `natural_size` apply it by default and leave the stored pixels alone
//! when `DecodeOptions::apply_orientation` is off; `crop_async` produces
//! the bytes `crop_decoded` does.

use std::future::Future;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use std::thread::Thread;

use dioxus_cropper::geometry::{Point, Size, Stencil, ViewTransform};
use dioxus_cropper::{
    crop_async, crop_decoded, natural_size, natural_size_with, CropOutput, DecodeOptions,
    DecodedSource, OutputFormat, SizeTarget,
};
use image::codecs::png::PngEncoder;
use image::{imageops, ExtendedColorType, ImageBuffer, ImageEncoder, Rgba, RgbaImage};

/// The upright image every fixture displays as: 6x4, every pixel distinct.
fn upright() -> RgbaImage {
    ImageBuffer::from_fn(6, 4, |x, y| {
        Rgba([(x * 40) as u8, (y * 60) as u8, 200, 255])
    })
}

/// A little-endian TIFF block holding one IFD entry: Orientation = `value`.
fn exif_orientation(value: u16) -> Vec<u8> {
    let mut exif = b"II*\0".to_vec();
    exif.extend_from_slice(&8u32.to_le_bytes());
    exif.extend_from_slice(&1u16.to_le_bytes());
    exif.extend_from_slice(&0x0112u16.to_le_bytes());
    exif.extend_from_slice(&3u16.to_le_bytes());
    exif.extend_from_slice(&1u32.to_le_bytes());
    exif.extend_from_slice(&value.to_le_bytes());
    exif.extend_from_slice(&[0, 0]);
    exif.extend_from_slice(&0u32.to_le_bytes());
    exif
}

/// PNG bytes of `stored` carrying an `eXIf` chunk with orientation `value`.
fn png_with_orientation(stored: &RgbaImage, value: u16) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut encoder = PngEncoder::new(&mut bytes);
    encoder
        .set_exif_metadata(exif_orientation(value))
        .expect("PNG accepts EXIF");
    encoder
        .write_image(
            stored.as_raw(),
            stored.width(),
            stored.height(),
            ExtendedColorType::Rgba8,
        )
        .expect("encode PNG fixture");
    bytes
}

/// The pixel buffer a camera stores for `upright` under EXIF `value`: the
/// inverse of the transform the orientation tag asks a viewer to apply.
fn stored_for(value: u16) -> RgbaImage {
    let up = upright();
    match value {
        1 => up,
        2 => imageops::flip_horizontal(&up),
        3 => imageops::rotate180(&up),
        6 => imageops::rotate270(&up),
        8 => imageops::rotate90(&up),
        other => panic!("no fixture for orientation {other}"),
    }
}

/// The decoded pixels of `decoded`, via `crop_decoded`'s raw RGBA output
/// over a view that maps the whole image 1:1.
fn pixels(decoded: &DecodedSource) -> (u32, u32, Vec<u8>) {
    let natural = decoded.natural_size();
    let out = crop_decoded(
        decoded,
        ViewTransform::default(),
        Stencil::rectangle(natural.width, natural.height),
        natural,
        CropOutput {
            size: SizeTarget::Native,
            format: OutputFormat::Rgba,
        },
    )
    .expect("crop");
    (out.width, out.height, out.bytes)
}

#[test]
fn decode_applies_orientation_by_default() {
    let up = upright();
    for value in [1, 2, 3, 6, 8] {
        let bytes = png_with_orientation(&stored_for(value), value);
        let decoded = DecodedSource::decode(&bytes).unwrap();
        assert_eq!(
            pixels(&decoded),
            (up.width(), up.height(), up.as_raw().clone()),
            "orientation {value}"
        );
    }
}

#[test]
fn decode_without_orientation_keeps_stored_pixels() {
    let off = DecodeOptions::default().with_orientation(false);
    for value in [3, 6, 8] {
        let stored = stored_for(value);
        let bytes = png_with_orientation(&stored, value);
        let decoded = DecodedSource::decode_with(&bytes, off).unwrap();
        assert_eq!(
            pixels(&decoded),
            (stored.width(), stored.height(), stored.as_raw().clone()),
            "orientation {value}"
        );
    }
}

#[test]
fn natural_size_follows_orientation() {
    let up = upright();
    for value in [1, 3, 6, 8] {
        let stored = stored_for(value);
        let bytes = png_with_orientation(&stored, value);
        assert_eq!(
            natural_size(&bytes).unwrap(),
            Size::new(up.width() as f32, up.height() as f32),
            "orientation {value}"
        );
        let off = DecodeOptions::default().with_orientation(false);
        assert_eq!(
            natural_size_with(&bytes, off).unwrap(),
            Size::new(stored.width() as f32, stored.height() as f32),
            "orientation {value}, not applied"
        );
    }
}

/// Wakes the blocked test thread.
struct ThreadWaker(Thread);

impl Wake for ThreadWaker {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}

/// Polls `future` to completion on the current thread.
fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let waker = Waker::from(Arc::new(ThreadWaker(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut cx) {
            return output;
        }
        std::thread::park();
    }
}

#[test]
fn crop_async_matches_crop_decoded() {
    let source: RgbaImage = ImageBuffer::from_fn(97, 61, |x, y| {
        Rgba([(x * 7 % 251) as u8, (y * 13 % 241) as u8, 90, 255])
    });
    let bytes = png_with_orientation(&source, 1);
    let viewport = Size::new(120.0, 120.0);
    let stencil = Stencil::rectangle(60.0, 40.0);
    let view = ViewTransform {
        offset: Point::new(7.5, -3.0),
        zoom: 1.7,
        rotation: 33.0,
    };
    for size in [
        SizeTarget::Native,
        SizeTarget::Exact {
            width: 20,
            height: 13,
        },
    ] {
        let output = CropOutput {
            size,
            format: OutputFormat::Png,
        };
        let expected = crop_decoded(
            &DecodedSource::decode(&bytes).unwrap(),
            view,
            stencil,
            viewport,
            output,
        )
        .unwrap();
        let got = block_on(crop_async(
            bytes.clone(),
            view,
            stencil,
            viewport,
            output,
            DecodeOptions::default(),
        ))
        .unwrap();
        assert_eq!(got, expected, "{size:?}");
    }
}

#[test]
fn crop_async_reports_decode_errors() {
    let result = block_on(crop_async(
        vec![0u8; 16],
        ViewTransform::default(),
        Stencil::square(10.0),
        Size::new(100.0, 100.0),
        CropOutput::default(),
        DecodeOptions::default(),
    ));
    assert!(matches!(result, Err(dioxus_cropper::CropError::Decode(_))));
}
