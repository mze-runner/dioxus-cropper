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

use crate::geometry::{contain_scale, normalize_rotation, Point, Size, Stencil, ViewTransform};
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::{ExtendedColorType, ImageEncoder, RgbaImage};
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
    /// The cropped image, PNG-encoded. Encoded for speed rather than for
    /// minimum size — the pixels are identical either way, so a caller
    /// archiving crops long-term can re-encode with a heavier compressor
    /// without loss.
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
    /// The requested output target cannot be satisfied: a
    /// [`SizeTarget::Exact`] width or height of zero, a
    /// [`SizeTarget::MaxDimension`] of zero, or a [`JpegOptions::quality`]
    /// of zero or above 100.
    InvalidOutputTarget,
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
            Self::InvalidOutputTarget => write!(
                f,
                "output target has a zero dimension or a JPEG quality outside 1-100"
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
            | Self::OutputTooLarge { .. }
            | Self::InvalidOutputTarget => None,
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
/// [`CropError::Decode`] if `source_bytes` cannot be decoded as an image or
/// exceeds the decode limits documented on [`DecodedSource::decode`].
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

/// The largest source width or height, in pixels, [`DecodedSource::decode`]
/// accepts. 16384 per side covers an 8K frame with generous headroom, and
/// refuses decoder bombs whose claimed dimensions are hostile even when
/// their predicted allocation squeaks under the byte cap.
const MAX_SOURCE_DIMENSION: u32 = 16_384;

impl DecodedSource {
    /// Decodes `source_bytes` once. Cache the result across repeated crops
    /// of the same picked file.
    ///
    /// # Errors
    ///
    /// Returns [`CropError::Decode`] if `source_bytes` cannot be decoded as
    /// an image. Decoding is limited: a source whose header claims a width
    /// or height above 16384 pixels, or whose decoding would allocate more
    /// than the `image` crate's default allocation limit, is refused with
    /// the same [`CropError::Decode`] before any pixel work — the dimension
    /// cap covers an 8K source with generous headroom while rejecting
    /// decoder bombs whose claimed dimensions are hostile even when their
    /// predicted allocation stays under the byte cap.
    pub fn decode(source_bytes: &[u8]) -> Result<Self, CropError> {
        // Keep the default allocation limit; the dimension caps are strict
        // and checked against the header before any decoding happens.
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(MAX_SOURCE_DIMENSION);
        limits.max_image_height = Some(MAX_SOURCE_DIMENSION);

        let mut reader = image::ImageReader::new(std::io::Cursor::new(source_bytes))
            .with_guessed_format()
            .map_err(|e| CropError::Decode(Box::new(e)))?;
        reader.limits(limits);

        // `into_rgba8` reuses the decoded buffer when the source is already
        // RGBA8, where `to_rgba8` would copy it unconditionally.
        reader
            .decode()
            .map(|img| Self(Arc::new(img.into_rgba8())))
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
    let (out_w, out_h) = native_output_size(natural, viewport, stencil, zoom)?;
    check_output_area(out_w, out_h)?;
    Ok((out_w, out_h))
}

/// Validates `natural`, `viewport`, `stencil` and `zoom` and computes the
/// native output dimensions — `stencil / (fit_scale * zoom)`, rounded and
/// clamped to at least 1 per axis. Shared by [`output_size`],
/// [`output_size_with`] and [`crop_decoded`]. Does not apply the
/// [`MAX_OUTPUT_PIXELS`] check — the caller checks the dimensions it will
/// actually allocate for.
fn native_output_size(
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

    Ok((out_w, out_h))
}

/// Rejects a `width`×`height` output whose area exceeds
/// [`MAX_OUTPUT_PIXELS`], as [`CropError::OutputTooLarge`].
fn check_output_area(width: u32, height: u32) -> Result<(), CropError> {
    // Computed as `u64` so the area itself cannot overflow while checking it
    // — `u32::MAX * u32::MAX` overflows `u32` but not `u64`.
    if u64::from(width) * u64::from(height) > MAX_OUTPUT_PIXELS {
        return Err(CropError::OutputTooLarge { width, height });
    }
    Ok(())
}

/// What [`crop_decoded`] produces: the output's pixel dimensions and its
/// byte format. The default — [`SizeTarget::Native`] plus
/// [`OutputFormat::Png`] — makes [`crop_decoded`] produce exactly the bytes
/// [`crop_decoded_to_png`] does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CropOutput {
    /// The output's pixel dimensions, relative to the native crop size.
    pub size: SizeTarget,
    /// The byte format `bytes` is delivered in.
    pub format: OutputFormat,
}

/// The output's pixel dimensions, expressed relative to the native crop
/// size — the `stencil / (fit_scale * zoom)` dimensions [`output_size`]
/// computes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SizeTarget {
    /// The native crop size, unchanged — identical dimensions (and, from
    /// [`crop_decoded`], identical pixels) to [`crop_decoded_to_png`].
    #[default]
    Native,
    /// Exactly `width`×`height` pixels, regardless of the native crop size.
    /// The native aspect ratio is NOT preserved: a `width`/`height` ratio
    /// that differs from the stencil's distorts the image, and avoiding
    /// that is the caller's responsibility.
    Exact {
        /// The output width, in pixels. Must be non-zero.
        width: u32,
        /// The output height, in pixels. Must be non-zero.
        height: u32,
    },
    /// The native crop size, downscaled (aspect-preserving) so its longer
    /// side is at most this many pixels. A native crop already within the
    /// cap is left at its native size — this target never upscales. Must be
    /// non-zero.
    MaxDimension(u32),
}

