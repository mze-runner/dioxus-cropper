# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.0.3]

### Added

- `crop_decoded`, cropping to a caller-chosen output target: `CropOutput`
  pairs a `SizeTarget` (`Native`, `Exact`, `MaxDimension`) with an
  `OutputFormat` (`Png`, `Jpeg(JpegOptions)`, raw `Rgba`), returning a
  `CroppedOutput` with the bytes and their `CroppedFormat`. Downscales are
  area-averaged, upscales bilinear; JPEG output composites transparency
  onto `JpegOptions::background`.
- `output_size_with`, predicting the final output dimensions for a
  `SizeTarget` without decoding or sampling.
- `CropError::InvalidOutputTarget`, returned for a zero `Exact` dimension,
  a zero `MaxDimension`, or a JPEG quality outside `1..=100`.
- `docs/known-limitations.md`, documenting that EXIF orientation is not
  applied.
- Criterion benchmark suite under `benches/`, covering decode, the full crop
  pass and PNG encode against deterministic synthetic fixtures.

### Changed

- Crop sampling computes coordinates in `f64`. At exact half-integer tie
  framings (for example an odd source dimension with a centred, unrotated
  view) the output can shift by one row or column relative to 0.0.2; the
  new results follow the documented screen-to-source transform exactly,
  where the previous `f32` arithmetic broke such ties inconsistently.
- Unrotated crops copy source rows directly; rotated crops advance the
  inverse transform incrementally instead of re-deriving it per pixel. The
  sampling stage is substantially faster; end-to-end crop time improves
  most where PNG encoding does not dominate — measurements in
  `docs/perf-baseline.md`.
- PNG encoding pins its settings explicitly (fast compression with
  adaptive filtering — the encoder's previous effective defaults) and
  pre-sizes the output buffer. Output bytes are unchanged in practice; a
  caller archiving crops can still re-encode with a heavier compressor
  without loss.
- `DecodedSource::decode` enforces decode limits: a source over 16384
  pixels per side, or whose decode would exceed the allocation cap, returns
  `CropError::Decode` instead of exhausting memory.

## [0.0.2]

### Breaking

- `CropError::Decode` and `CropError::Encode` carry `Box<dyn std::error::Error + Send + Sync>`; `image` does not appear in the public API.
- `Cropper`'s `src` prop is `Arc<str>` with `#[props(into)]`.
- `CropError` is `#[non_exhaustive]`.
- `CropError` has an `EmptyNatural` variant, returned by `output_size` when `natural`'s width or height is not positive and finite.
- `CropError` has an `OutputTooLarge` variant, returned by `output_size` when the computed output area would exceed `MAX_OUTPUT_PIXELS`.

### Added

- `output_size`, returning the cropped output's pixel dimensions for a given natural size, viewport, stencil and zoom.
- Root re-exports of `normalize_rotation` and `rotated_bounding_box`, alongside the `geometry` re-exports.
- `MAX_OUTPUT_PIXELS`, the largest output area in pixels `output_size` will return.

### Fixed

- `output_size` validates `natural` and returns `CropError::EmptyNatural` if its width or height is not positive and finite.
- `output_size` rejects a computed output area beyond `MAX_OUTPUT_PIXELS`, returning `CropError::OutputTooLarge`.

### Changed

- `DecodedSource` is `Arc`-backed; cloning it is a refcount bump.
- Declared MSRV of 1.88.0.
- Dual-licensed `MIT OR Apache-2.0`.

## [0.0.1]

Initial release: headless image cropper component for Dioxus, with pan/zoom/rotation geometry and PNG crop output.
