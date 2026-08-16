//! Turns the caller's original source bytes plus the
//! [`ViewTransform`]/[`Stencil`] pair that [`Cropper`](crate::Cropper) is
//! rendering into the cut-out image the user actually sees inside the
//! stencil. No `dioxus` dependency — this is plain, host-agnostic geometry
//! and pixel sampling, exercised without a renderer.
//!
//! ## Deriving the screen-to-source pixel mapping
//!
//! [`Cropper`](crate::Cropper)'s `<img>` element is sized inline to exactly
//! `natural_size` (the decoded image's real pixel dimensions), then scaled
//! by CSS `scale({scale})` in its `transform`, where `scale = fit_scale *
//! zoom` and `fit_scale = `[`contain_scale`]`(natural_size, viewport)`
//! — the factor that fits `natural_size` inside the fixed viewport while
//! preserving aspect ratio, the same way CSS `object-fit: contain` would.
//! One screen pixel is thus `1 / scale` source (natural) pixels, not
//! `1 / zoom`.
//!
//! The image element's transform is
//! `translate(calc(-50% + offset.x), calc(-50% + offset.y)) rotate(rotation) scale(scale)`,
//! applied to an element already centred in the viewport via
//! `top/left: 50%`. CSS composes transform functions right-to-left against
//! the element's own local box: `scale` acts first (in the image's own,
//! unscaled coordinate system, origin at its own centre — CSS's default
//! `transform-origin`), then `rotate` about that same centre, then
//! `translate` — which, applied last, is a plain vector add of screen
//! pixels, untouched by the scale or rotation that already happened. So,
//! writing `c` for a source pixel's position relative to the image's own
//! centre (in natural, unscaled pixels) and `screen` for the corresponding
//! point relative to the viewport's centre (where `Cropper` centres both the
//! image's `top/left: 50%` anchor and the stencil itself, so the stencil's
//! centre sits at the image's centre displaced by `-offset` screen pixels):
//!
//! ```text
//! screen = Rotate(rotation) * (scale * c) + offset
//! ```
//!
//! `Rotate(theta)` is the standard rotation matrix
//! `[cos θ, -sin θ; sin θ, cos θ]`; CSS's positive `rotate()` angle turns the
//! element clockwise on screen, which is exactly what this matrix produces
//! in the screen's own y-down coordinate system (`(1, 0)` at `theta = 90°`
//! maps to `(0, 1)`, i.e. east to south).
//!
//! Inverting for the sampling loop, given a target `screen` point:
//!
//! ```text
//! c = Rotate(-rotation) * (screen - offset) / scale
//! ```
//!
//! and the natural source-pixel coordinate is `c` plus the source image's
//! own centre, `(width / 2, height / 2)`.
//!
//! The output image is sized `stencil_size / scale` (in source pixels), not
//! the stencil's screen size: a source pixel maps to one output pixel, so
//! cropping at high `scale` does not upscale, and cropping at low `scale`
//! does not throw away resolution. Output pixel `(ox, oy)`,
//! relative to the output image's own centre, corresponds to
//! `screen = (ox - out_w/2, oy - out_h/2) * scale` — the `* scale`
//! re-expands the output pixel back into the full-resolution screen pixels
//! the stencil actually spans, so the inversion above is evaluated at
//! exactly the point the user saw inside the stencil.
//!
//! Worked example: a 1920-wide source image, a 480px viewport, `zoom = 1.0`.
//! `fit_scale = 480 / 1920 = 0.25`, so `scale = 0.25`. A 240px stencil then
//! yields `240 / 0.25 = 960` source pixels per side.

use crate::geometry::{contain_scale, Size, Stencil, ViewTransform};
use image::{ImageBuffer, Rgba, RgbaImage};
use std::sync::Arc;

/// The cropped image: PNG-encoded bytes plus the pixel dimensions the
/// caller needs to display or sanity-check the result. Fields are the
/// dimensions of `png_bytes`, not the stencil's on-screen size — see the
/// module doc for why they differ whenever `fit_scale * zoom != 1.0`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CroppedImage {
    /// The cropped image's width, in pixels.
    pub width: u32,
    /// The cropped image's height, in pixels.
    pub height: u32,
    /// The cropped image, PNG-encoded.
    pub png_bytes: Vec<u8>,
}