/// The byte format [`crop_decoded`] encodes the sampled pixels into.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    /// PNG, with the same encoder settings as [`crop_decoded_to_png`].
    #[default]
    Png,
    /// JPEG. JPEG has no alpha channel, so pixels are composited onto
    /// [`JpegOptions::background`] before encoding.
    Jpeg(JpegOptions),
    /// The raw RGBA8 pixel buffer, row-major, 4 bytes per pixel — no
    /// encoding.
    Rgba,
}

/// Settings for [`OutputFormat::Jpeg`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JpegOptions {
    /// JPEG quality, `1..=100`. A value of zero or above 100 makes
    /// [`crop_decoded`] return [`CropError::InvalidOutputTarget`].
    pub quality: u8,
    /// The opaque RGB colour transparent and semi-transparent pixels are
    /// composited onto: `out = bg * (255 - a) / 255 + c * a / 255`, rounded
    /// per channel.
    pub background: [u8; 3],
}

/// Quality 80 on a black background.
impl Default for JpegOptions {
    fn default() -> Self {
        Self {
            quality: 80,
            background: [0, 0, 0],
        }
    }
}

/// The byte format a [`CroppedOutput::bytes`] buffer actually holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CroppedFormat {
    /// PNG-encoded bytes.
    Png,
    /// JPEG-encoded bytes.
    Jpeg,
    /// Raw RGBA8 bytes, row-major, 4 bytes per pixel.
    Rgba,
}

/// The cropped image [`crop_decoded`] returns: the pixel dimensions of the
/// output plus its bytes in the format the caller requested.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CroppedOutput {
    /// The output's width, in pixels.
    pub width: u32,
    /// The output's height, in pixels.
    pub height: u32,
    /// The format `bytes` holds.
    pub format: CroppedFormat,
    /// The output image, encoded per `format`.
    pub bytes: Vec<u8>,
}

/// The pixel dimensions [`crop_decoded`] would produce for the given
/// inputs and `target`, without decoding or sampling any pixels.
/// `crop_decoded` computes its own output dimensions through the same
/// code, so the two cannot drift. With [`SizeTarget::Native`] this returns
/// exactly what [`output_size`] returns, including its errors.
///
/// # Errors
///
/// Returns every input-validation error [`output_size`] documents, for the
/// same conditions; [`CropError::InvalidOutputTarget`] if `target` is
/// [`SizeTarget::Exact`] with a zero width or height or
/// [`SizeTarget::MaxDimension`]`(0)`; and [`CropError::OutputTooLarge`] if
/// the FINAL dimensions — after `target` is applied — would exceed
/// [`MAX_OUTPUT_PIXELS`]. Native dimensions above the limit are not an
/// error when `target` shrinks the output below it.
pub fn output_size_with(
    natural: Size,
    viewport: Size,
    stencil: Stencil,
    zoom: f32,
    target: SizeTarget,
) -> Result<(u32, u32), CropError> {
    let (nat_w, nat_h) = native_output_size(natural, viewport, stencil, zoom)?;
    apply_size_target(nat_w, nat_h, target)
}

/// Applies `target` to the native output dimensions and checks the FINAL
/// dimensions against [`MAX_OUTPUT_PIXELS`].
fn apply_size_target(nat_w: u32, nat_h: u32, target: SizeTarget) -> Result<(u32, u32), CropError> {
    let (out_w, out_h) = match target {
        SizeTarget::Native => (nat_w, nat_h),
        SizeTarget::Exact { width, height } => {
            if width == 0 || height == 0 {
                return Err(CropError::InvalidOutputTarget);
            }
            (width, height)
        }
        SizeTarget::MaxDimension(cap) => {
            if cap == 0 {
                return Err(CropError::InvalidOutputTarget);
            }
            let longest = nat_w.max(nat_h);
            if longest > cap {
                // Aspect-preserving shrink of both axes by `cap / longest`;
                // the longer side lands exactly on `cap`, the shorter one
                // rounds, clamped to at least 1 pixel.
                let factor = f64::from(cap) / f64::from(longest);
                (
                    (f64::from(nat_w) * factor).round().max(1.0) as u32,
                    (f64::from(nat_h) * factor).round().max(1.0) as u32,
                )
            } else {
                (nat_w, nat_h)
            }
        }
    };
    check_output_area(out_w, out_h)?;
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

    // Sampling arithmetic runs in f64: an f32 mantissa (24 bits) spends 13
    // of them on the integer part of an 8K coordinate, leaving well under a
    // thousandth of a pixel — too close to `.round()`'s decision boundary
    // once a rotation and a division have each contributed their own
    // half-ulp. The f32 inputs widen into f64 exactly, so nothing is lost
    // on the way in.
    let fit_scale = contain_scale(decoded.natural_size(), viewport);
    let scale = f64::from(fit_scale) * f64::from(zoom);

    let pixels = rasterize(source, out_w, out_h, scale, offset, rotation);

    // Fast DEFLATE with adaptive per-row filtering. The capacity assumes
    // RGBA deflates at least 4:1 — a heuristic, not a bound.
    let mut png_bytes = Vec::with_capacity(out_w as usize * out_h as usize);
    PngEncoder::new_with_quality(&mut png_bytes, CompressionType::Fast, FilterType::Adaptive)
        .write_image(&pixels, out_w, out_h, ExtendedColorType::Rgba8)
        .map_err(|e| CropError::Encode(Box::new(e)))?;

    Ok(CroppedImage {
        width: out_w,
        height: out_h,
        png_bytes,
    })
}

