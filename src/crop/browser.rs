//! The browser crop path: the same crop [`crop_decoded`](super::crop_decoded)
//! produces, with the decode, resample and encode handed to the browser.
//! `createImageBitmap` decodes the source (applying EXIF orientation, as
//! the stage's `<img>` does), a downscaling crop resizes the bitmap to the
//! output resolution with `createImageBitmap`, a 2D canvas of the final
//! output size draws it through the module doc's transform, and `toBlob`
//! encodes the result. No
//! decoded copy of the source is allocated in WASM memory; only the
//! encoded output (or, for [`OutputFormat::Rgba`], the output pixels) is
//! copied back.

use super::{
    apply_size_target, native_output_size, CropError, CropOutput, CroppedFormat, CroppedOutput,
    DecodeOptions, OutputFormat,
};
use crate::geometry::{contain_scale, Size, Stencil, ViewTransform};
use js_sys::{Array, Promise, Reflect, Uint8Array};
use wasm_bindgen_futures::JsFuture;
use web_sys::wasm_bindgen::{JsCast, JsValue};
use web_sys::{
    Blob, CanvasRenderingContext2d, HtmlCanvasElement, ImageBitmap, ImageBitmapOptions,
    ImageOrientation, ResizeQuality,
};

/// [`crop_blob_in_browser`] over `source_bytes`, which are copied into a
/// `Blob` first.
///
/// # Errors
///
/// The same as [`crop_blob_in_browser`]; [`CropError::Decode`] also covers
/// a failure to build the `Blob`.
pub async fn crop_in_browser(
    source_bytes: &[u8],
    view: ViewTransform,
    stencil: Stencil,
    viewport: Size,
    output: CropOutput,
    decode: DecodeOptions,
) -> Result<CroppedOutput, CropError> {
    let parts = Array::of1(&Uint8Array::from(source_bytes));
    let blob = Blob::new_with_u8_array_sequence(&parts).map_err(|e| decode_error(&e))?;
    crop_blob_in_browser(&blob, view, stencil, viewport, output, decode).await
}

