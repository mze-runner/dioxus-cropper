# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

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