/// Same crop as [`crop_decoded_to_png`], with the output's dimensions and
/// byte format selected by `output`. With the default `output` —
/// [`SizeTarget::Native`] plus [`OutputFormat::Png`] — the result's bytes
/// are identical to [`crop_decoded_to_png`]'s `png_bytes`.
///
/// When the final dimensions equal the native crop size (including
/// [`SizeTarget::Native`] and a [`SizeTarget::MaxDimension`] cap the native
/// size already satisfies), pixels are sampled exactly as
/// [`crop_decoded_to_png`] samples them. A downscaling target is resampled
/// by area averaging (exact at zero rotation, supersampled box average
/// otherwise); an upscaling [`SizeTarget::Exact`] axis is resampled
/// bilinearly. All averaging runs in premultiplied-alpha space, so
/// transparent out-of-source samples do not darken edges.
///
/// `stencil`'s shape is not applied, exactly as on [`crop_decoded_to_png`]:
/// a circle stencil yields its square bounding box, unmasked. `viewport`
/// carries the same must-match-the-component requirement documented on
/// [`crop_to_png`].
///
/// # Errors
///
/// Returns the same errors as [`crop_decoded_to_png`] for the same view and
/// framing conditions, with two differences:
/// [`CropError::InvalidOutputTarget`] if `output.size` is
/// [`SizeTarget::Exact`] with a zero width or height or
/// [`SizeTarget::MaxDimension`]`(0)`, or if `output.format` is
/// [`OutputFormat::Jpeg`] with a quality of zero or above 100; and
/// [`CropError::OutputTooLarge`] applies to the FINAL dimensions after
/// `output.size` (see [`output_size_with`]). [`CropError::Encode`] is
/// returned if PNG or JPEG encoding fails; [`OutputFormat::Rgba`] performs
/// no encoding.
pub fn crop_decoded(
    decoded: &DecodedSource,
    view: ViewTransform,
    stencil: Stencil,
    viewport: Size,
    output: CropOutput,
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
    let natural = decoded.natural_size();
    let (nat_w, nat_h) = native_output_size(natural, viewport, stencil, zoom)?;
    let (out_w, out_h) = apply_size_target(nat_w, nat_h, output.size)?;

    if let OutputFormat::Jpeg(opts) = output.format {
        if opts.quality == 0 || opts.quality > 100 {
            return Err(CropError::InvalidOutputTarget);
        }
    }

    let source = decoded.0.as_ref();
    // f64 for the same precision reasons documented in `crop_decoded_to_png`.
    let fit_scale = contain_scale(natural, viewport);
    let scale = f64::from(fit_scale) * f64::from(zoom);

    let pixels = if (out_w, out_h) == (nat_w, nat_h) {
        // Final dimensions equal the native crop: the exact sampling path
        // `crop_decoded_to_png` uses, byte-identical pixels.
        rasterize(source, out_w, out_h, scale, offset, rotation)
    } else {
        resample(
            source,
            &Resample {
                nat_w,
                nat_h,
                out_w,
                out_h,
                scale,
                offset,
                rotation,
            },
        )
    };

    let (format, bytes) = encode_pixels(pixels, out_w, out_h, output.format)?;
    Ok(CroppedOutput {
        width: out_w,
        height: out_h,
        format,
        bytes,
    })
}

/// Encodes the raw RGBA8 buffer `pixels` into `format`'s byte format and
/// reports what the returned bytes hold.
fn encode_pixels(
    pixels: Vec<u8>,
    out_w: u32,
    out_h: u32,
    format: OutputFormat,
) -> Result<(CroppedFormat, Vec<u8>), CropError> {
    match format {
        OutputFormat::Png => {
            // The same encoder call and settings as `crop_decoded_to_png`.
            let mut bytes = Vec::with_capacity(out_w as usize * out_h as usize);
            PngEncoder::new_with_quality(&mut bytes, CompressionType::Fast, FilterType::Adaptive)
                .write_image(&pixels, out_w, out_h, ExtendedColorType::Rgba8)
                .map_err(|e| CropError::Encode(Box::new(e)))?;
            Ok((CroppedFormat::Png, bytes))
        }
        OutputFormat::Jpeg(opts) => {
            // Composite onto the opaque background:
            // `out = bg * (255 - a) / 255 + c * a / 255`, rounded.
            let mut rgb = Vec::with_capacity(out_w as usize * out_h as usize * 3);
            for px in pixels.as_chunks::<4>().0 {
                let a = u32::from(px[3]);
                for (&c, &bg) in px[..3].iter().zip(&opts.background) {
                    let blended = u32::from(bg) * (255 - a) + u32::from(c) * a;
                    rgb.push((f64::from(blended) / 255.0).round() as u8);
                }
            }
            let mut bytes = Vec::new();
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, opts.quality)
                .write_image(&rgb, out_w, out_h, ExtendedColorType::Rgb8)
                .map_err(|e| CropError::Encode(Box::new(e)))?;
            Ok((CroppedFormat::Jpeg, bytes))
        }
        OutputFormat::Rgba => Ok((CroppedFormat::Rgba, pixels)),
    }
}

/// Samples every pixel of the `out_w`×`out_h` crop from `source` and
/// returns the raw RGBA bytes, row-major. The buffer starts zeroed — zero
/// RGBA is the fully transparent fill every out-of-source sample is
/// documented to produce — so both paths below only ever write in-source
/// pixels and leave the rest untouched.
///
/// Dispatches between two equivalent evaluations of the module doc's
/// inverse map: [`raster_translation`] when [`translation_offsets`] proves
/// the view is a pure integer translation, and [`raster_general`]
/// otherwise. Byte-for-byte equivalence of the two is exercised across a
/// parameter grid by this module's tests.
fn rasterize(
    source: &RgbaImage,
    out_w: u32,
    out_h: u32,
    scale: f64,
    offset: Point,
    rotation: f32,
) -> Vec<u8> {
    let mut pixels = vec![0u8; out_w as usize * out_h as usize * 4];
    if let Some((tx, ty)) = translation_offsets(source, out_w, out_h, scale, offset, rotation) {
        raster_translation(source, out_w, out_h, tx, ty, &mut pixels);
    } else {
        raster_general(source, out_w, out_h, scale, offset, rotation, &mut pixels);
    }
    pixels
}