/// The crop [`crop_decoded`](super::crop_decoded) produces for the same
/// `view`, `stencil`, `viewport` and `output`, computed by the browser.
/// Only available on `wasm32`, where it needs a `Window` and `Document`.
///
/// The source's natural size is the decoded bitmap's, after EXIF
/// orientation unless `decode` turns it off — for an orientation 5–8
/// source, width and height are swapped relative to the file header.
/// [`Cropper`](crate::Cropper)'s `natural_size` must be that same size, as
/// [`natural_size_with`](super::natural_size_with) reports it for the same
/// `decode`.
///
/// Output dimensions are exactly those of [`output_size_with`](super::output_size_with)
/// for `output.size`. Pixels match `crop_decoded`'s within resampling
/// tolerance: the browser's filtered `drawImage` replaces nearest-neighbour
/// sampling, area averaging and bilinear upscaling, and encoders differ.
/// The circle stencil yields its square bounding box, unmasked. JPEG output
/// is composited onto [`JpegOptions::background`](super::JpegOptions::background).
///
/// Decode and encode run asynchronously in the browser; the calling task
/// yields while they run.
///
/// # Errors
///
/// Returns the validation errors [`crop_decoded`](super::crop_decoded)
/// returns for the same conditions. [`CropError::Decode`] if the browser
/// cannot decode `source`. [`CropError::Encode`] if the canvas cannot be
/// created, drawn or encoded — including an output larger than the
/// browser's canvas size limit, which is lower than [`MAX_OUTPUT_PIXELS`](super::MAX_OUTPUT_PIXELS)
/// in some engines.
pub async fn crop_blob_in_browser(
    source: &Blob,
    view: ViewTransform,
    stencil: Stencil,
    viewport: Size,
    output: CropOutput,
    decode: DecodeOptions,
) -> Result<CroppedOutput, CropError> {
    let ViewTransform {
        offset,
        zoom,
        rotation,
    } = view;
    if !offset.x.is_finite() || !offset.y.is_finite() || !zoom.is_finite() || !rotation.is_finite()
    {
        return Err(CropError::NonFiniteTransform);
    }
    if let OutputFormat::Jpeg(opts) = output.format {
        if opts.quality == 0 || opts.quality > 100 {
            return Err(CropError::InvalidOutputTarget);
        }
    }

    let mut bitmap = decode_bitmap(source, decode).await?;
    let natural = Size::new(bitmap.0.width() as f32, bitmap.0.height() as f32);
    let (nat_w, nat_h) = native_output_size(natural, viewport, stencil, zoom)?;
    let (out_w, out_h) = apply_size_target(nat_w, nat_h, output.size)?;

    let canvas = Canvas::new(out_w, out_h)?;
    let ctx = canvas.context_2d()?;

    if let OutputFormat::Jpeg(opts) = output.format {
        let [r, g, b] = opts.background;
        ctx.set_fill_style_str(&format!("rgb({r}, {g}, {b})"));
        ctx.fill_rect(0.0, 0.0, f64::from(out_w), f64::from(out_h));
    }

    ctx.set_image_smoothing_enabled(true);
    // `imageSmoothingQuality` has no web-sys binding; engines without it
    // ignore the property.
    let _ = Reflect::set(
        &ctx,
        &JsValue::from_str("imageSmoothingQuality"),
        &JsValue::from_str("high"),
    );

    // The module doc's forward map, source index `s` to native-crop index
    // `n`: `n = Rotate(rotation) * (s - src / 2) + offset / scale + nat / 2`,
    // then scaled per axis by `out / nat` onto the final output. Canvas
    // coordinates address pixel areas, so the map is conjugated by a
    // half-pixel shift (`p - 0.5` in, `+ 0.5` out) to put source pixel
    // centres on the output pixel centres `crop_decoded` samples.
    let fit_scale = contain_scale(natural, viewport);
    let scale = f64::from(fit_scale) * f64::from(zoom);
    let (sin, cos) = f64::from(rotation).to_radians().sin_cos();
    let kx = f64::from(out_w) / f64::from(nat_w);
    let ky = f64::from(out_h) / f64::from(nat_h);
    let hx = f64::from(natural.width) / 2.0 + 0.5;
    let hy = f64::from(natural.height) / 2.0 + 0.5;
    let ex = -(cos * hx - sin * hy) + f64::from(offset.x) / scale + f64::from(nat_w) / 2.0 + 0.5;
    let ey = -(sin * hx + cos * hy) + f64::from(offset.y) / scale + f64::from(nat_h) / 2.0 + 0.5;

    // A downscaling crop draws from a copy resized to the output
    // resolution: `createImageBitmap` resizes off the main thread, where a
    // canvas draw of the full-size bitmap rasterises on it. Bitmap pixel
    // `q` covers source area `q * src / bitmap`, folded into the matrix's
    // x and y columns. An engine that ignores the resize options returns
    // the full-size bitmap, and the fold reduces to the identity.
    let shrink = kx.max(ky);
    if shrink < 1.0 {
        let width = (f64::from(natural.width) * shrink).round().max(1.0) as u32;
        let height = (f64::from(natural.height) * shrink).round().max(1.0) as u32;
        bitmap = resize(&bitmap, width, height).await?;
    }
    let rx = f64::from(natural.width) / f64::from(bitmap.0.width());
    let ry = f64::from(natural.height) / f64::from(bitmap.0.height());

    ctx.set_transform(
        kx * cos * rx,
        ky * sin * rx,
        -kx * sin * ry,
        ky * cos * ry,
        kx * ex,
        ky * ey,
    )
    .map_err(|e| encode_error(&e))?;
    ctx.draw_image_with_image_bitmap(&bitmap.0, 0.0, 0.0)
        .map_err(|e| encode_error(&e))?;
    drop(bitmap);

    let (format, bytes) = match output.format {
        OutputFormat::Png => (CroppedFormat::Png, canvas.encode("image/png", None).await?),
        OutputFormat::Jpeg(opts) => {
            let quality = f64::from(opts.quality) / 100.0;
            let bytes = canvas.encode("image/jpeg", Some(quality)).await?;
            (CroppedFormat::Jpeg, bytes)
        }
        OutputFormat::Rgba => {
            let data = ctx
                .get_image_data(0.0, 0.0, f64::from(out_w), f64::from(out_h))
                .map_err(|e| encode_error(&e))?;
            (CroppedFormat::Rgba, data.data().0)
        }
    };

    Ok(CroppedOutput {
        width: out_w,
        height: out_h,
        format,
        bytes,
    })
}

/// A decoded `ImageBitmap`, closed on drop to release its pixels.
struct Bitmap(ImageBitmap);

impl Drop for Bitmap {
    fn drop(&mut self) {
        self.0.close();
    }
}