/// Everything that can go wrong producing a crop.
#[non_exhaustive]
#[derive(Debug)]
pub enum CropError {
    /// `view`'s `offset.x`, `offset.y`, `zoom` or `rotation` is `NaN` or
    /// infinite — the transform this crate renders has no interpretation
    /// for a non-finite value, so there is nothing correct to sample.
    NonFiniteTransform,
    /// `zoom` is not a positive, finite number — a `zoom` of `0.0` or
    /// negative would divide by zero or mirror/invert the image.
    InvalidZoom,
    /// `stencil`'s width or height is not a positive, finite number — there
    /// is no non-empty region to cut.
    EmptyStencil,
    /// `viewport`'s width or height is not a positive, finite number — an
    /// empty or non-finite viewport would make `fit_scale` zero, non-finite,
    /// or produce a divide-by-zero downstream.
    EmptyViewport,
    /// `natural`'s width or height is not a positive, finite number — there
    /// is no real image extent to derive a scale or an output size from.
    EmptyNatural,
    /// The computed output would exceed [`MAX_OUTPUT_PIXELS`] pixels. Carries
    /// the computed `(width, height)` so the caller can report them. Reached
    /// at low `zoom` against a small viewport — a small
    /// `stencil / (fit_scale * zoom)` ratio blows the output up rather than
    /// down.
    OutputTooLarge {
        /// The computed output width, in pixels, that exceeded the limit.
        width: u32,
        /// The computed output height, in pixels, that exceeded the limit.
        height: u32,
    },
    /// `source_bytes` could not be decoded as an image.
    Decode(Box<dyn std::error::Error + Send + Sync>),
    /// The sampled result could not be PNG-encoded.
    Encode(Box<dyn std::error::Error + Send + Sync>),
}

impl std::fmt::Display for CropError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFiniteTransform => {
                write!(f, "view transform contains a non-finite value")
            }
            Self::InvalidZoom => write!(f, "zoom must be a positive, finite number"),
            Self::EmptyStencil => write!(f, "stencil has zero or negative width/height"),
            Self::EmptyViewport => {
                write!(f, "viewport has zero, negative, or non-finite width/height")
            }
            Self::EmptyNatural => {
                write!(f, "natural has zero, negative, or non-finite width/height")
            }
            Self::OutputTooLarge { width, height } => write!(
                f,
                "computed output {width}x{height} exceeds the {MAX_OUTPUT_PIXELS}-pixel limit"
            ),
            Self::Decode(e) => write!(f, "could not decode source image: {e}"),
            Self::Encode(e) => write!(f, "could not encode cropped image: {e}"),
        }
    }
}

impl std::error::Error for CropError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Decode(e) | Self::Encode(e) => Some(e.as_ref()),
            Self::NonFiniteTransform
            | Self::InvalidZoom
            | Self::EmptyStencil
            | Self::EmptyViewport
            | Self::EmptyNatural
            | Self::OutputTooLarge { .. } => None,
        }
    }
}

/// Produces exactly what `Cropper` shows inside `stencil` for the given
/// `view`, by decoding `source_bytes` and inverse-sampling every output
/// pixel back through the transform (see the module doc for the
/// derivation). Nearest-neighbour sampling; a sample that lands outside the
/// decoded source is filled transparent rather than erroring — the
/// stencil's position is unclamped, so the caller can legitimately frame
/// part or all of it over empty space.
///
/// `stencil`'s shape is not applied here: a circle stencil yields its
/// square bounding box, unmasked, so a caller can round it with CSS at
/// display time rather than lose pixels to a baked-in alpha mask.
///
/// `viewport` must be the exact same value passed as
/// [`Cropper`](crate::Cropper)'s `viewport` prop for this `view` — the same
/// requirement `natural_size` carries (see [`DecodedSource::natural_size`]'s
/// doc). Nothing links a `Cropper` a caller rendered to a later
/// `crop_to_png` call except the caller supplying the identical `Size` to
/// both — construct it once and pass the same value to both call sites.
///
/// This decodes `source_bytes` fresh on every call — decode dominates the
/// pipeline's cost. A caller offering a repeatable "Crop" action against the
/// same picked file must decode once via [`DecodedSource::decode`] and call
/// [`crop_decoded_to_png`] per press instead of this function.
///
/// # Errors
///
/// Returns [`CropError::NonFiniteTransform`] if any of `view`'s fields is
/// `NaN` or infinite, [`CropError::InvalidZoom`] if `view.zoom` is not
/// positive and finite, [`CropError::EmptyStencil`] if `stencil`'s width or
/// height is not positive and finite, [`CropError::EmptyViewport`] if
/// `viewport`'s width or height is not positive and finite, and
/// [`CropError::Decode`] if `source_bytes` cannot be decoded as an image.
/// [`CropError::Encode`] is returned if the sampled result cannot be
/// PNG-encoded.
pub fn crop_to_png(
    source_bytes: &[u8],
    view: ViewTransform,
    stencil: Stencil,
    viewport: Size,
) -> Result<CroppedImage, CropError> {
    let decoded = DecodedSource::decode(source_bytes)?;
    crop_decoded_to_png(&decoded, view, stencil, viewport)
}