/// Decides whether the crop is a pure integer translation of the source
/// and, if so, returns the per-axis translation `(tx, ty)` such that output
/// pixel `(ox, oy)` samples source pixel `(ox + tx, oy + ty)`.
///
/// At zero rotation the inverse map (module doc) collapses per axis to
/// `sx = ox + Cx` with `Cx = src_w / 2 - out_w / 2 - offset.x / scale`
/// (`Cy` analogously with heights and `offset.y`): the rotation terms drop
/// out and the `* scale` / `/ scale` pair cancels exactly, leaving a
/// constant. Rounding then commutes with the integer pixel index —
/// `round(ox + Cx) = ox + round(Cx)` — so one rounded constant serves every
/// pixel and rows can be block-copied.
///
/// That identity has one exception: a `Cx` whose fractional part is exactly
/// `±0.5`. Rust rounds halves away from zero, so e.g. `round(3 + (-2.5)) =
/// 1` while `3 + round(-2.5) = 0` — a whole-column shift. Half-integer
/// constants therefore bail to the general path, which rounds each pixel's
/// coordinate individually.
fn translation_offsets(
    source: &RgbaImage,
    out_w: u32,
    out_h: u32,
    scale: f64,
    offset: Point,
    rotation: f32,
) -> Option<(i64, i64)> {
    // Exact zero after normalisation only — any real rotation angle needs
    // the trigonometric path. `normalize_rotation` maps full turns (±360°,
    // 720°, …) to exactly 0.0, so those take the fast path too.
    if normalize_rotation(rotation) != 0.0 {
        return None;
    }
    let cx = f64::from(source.width()) / 2.0 - f64::from(out_w) / 2.0 - f64::from(offset.x) / scale;
    let cy =
        f64::from(source.height()) / 2.0 - f64::from(out_h) / 2.0 - f64::from(offset.y) / scale;
    // The half-integer tie bail-out described above. Exact comparison is
    // deliberate: only the one value where round's tie-breaking engages is
    // affected, and `fract()` of any finite f64 is exact.
    if cx.fract().abs() == 0.5 || cy.fract().abs() == 0.5 {
        return None;
    }
    Some((cx.round() as i64, cy.round() as i64))
}

/// The unrotated fast path: the crop is `source` shifted by the integer
/// vector `(tx, ty)` from [`translation_offsets`]. Computes the overlapping
/// output row/column ranges once, then block-copies one contiguous
/// 4-bytes-per-pixel source row segment per output row; rows and columns
/// outside the overlap keep the buffer's transparent zero fill.
fn raster_translation(
    source: &RgbaImage,
    out_w: u32,
    out_h: u32,
    tx: i64,
    ty: i64,
    pixels: &mut [u8],
) {
    let src_w = i64::from(source.width());
    let src_h = i64::from(source.height());

    // The overlap on each axis: the output indices where `0 <= o + t <
    // src`. Saturating arithmetic because `tx`/`ty` came through a
    // saturating float-to-int cast and may sit at `i64`'s limits, where
    // plain negation/subtraction would overflow; any such translation has
    // an empty overlap, which the clamp preserves.
    let ox_start = tx.saturating_neg().clamp(0, i64::from(out_w));
    let ox_end = src_w.saturating_sub(tx).clamp(0, i64::from(out_w));
    let oy_start = ty.saturating_neg().clamp(0, i64::from(out_h));
    let oy_end = src_h.saturating_sub(ty).clamp(0, i64::from(out_h));
    if ox_start >= ox_end || oy_start >= oy_end {
        // No overlap — the output stays fully transparent, the same result
        // the per-pixel bounds check produces sample by sample.
        return;
    }

    let src_raw: &[u8] = source.as_raw();
    let out_row_len = out_w as usize * 4;
    let src_row_len = source.width() as usize * 4;
    let seg_len = (ox_end - ox_start) as usize * 4;
    let src_x0 = (ox_start + tx) as usize * 4;

    for oy in oy_start..oy_end {
        // `oy + ty` is in `[0, src_h)` by the range construction above, so
        // both slices below are in bounds by construction.
        let src_at = (oy + ty) as usize * src_row_len + src_x0;
        let dst_at = oy as usize * out_row_len + ox_start as usize * 4;
        pixels[dst_at..dst_at + seg_len].copy_from_slice(&src_raw[src_at..src_at + seg_len]);
    }
}