/// Decodes `source` with `createImageBitmap`, applying EXIF orientation
/// when `decode` asks for it.
async fn decode_bitmap(source: &Blob, decode: DecodeOptions) -> Result<Bitmap, CropError> {
    let window = web_sys::window().ok_or_else(|| CropError::Decode("no window".into()))?;
    let options = ImageBitmapOptions::new();
    if decode.apply_orientation {
        options.set_image_orientation(ImageOrientation::FromImage);
    } else {
        // `"none"` has no `ImageOrientation` variant in web-sys.
        Reflect::set(
            &options,
            &JsValue::from_str("imageOrientation"),
            &JsValue::from_str("none"),
        )
        .map_err(|e| decode_error(&e))?;
    }
    let promise = window
        .create_image_bitmap_with_blob_and_image_bitmap_options(source, &options)
        .map_err(|e| decode_error(&e))?;
    let bitmap = JsFuture::from(promise)
        .await
        .map_err(|e| decode_error(&e))?;
    bitmap
        .dyn_into::<ImageBitmap>()
        .map(Bitmap)
        .map_err(|e| decode_error(&e))
}

/// Resizes `bitmap` to `width`×`height` with `createImageBitmap`, at high
/// resampling quality.
async fn resize(bitmap: &Bitmap, width: u32, height: u32) -> Result<Bitmap, CropError> {
    let window = web_sys::window().ok_or_else(|| CropError::Encode("no window".into()))?;
    let options = ImageBitmapOptions::new();
    options.set_resize_width(width);
    options.set_resize_height(height);
    options.set_resize_quality(ResizeQuality::High);
    let promise = window
        .create_image_bitmap_with_image_bitmap_and_image_bitmap_options(&bitmap.0, &options)
        .map_err(|e| encode_error(&e))?;
    JsFuture::from(promise)
        .await
        .map_err(|e| encode_error(&e))?
        .dyn_into::<ImageBitmap>()
        .map(Bitmap)
        .map_err(|e| encode_error(&e))
}

/// A detached canvas, shrunk to zero size on drop so engines release its
/// backing store without waiting for garbage collection.
struct Canvas(HtmlCanvasElement);

impl Canvas {
    fn new(width: u32, height: u32) -> Result<Self, CropError> {
        let document = web_sys::window()
            .and_then(|w| w.document())
            .ok_or_else(|| CropError::Encode("no document".into()))?;
        let canvas = document
            .create_element("canvas")
            .map_err(|e| encode_error(&e))?
            .dyn_into::<HtmlCanvasElement>()
            .map_err(|e| encode_error(&e))?;
        canvas.set_width(width);
        canvas.set_height(height);
        Ok(Self(canvas))
    }

    fn context_2d(&self) -> Result<CanvasRenderingContext2d, CropError> {
        self.0
            .get_context("2d")
            .map_err(|e| encode_error(&e))?
            .ok_or_else(|| CropError::Encode("canvas has no 2d context".into()))?
            .dyn_into::<CanvasRenderingContext2d>()
            .map_err(|e| encode_error(&e))
    }

    /// Encodes the canvas with `toBlob` as `mime`, at `quality` in `0..=1`
    /// for lossy formats, and copies the encoded bytes out.
    async fn encode(&self, mime: &str, quality: Option<f64>) -> Result<Vec<u8>, CropError> {
        let promise = Promise::new(&mut |resolve, reject| {
            let started = match quality {
                Some(q) => self.0.to_blob_with_type_and_encoder_options(
                    &resolve,
                    mime,
                    &JsValue::from_f64(q),
                ),
                None => self.0.to_blob_with_type(&resolve, mime),
            };
            if let Err(e) = started {
                let _ = reject.call1(&JsValue::UNDEFINED, &e);
            }
        });
        let blob = JsFuture::from(promise)
            .await
            .map_err(|e| encode_error(&e))?
            .dyn_into::<Blob>()
            .map_err(|_| {
                CropError::Encode(
                    "canvas produced no image; the output may exceed the browser's canvas size limit"
                        .into(),
                )
            })?;
        let buffer = JsFuture::from(blob.array_buffer())
            .await
            .map_err(|e| encode_error(&e))?;
        Ok(Uint8Array::new(&buffer).to_vec())
    }
}

impl Drop for Canvas {
    fn drop(&mut self) {
        self.0.set_width(0);
        self.0.set_height(0);
    }
}

fn decode_error(e: &JsValue) -> CropError {
    CropError::Decode(js_message(e).into())
}

fn encode_error(e: &JsValue) -> CropError {
    CropError::Encode(js_message(e).into())
}

/// The message of a thrown JS value: an `Error`'s `message`, a thrown
/// string as-is, otherwise its debug form.
fn js_message(e: &JsValue) -> String {
    e.dyn_ref::<js_sys::Error>()
        .map(|err| String::from(err.message()))
        .or_else(|| e.as_string())
        .unwrap_or_else(|| format!("{e:?}"))
}