/// A source image decoded once, held ready for repeated crops. Decoding a
/// multi-megapixel photograph dominates `crop_to_png`'s cost — a caller that
/// lets the user press "Crop" more than once against the same picked file
/// must decode once with [`Self::decode`] and reuse it via
/// [`crop_decoded_to_png`], not call [`crop_to_png`] per press.
///
/// Cloning is a shared-handle copy — a refcount bump over an `Arc`, not a
/// copy of the underlying pixel buffer.
#[derive(Debug, Clone)]
pub struct DecodedSource(Arc<RgbaImage>);

impl DecodedSource {
    /// Decodes `source_bytes` once. Cache the result across repeated crops
    /// of the same picked file.
    ///
    /// # Errors
    ///
    /// Returns [`CropError::Decode`] if `source_bytes` cannot be decoded as
    /// an image.
    pub fn decode(source_bytes: &[u8]) -> Result<Self, CropError> {
        image::load_from_memory(source_bytes)
            .map(|img| Self(Arc::new(img.to_rgba8())))
            .map_err(|e| CropError::Decode(Box::new(e)))
    }

    /// The decoded image's real pixel dimensions — exactly what a caller
    /// must pass as [`Cropper`](crate::Cropper)'s `natural_size` prop, so
    /// the component's rendered fit and this module's crop maths agree on
    /// the same image.
    pub fn natural_size(&self) -> Size {
        Size::new(self.0.width() as f32, self.0.height() as f32)
    }
}

/// The largest output area, in pixels (width × height), [`output_size`] will
/// return. 64 megapixels — generous for any real crop, and small enough that
/// the resulting RGBA buffer (256 MB) stays well inside a 32-bit `usize`
/// address space, avoiding the overflow/allocation failure this limit
/// guards against. Pre-check against this constant to avoid provoking
/// [`CropError::OutputTooLarge`].
pub const MAX_OUTPUT_PIXELS: u64 = 64 * 1024 * 1024;

/// The pixel dimensions [`crop_decoded_to_png`] (and [`crop_to_png`]) would
/// produce for the given `natural`, `viewport`, `stencil` and `zoom`,
/// without decoding or sampling any pixels. `crop_decoded_to_png` calls this
/// function for its own output dimensions, so the two can never drift.
///
/// # Errors
///
/// Returns [`CropError::NonFiniteTransform`] if `zoom` is `NaN` or infinite,
/// [`CropError::InvalidZoom`] if `zoom` is not positive, [`CropError::EmptyStencil`]
/// if `stencil`'s width or height is not positive and finite,
/// [`CropError::EmptyViewport`] if `viewport`'s width or height is not
/// positive and finite, [`CropError::EmptyNatural`] if `natural`'s width
/// or height is not positive and finite, and [`CropError::OutputTooLarge`]
/// if the computed output area would exceed [`MAX_OUTPUT_PIXELS`].
pub fn output_size(
    natural: Size,
    viewport: Size,
    stencil: Stencil,
    zoom: f32,
) -> Result<(u32, u32), CropError> {
    if !zoom.is_finite() {
        return Err(CropError::NonFiniteTransform);
    }
    if zoom <= 0.0 {
        return Err(CropError::InvalidZoom);
    }
    let stencil_w_ok = stencil.width().is_finite() && stencil.width() > 0.0;
    let stencil_h_ok = stencil.height().is_finite() && stencil.height() > 0.0;
    if !stencil_w_ok || !stencil_h_ok {
        return Err(CropError::EmptyStencil);
    }
    let viewport_w_ok = viewport.width.is_finite() && viewport.width > 0.0;
    let viewport_h_ok = viewport.height.is_finite() && viewport.height > 0.0;
    if !viewport_w_ok || !viewport_h_ok {
        return Err(CropError::EmptyViewport);
    }
    let natural_w_ok = natural.width.is_finite() && natural.width > 0.0;
    let natural_h_ok = natural.height.is_finite() && natural.height > 0.0;
    if !natural_w_ok || !natural_h_ok {
        return Err(CropError::EmptyNatural);
    }

    // The same fit scale `Cropper` renders the image at (see module doc):
    // the combined source-to-screen scale is `fit_scale * zoom`, not `zoom`
    // alone, whenever the source's natural size differs from the viewport.
    let fit_scale = contain_scale(natural, viewport);
    let scale = fit_scale * zoom;

    // `.max(1)` guards only against the output resolving to zero pixels
    // through rounding (e.g. a tiny stencil at very high zoom) — `stencil`
    // was already confirmed non-empty above, so this never masks the
    // `EmptyStencil` case, only float rounding at its edge.
    let out_w = ((stencil.width() / scale).round() as i64).clamp(1, u32::MAX as i64) as u32;
    let out_h = ((stencil.height() / scale).round() as i64).clamp(1, u32::MAX as i64) as u32;

    // Computed as `u64` so the area itself cannot overflow while checking it
    // — `u32::MAX * u32::MAX` overflows `u32` but not `u64`.
    if u64::from(out_w) * u64::from(out_h) > MAX_OUTPUT_PIXELS {
        return Err(CropError::OutputTooLarge {
            width: out_w,
            height: out_h,
        });
    }

    Ok((out_w, out_h))
}