/// The general inverse-sampling path, for any rotation: walks every output
/// pixel, stepping the source coordinate incrementally instead of
/// re-evaluating the full map. The map is affine in `(ox, oy)` — writing
/// `dx = (ox - out_w / 2) * scale - offset.x` (`dy` likewise) it reads
/// `sx = (dx * cos_inv - dy * sin_inv) / scale + src_w / 2` and
/// `sy = (dx * sin_inv + dy * cos_inv) / scale + src_h / 2` — so its
/// partial derivatives are constants: `cos_inv`/`sin_inv` per column and
/// `-sin_inv`/`cos_inv` per row. One start evaluation and two additions per
/// pixel replace the per-pixel multiplies and divides.
fn raster_general(
    source: &RgbaImage,
    out_w: u32,
    out_h: u32,
    scale: f64,
    offset: Point,
    rotation: f32,
    pixels: &mut [u8],
) {
    let (src_w, src_h) = (source.width(), source.height());
    let rotation_rad = f64::from(rotation).to_radians();
    // Sampling needs the INVERSE rotation (see module doc): `Rotate(-rotation)`.
    let (sin_inv, cos_inv) = (-rotation_rad).sin_cos();

    // The inverse map evaluated once, at output pixel (0, 0); everything
    // after is accumulator stepping.
    let dx0 = (0.0 - f64::from(out_w) / 2.0) * scale - f64::from(offset.x);
    let dy0 = (0.0 - f64::from(out_h) / 2.0) * scale - f64::from(offset.y);
    let mut row_sx = (dx0 * cos_inv - dy0 * sin_inv) / scale + f64::from(src_w) / 2.0;
    let mut row_sy = (dx0 * sin_inv + dy0 * cos_inv) / scale + f64::from(src_h) / 2.0;

    let src_raw: &[u8] = source.as_raw();
    let src_row_len = src_w as usize * 4;

    for row in pixels.chunks_exact_mut(out_w as usize * 4) {
        let (mut sx, mut sy) = (row_sx, row_sy);
        for px in row.as_chunks_mut::<4>().0 {
            // Nearest-neighbour bounds semantics, preserved exactly:
            // - `.round()` breaks ties away from zero;
            // - a coordinate rounding to `-0.0` (from e.g. `-0.3`) passes
            //   `>= 0.0` and samples index 0 — the negative side's first
            //   out-of-bounds result is `-1.0`, reached from `-0.5` out;
            // - the `as u32` casts saturate, so an upper-side excess lands
            //   at `u32::MAX` and fails the width/height check.
            // No per-pixel finiteness checks are needed: the caller
            // validated offset/zoom/rotation as finite and `scale` as
            // positive, and were an extreme-but-finite combination still to
            // overflow into infinity or NaN here, NaN fails `>= 0.0` and
            // infinity saturates out of bounds — both fall through to the
            // transparent fill, exactly like any other out-of-source
            // sample.
            let ix = sx.round();
            let iy = sy.round();
            if ix >= 0.0 && iy >= 0.0 {
                let (ix, iy) = (ix as u32, iy as u32);
                if ix < src_w && iy < src_h {
                    let at = iy as usize * src_row_len + ix as usize * 4;
                    px.copy_from_slice(&src_raw[at..at + 4]);
                }
            }
            sx += cos_inv;
            sy += sin_inv;
        }
        row_sx -= sin_inv;
        row_sy += cos_inv;
    }
}

/// The parameters of a resampled crop: the native crop's dimensions
/// (`nat_w`×`nat_h`, what [`rasterize`] would produce), the requested final
/// dimensions (`out_w`×`out_h`), and the native crop's affine mapping into
/// the source (`scale`, `offset`, `rotation` — the same values [`rasterize`]
/// takes).
struct Resample {
    /// The native crop width, in pixels.
    nat_w: u32,
    /// The native crop height, in pixels.
    nat_h: u32,
    /// The final output width, in pixels.
    out_w: u32,
    /// The final output height, in pixels.
    out_h: u32,
    /// The combined source-to-screen scale, `fit_scale * zoom`.
    scale: f64,
    /// The view's offset, in screen pixels.
    offset: Point,
    /// The view's rotation, in degrees.
    rotation: f32,
}

/// Samples the `r.out_w`×`r.out_h` output by resampling the native crop
/// (never materialised) and returns raw RGBA bytes, row-major. Only called
/// when the final dimensions differ from the native ones; equal dimensions
/// go through [`rasterize`].
///
/// Dispatch: a pure downscale of at most [`MAX_RESAMPLE_RATIO`] per axis
/// whose view [`translation_offsets`] proves to be an integer translation
/// (including its half-integer tie bail-out, at the native mapping) takes
/// [`resample_area`] — an exact area average, whose cost grows with the
/// native area and is bounded by the ratio limit. Every other case takes
/// [`resample_supersample`] — a box average over a subsample grid, with
/// bilinear per-subsample reads whenever an axis upscales (`kx`/`ky`
/// resolve to 1 on such an axis) and nearest-neighbour reads otherwise,
/// whose cost is bounded by the output area times the grid clamp.
fn resample(source: &RgbaImage, r: &Resample) -> Vec<u8> {
    let upscales = r.out_w > r.nat_w || r.out_h > r.nat_h;
    let rx = f64::from(r.nat_w) / f64::from(r.out_w);
    let ry = f64::from(r.nat_h) / f64::from(r.out_h);
    if !upscales && rx <= MAX_RESAMPLE_RATIO && ry <= MAX_RESAMPLE_RATIO {
        if let Some((tx, ty)) =
            translation_offsets(source, r.nat_w, r.nat_h, r.scale, r.offset, r.rotation)
        {
            return resample_area(source, r, tx, ty);
        }
    }
    resample_supersample(source, r, upscales)
}

/// The largest per-axis native-to-output ratio [`resample_area`] handles and
/// the per-axis subsample-grid clamp of [`resample_supersample`]. Caps the
/// per-output-pixel work of both resampling paths.
const MAX_RESAMPLE_RATIO: f64 = 16.0;

/// Exact area average for an unrotated, integer-translated downscale.
/// Output pixel `(ox, oy)` covers the native-crop rectangle
/// `[ox*rx, (ox+1)*rx) x [oy*ry, (oy+1)*ry)` with `rx = nat_w / out_w` and
/// `ry = nat_h / out_h` (f64); native-crop pixel `(nx, ny)` is source pixel
/// `(nx + tx, ny + ty)`, or transparent when that lies outside the source.
/// Edge rows/columns are weighted by fractional coverage. Averaging runs in
/// premultiplied-alpha space: out-of-source cells contribute nothing to the
/// accumulators but their weight stays in the total, so they dilute alpha
/// without darkening colour.
fn resample_area(source: &RgbaImage, r: &Resample, tx: i64, ty: i64) -> Vec<u8> {
    let rx = f64::from(r.nat_w) / f64::from(r.out_w);
    let ry = f64::from(r.nat_h) / f64::from(r.out_h);
    let src_w = i64::from(source.width());
    let src_h = i64::from(source.height());
    let src_raw: &[u8] = source.as_raw();
    let src_row_len = source.width() as usize * 4;

    let mut pixels = vec![0u8; r.out_w as usize * r.out_h as usize * 4];
    for oy in 0..r.out_h {
        let y0 = f64::from(oy) * ry;
        let y1 = f64::from(oy + 1) * ry;
        for ox in 0..r.out_w {
            let x0 = f64::from(ox) * rx;
            let x1 = f64::from(ox + 1) * rx;
            let mut acc = [0.0f64; 4];
            for ny in (y0.floor() as i64)..(y1.ceil() as i64) {
                let wy = y1.min((ny + 1) as f64) - y0.max(ny as f64);
                if wy <= 0.0 {
                    continue;
                }
                // Saturating: `ty` came through a saturating float-to-int
                // cast and may sit at `i64`'s limits; any such translation
                // is out of bounds, which the check below preserves.
                let sy = ny.saturating_add(ty);
                for nx in (x0.floor() as i64)..(x1.ceil() as i64) {
                    let wx = x1.min((nx + 1) as f64) - x0.max(nx as f64);
                    if wx <= 0.0 {
                        continue;
                    }
                    let sx = nx.saturating_add(tx);
                    if sx >= 0 && sy >= 0 && sx < src_w && sy < src_h {
                        let at = sy as usize * src_row_len + sx as usize * 4;
                        accumulate(&mut acc, &src_raw[at..at + 4], wx * wy);
                    }
                }
            }
            let at = (oy as usize * r.out_w as usize + ox as usize) * 4;
            resolve(&acc, (x1 - x0) * (y1 - y0), &mut pixels[at..at + 4]);
        }
    }
    pixels
}

/// Supersampled box average through the affine inverse, for rotated
/// downscales, translation ties, upscales, and mixed-axis targets. Each
/// output pixel averages a `kx`×`ky` grid of subsample points — the centres
/// of equal subcells of the pixel's native-crop footprint — with
/// `kx = ceil(rx)` and `ky = ceil(ry)`, each clamped to `1..=16` (an
/// upscaling or equal axis resolves to 1). Each subsample point, shifted by
/// `-0.5` into the pixel-centres-at-integers convention [`raster_general`]
/// samples in, is mapped through the same f64 affine inverse and read
/// nearest-neighbour, or bilinearly when `bilinear` is set. Averaging runs
/// in premultiplied-alpha space; out-of-source subsamples stay transparent.
fn resample_supersample(source: &RgbaImage, r: &Resample, bilinear: bool) -> Vec<u8> {
    let rx = f64::from(r.nat_w) / f64::from(r.out_w);
    let ry = f64::from(r.nat_h) / f64::from(r.out_h);
    let kx = (rx.ceil() as u32).clamp(1, MAX_RESAMPLE_RATIO as u32);
    let ky = (ry.ceil() as u32).clamp(1, MAX_RESAMPLE_RATIO as u32);
    let total_weight = f64::from(kx * ky);

    let rotation_rad = f64::from(r.rotation).to_radians();
    // The INVERSE rotation, exactly as `raster_general` samples with.
    let (sin_inv, cos_inv) = (-rotation_rad).sin_cos();
    let half_nat_w = f64::from(r.nat_w) / 2.0;
    let half_nat_h = f64::from(r.nat_h) / 2.0;
    let half_src_w = f64::from(source.width()) / 2.0;
    let half_src_h = f64::from(source.height()) / 2.0;

    let mut pixels = vec![0u8; r.out_w as usize * r.out_h as usize * 4];
    for oy in 0..r.out_h {
        for ox in 0..r.out_w {
            let mut acc = [0.0f64; 4];
            for j in 0..ky {
                let y = (f64::from(oy) + (f64::from(j) + 0.5) / f64::from(ky)) * ry - 0.5;
                let dy = (y - half_nat_h) * r.scale - f64::from(r.offset.y);
                for i in 0..kx {
                    let x = (f64::from(ox) + (f64::from(i) + 0.5) / f64::from(kx)) * rx - 0.5;
                    let dx = (x - half_nat_w) * r.scale - f64::from(r.offset.x);
                    let sx = (dx * cos_inv - dy * sin_inv) / r.scale + half_src_w;
                    let sy = (dx * sin_inv + dy * cos_inv) / r.scale + half_src_h;
                    if bilinear {
                        bilinear_sample(source, sx, sy, &mut acc);
                    } else {
                        nearest_sample(source, sx, sy, &mut acc);
                    }
                }
            }
            let at = (oy as usize * r.out_w as usize + ox as usize) * 4;
            resolve(&acc, total_weight, &mut pixels[at..at + 4]);
        }
    }
    pixels
}