/// Same as [`crop_to_png`], but against an already-decoded source — the
/// call this crate expects a repeated-crop caller to make instead of
/// re-decoding the original bytes on every press.
///
/// `viewport` carries the same must-match-the-component requirement
/// documented on [`crop_to_png`] — see there.
///
/// # Errors
///
/// Returns the same error variants as [`crop_to_png`], for the same
/// conditions, except [`CropError::Decode`] — `decoded` is already decoded.
pub fn crop_decoded_to_png(
    decoded: &DecodedSource,
    view: ViewTransform,
    stencil: Stencil,
    viewport: Size,
) -> Result<CroppedImage, CropError> {
    let ViewTransform {
        offset,
        zoom,
        rotation,
    } = view;

    if !offset.x.is_finite() || !offset.y.is_finite() || !zoom.is_finite() || !rotation.is_finite()
    {
        return Err(CropError::NonFiniteTransform);
    }

    let (out_w, out_h) = output_size(decoded.natural_size(), viewport, stencil, zoom)?;

    let source = decoded.0.as_ref();
    let (src_w, src_h) = (source.width() as f32, source.height() as f32);

    let fit_scale = contain_scale(decoded.natural_size(), viewport);
    let scale = fit_scale * zoom;

    let rotation_rad = rotation.to_radians();
    // Sampling needs the INVERSE rotation (see module doc): `Rotate(-rotation)`.
    let (sin_inv, cos_inv) = (-rotation_rad).sin_cos();

    let output: RgbaImage = ImageBuffer::from_fn(out_w, out_h, |ox, oy| {
        let screen_x = (ox as f32 - out_w as f32 / 2.0) * scale;
        let screen_y = (oy as f32 - out_h as f32 / 2.0) * scale;

        let dx = screen_x - offset.x;
        let dy = screen_y - offset.y;

        let cx = (dx * cos_inv - dy * sin_inv) / scale;
        let cy = (dx * sin_inv + dy * cos_inv) / scale;

        let sx = cx + src_w / 2.0;
        let sy = cy + src_h / 2.0;

        sample_nearest(source, sx, sy)
    });

    let mut png_bytes = Vec::new();
    output
        .write_to(
            &mut std::io::Cursor::new(&mut png_bytes),
            image::ImageFormat::Png,
        )
        .map_err(|e| CropError::Encode(Box::new(e)))?;

    Ok(CroppedImage {
        width: out_w,
        height: out_h,
        png_bytes,
    })
}

/// Nearest-neighbour lookup at a continuous source coordinate. Rounds to
/// the nearest pixel index and returns fully transparent for anything that
/// rounds outside the source's bounds — the caller's stencil position is
/// unclamped, so out-of-source samples are an expected, non-error case.
fn sample_nearest(source: &RgbaImage, sx: f32, sy: f32) -> Rgba<u8> {
    let ix = sx.round();
    let iy = sy.round();
    if ix < 0.0 || iy < 0.0 || !ix.is_finite() || !iy.is_finite() {
        return Rgba([0, 0, 0, 0]);
    }
    let (ix, iy) = (ix as u32, iy as u32);
    if ix >= source.width() || iy >= source.height() {
        Rgba([0, 0, 0, 0])
    } else {
        *source.get_pixel(ix, iy)
    }
}