/// Adds source pixel `(sx, sy)`, rounded nearest-neighbour with the same
/// bounds semantics as [`raster_general`], to `acc` with weight 1. An
/// out-of-source coordinate adds nothing — a transparent sample.
fn nearest_sample(source: &RgbaImage, sx: f64, sy: f64, acc: &mut [f64; 4]) {
    let ix = sx.round();
    let iy = sy.round();
    if ix >= 0.0 && iy >= 0.0 {
        let (ix, iy) = (ix as u32, iy as u32);
        if ix < source.width() && iy < source.height() {
            let at = (iy as usize * source.width() as usize + ix as usize) * 4;
            accumulate(acc, &source.as_raw()[at..at + 4], 1.0);
        }
    }
}

/// Adds the bilinear blend of the 4 source pixels around fractional
/// coordinate `(sx, sy)` to `acc`, with weights summing to 1. Neighbours
/// outside the source are transparent: they keep their weight but add
/// nothing to the accumulators.
fn bilinear_sample(source: &RgbaImage, sx: f64, sy: f64, acc: &mut [f64; 4]) {
    let x0 = sx.floor();
    let y0 = sy.floor();
    let fx = sx - x0;
    let fy = sy - y0;
    let src_w = f64::from(source.width());
    let src_h = f64::from(source.height());
    for (px, py, w) in [
        (x0, y0, (1.0 - fx) * (1.0 - fy)),
        (x0 + 1.0, y0, fx * (1.0 - fy)),
        (x0, y0 + 1.0, (1.0 - fx) * fy),
        (x0 + 1.0, y0 + 1.0, fx * fy),
    ] {
        if w > 0.0 && px >= 0.0 && py >= 0.0 && px < src_w && py < src_h {
            let at = (py as usize * source.width() as usize + px as usize) * 4;
            accumulate(acc, &source.as_raw()[at..at + 4], w);
        }
    }
}

/// Adds one RGBA source pixel to the premultiplied accumulators with weight
/// `w`: `w * channel * alpha` into the three colour slots, `w * alpha` into
/// the alpha slot.
fn accumulate(acc: &mut [f64; 4], px: &[u8], w: f64) {
    let a = f64::from(px[3]);
    acc[0] += w * f64::from(px[0]) * a;
    acc[1] += w * f64::from(px[1]) * a;
    acc[2] += w * f64::from(px[2]) * a;
    acc[3] += w * a;
}

/// Resolves premultiplied accumulators into one straight-alpha RGBA output
/// pixel: alpha is the alpha accumulator over `total_weight`; each colour is
/// its accumulator over the alpha accumulator when that is non-zero. A fully
/// transparent result leaves `out`'s zeroed colour bytes untouched.
fn resolve(acc: &[f64; 4], total_weight: f64, out: &mut [u8]) {
    out[3] = (acc[3] / total_weight).round() as u8;
    if acc[3] > 0.0 {
        out[0] = (acc[0] / acc[3]).round() as u8;
        out[1] = (acc[1] / acc[3]).round() as u8;
        out[2] = (acc[2] / acc[3]).round() as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    /// A deterministic multi-channel gradient: every pixel differs from its
    /// neighbours in at least one channel, so a one-pixel sampling shift
    /// changes bytes somewhere.
    fn gradient_source(width: u32, height: u32) -> RgbaImage {
        RgbaImage::from_fn(width, height, |x, y| {
            Rgba([
                (x * 7 % 251) as u8,
                (y * 13 % 241) as u8,
                ((x + y) * 3 % 253) as u8,
                255,
            ])
        })
    }

    /// Direct, non-incremental f64 evaluation of the module doc's inverse
    /// map at every output pixel — the reference both production paths must
    /// reproduce byte-for-byte.
    fn reference_raster(
        source: &RgbaImage,
        out_w: u32,
        out_h: u32,
        scale: f64,
        offset: Point,
        rotation: f32,
    ) -> Vec<u8> {
        let rotation_rad = f64::from(rotation).to_radians();
        let (sin_inv, cos_inv) = (-rotation_rad).sin_cos();
        let mut pixels = vec![0u8; out_w as usize * out_h as usize * 4];
        for oy in 0..out_h {
            for ox in 0..out_w {
                let dx = (f64::from(ox) - f64::from(out_w) / 2.0) * scale - f64::from(offset.x);
                let dy = (f64::from(oy) - f64::from(out_h) / 2.0) * scale - f64::from(offset.y);
                let sx = (dx * cos_inv - dy * sin_inv) / scale + f64::from(source.width()) / 2.0;
                let sy = (dx * sin_inv + dy * cos_inv) / scale + f64::from(source.height()) / 2.0;
                let (ix, iy) = (sx.round(), sy.round());
                if ix >= 0.0 && iy >= 0.0 {
                    let (ix, iy) = (ix as u32, iy as u32);
                    if ix < source.width() && iy < source.height() {
                        let at = (oy as usize * out_w as usize + ox as usize) * 4;
                        pixels[at..at + 4].copy_from_slice(&source.get_pixel(ix, iy).0);
                    }
                }
            }
        }
        pixels
    }

    /// Fast path vs general path, byte-equal, across a deterministic
    /// parameter matrix at rotation 0: even/odd source dimensions, even/odd
    /// output dimensions, integer and fractional offsets, zooms below and
    /// above 1, and several viewport/stencil framings.
    #[test]
    fn translation_path_matches_general_path_at_rotation_zero() {
        let sources = [(16u32, 16u32), (17, 13)];
        let framings = [
            (Size::new(32.0, 32.0), Stencil::rectangle(16.0, 16.0)),
            (Size::new(24.0, 18.0), Stencil::rectangle(15.0, 9.0)),
            (Size::new(20.0, 20.0), Stencil::square(13.0)),
        ];
        let offsets = [
            Point::ZERO,
            Point::new(3.0, -2.0),
            Point::new(0.25, 0.75),
            Point::new(-5.5, 4.25),
        ];
        let zooms = [0.5f32, 1.0, 1.7, 2.0];

        let mut compared = 0usize;
        for (src_w, src_h) in sources {
            let source = gradient_source(src_w, src_h);
            let natural = Size::new(src_w as f32, src_h as f32);
            for (viewport, stencil) in framings {
                for offset in offsets {
                    for zoom in zooms {
                        let (out_w, out_h) = output_size(natural, viewport, stencil, zoom)
                            .expect("grid parameters are valid");
                        let scale = f64::from(contain_scale(natural, viewport)) * f64::from(zoom);
                        // Half-integer ties are the general path's job by
                        // design — the matrix compares eligible cases.
                        let Some((tx, ty)) =
                            translation_offsets(&source, out_w, out_h, scale, offset, 0.0)
                        else {
                            continue;
                        };
                        let len = out_w as usize * out_h as usize * 4;
                        let mut fast = vec![0u8; len];
                        raster_translation(&source, out_w, out_h, tx, ty, &mut fast);
                        let mut general = vec![0u8; len];
                        raster_general(&source, out_w, out_h, scale, offset, 0.0, &mut general);
                        assert_eq!(
                            fast,
                            general,
                            "paths diverged: source {src_w}x{src_h}, viewport {}x{}, stencil \
                             {}x{}, offset ({}, {}), zoom {zoom}",
                            viewport.width,
                            viewport.height,
                            stencil.width(),
                            stencil.height(),
                            offset.x,
                            offset.y,
                        );
                        compared += 1;
                    }
                }
            }
        }
        assert!(
            compared >= 50,
            "only {compared} eligible cases — widen the grid"
        );
    }

    /// Constructs an exact half-integer translation constant: `fit_scale =
    /// 1`, `zoom = 1`, a 10-wide source and a 15-wide output give `Cx = 5 -
    /// 7.5 - 0 = -2.5`. `round(-2.5)` is `-3` (ties away from zero), so a
    /// blanket translation would sample `ox - 3` everywhere while the
    /// per-pixel formula at e.g. `ox = 3` samples `round(0.5) = 1` — a
    /// whole-column divergence. The tie must route to the general path, and
    /// the result must still match the reference per-pixel formula.
    #[test]
    fn half_integer_translation_tie_routes_to_the_general_path() {
        let source = gradient_source(10, 10);
        let natural = Size::new(10.0, 10.0);
        let viewport = Size::new(10.0, 10.0);
        let stencil = Stencil::rectangle(15.0, 15.0);
        let (out_w, out_h) = output_size(natural, viewport, stencil, 1.0).expect("valid inputs");
        assert_eq!((out_w, out_h), (15, 15));
        let scale = f64::from(contain_scale(natural, viewport));
        assert_eq!(scale, 1.0);

        assert!(
            translation_offsets(&source, out_w, out_h, scale, Point::ZERO, 0.0).is_none(),
            "a half-integer constant must not be treated as a translation"
        );
        let produced = rasterize(&source, out_w, out_h, scale, Point::ZERO, 0.0);
        let reference = reference_raster(&source, out_w, out_h, scale, Point::ZERO, 0.0);
        assert_eq!(produced, reference);
    }

    /// The general path's incremental (accumulator) stepping against a
    /// direct per-pixel evaluation of the same f64 map, byte-equal — guards
    /// the start values and the step constants of the accumulation.
    #[test]
    fn general_path_stepping_matches_direct_evaluation_at_rotation_zero() {
        let cases = [
            (
                40u32,
                30u32,
                Size::new(20.0, 20.0),
                Stencil::rectangle(16.0, 10.0),
                Point::new(0.3, -1.7),
                1.0f32,
            ),
            (
                64,
                64,
                Size::new(32.0, 32.0),
                Stencil::square(19.0),
                Point::new(2.25, 3.5),
                1.3,
            ),
            // A low zoom blows the output up to several hundred pixels per
            // side: enough columns that even a tiny per-step error in the
            // accumulation drifts across a rounding boundary somewhere.
            (
                200,
                160,
                Size::new(100.0, 100.0),
                Stencil::rectangle(90.0, 70.0),
                Point::new(-3.3, 1.75),
                0.25,
            ),
        ];
        for (src_w, src_h, viewport, stencil, offset, zoom) in cases {
            let source = gradient_source(src_w, src_h);
            let natural = Size::new(src_w as f32, src_h as f32);
            let (out_w, out_h) =
                output_size(natural, viewport, stencil, zoom).expect("valid inputs");
            let scale = f64::from(contain_scale(natural, viewport)) * f64::from(zoom);
            let mut general = vec![0u8; out_w as usize * out_h as usize * 4];
            raster_general(&source, out_w, out_h, scale, offset, 0.0, &mut general);
            let reference = reference_raster(&source, out_w, out_h, scale, offset, 0.0);
            assert_eq!(general, reference, "case {src_w}x{src_h}, zoom {zoom}");
        }
    }

    /// A source whose header claims a dimension beyond the decode cap is
    /// refused before any pixel work, as `CropError::Decode`.
    #[test]
    fn decode_refuses_oversized_dimensions() {
        let wide = RgbaImage::new(MAX_SOURCE_DIMENSION + 1, 1);
        let mut bytes = Vec::new();
        wide.write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .expect("encode oversized fixture");
        assert!(matches!(
            DecodedSource::decode(&bytes),
            Err(CropError::Decode(_))
        ));
    }
}
