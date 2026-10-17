# dioxus-cropper

[![CI](https://github.com/mze-runner/dioxus-cropper/actions/workflows/ci.yml/badge.svg)](https://github.com/mze-runner/dioxus-cropper/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/dioxus-cropper.svg)](https://crates.io/crates/dioxus-cropper)
[![docs.rs](https://img.shields.io/docsrs/dioxus-cropper)](https://docs.rs/dioxus-cropper)

A headless image cropper component for Dioxus.

**[Live demo](https://mze-runner.github.io/dioxus-cropper/)** — pan, zoom, rotate and crop an image of your own, with every configurable prop on a control rail.

## Headless

`Cropper` renders a positioned, clipped image and a stencil overlay; it owns no view state and ships no CSS, no icons, and no strings. The host owns pan/zoom/rotation state, supplies every control (zoom slider, rotate button, confirm button), and styles the result with its own classes.

## Installation

```toml
[dependencies]
dioxus-cropper = "0.0.4"
```

Select a Dioxus renderer feature (`web`, `desktop`, etc.) in the consuming crate.

## Usage

`Cropper` is controlled: the host holds a `ViewTransform` (offset, zoom, rotation), passes it in as `view`, and folds the `on_pan`/`on_zoom` deltas the component reports back into that state. `clamp_offset` keeps the pan within the stencil-coverage bound for the current zoom.

```rust
use dioxus::prelude::*;
use dioxus_cropper::geometry::{clamp_offset, contain_scale, Point, Size, Stencil, ViewTransform};
use dioxus_cropper::Cropper;

fn app() -> Element {
    let natural_size = Size::new(1920.0, 1080.0);
    let viewport = Size::new(480.0, 480.0);
    let stencil = Stencil::square(240.0);
    let fit_scale = contain_scale(natural_size, viewport);

    let mut view = use_signal(ViewTransform::default);

    rsx! {
        Cropper {
            src: "/assets/photo.jpg",
            natural_size,
            view: view(),
            stencil,
            viewport,
            on_pan: move |delta: Point| {
                let mut v = view.write();
                let panned = Point::new(v.offset.x + delta.x, v.offset.y + delta.y);
                v.offset = clamp_offset(panned, natural_size, fit_scale, v.zoom, v.rotation, stencil);
            },
            on_zoom: move |delta: f32| {
                let mut v = view.write();
                v.zoom = (v.zoom + delta * 0.001).max(1.0);
            },
        }
    }
}
```

`natural_size` must be the source image's pixel dimensions after EXIF orientation, as the webview renders it — `natural_size(source_bytes)` reads them from the header without decoding pixels. `view`, `stencil` and `viewport` describe the current state; the component does not persist or mutate any of them.

For a user-picked file on the web target, pass an object URL (`URL.createObjectURL` over a `Blob` of the file bytes) as `src` rather than a `data:` URI — an object URL is a constant-size handle, where a `data:` URI embeds the whole base64-encoded image in the DOM attribute. Revoke a replaced object URL with `URL.revokeObjectURL`.

On desktop, blob URLs cannot be created from Rust; serve the bytes from memory with `dioxus::desktop::use_asset_handler` and pass that path as `src`. The demo's `object_url` module does both.

## Producing a crop

Cropping is a separate call, not something `Cropper` does internally. `crop_async` is the call for both web and desktop hosts; it never blocks the UI thread:

```rust
use dioxus_cropper::{crop_async, CropOutput, DecodeOptions};

let cropped = crop_async(source_bytes, view, stencil, viewport, CropOutput::default(), DecodeOptions::default()).await?;
// cropped.bytes, cropped.width, cropped.height, cropped.format
```

On wasm32 the browser decodes, resamples and encodes (`createImageBitmap`, a 2D canvas, `toBlob`); no decoded copy of the source is held in WASM memory. On native targets the decode and `crop_decoded` run on a background thread; the future needs no particular async runtime. EXIF orientation is applied on both unless `DecodeOptions::default().with_orientation(false)` turns it off, in which case render the stage with CSS `image-orientation: none` and pass `natural_size_with` for the same options.

On wasm32, `crop_blob_in_browser` takes a `Blob` (for example a picked `File`) in place of bytes.

The synchronous building blocks below remain available. `DecodedSource::decode` applies EXIF orientation; `DecodedSource::decode_with` takes `DecodeOptions`. `crop_to_png` decodes the source bytes and samples the same region `Cropper` is displaying, returning PNG bytes plus the output's pixel dimensions:

```rust
use dioxus_cropper::crop_to_png;

let cropped = crop_to_png(source_bytes, view, stencil, viewport)?;
// cropped.png_bytes, cropped.width, cropped.height
```

`viewport` passed here must be the exact same value passed to `Cropper` for that `view`.

For a picked file the user may crop more than once, decode once with `DecodedSource::decode` and call `crop_decoded_to_png` per press instead — `crop_to_png` decodes fresh on every call, and decode dominates the pipeline's cost.

To preview the crop's output dimensions without decoding or sampling any pixels, call `output_size` — it computes exactly what `crop_decoded_to_png` returns for the same `natural`, `viewport`, `stencil` and `zoom`:

```rust
use dioxus_cropper::output_size;

let (width, height) = output_size(natural_size, viewport, stencil, view.zoom)?;
```

## Target output

`crop_decoded` extends `crop_decoded_to_png` with an output target: a size (`Native` — the default, today's "as is" behaviour; `Exact` — fixed dimensions regardless of the crop's native size; `MaxDimension` — a cap on the longer side, never upscaling) and a format (`Png`, `Jpeg`, or raw `Rgba` bytes). Downscaling area-averages; upscaling is bilinear. JPEG has no alpha channel, so transparent pixels are composited onto `JpegOptions::background`:

```rust
use dioxus_cropper::{crop_decoded, CropOutput, JpegOptions, OutputFormat, SizeTarget};

let output = CropOutput {
    size: SizeTarget::Exact { width: 512, height: 512 },
    format: OutputFormat::Jpeg(JpegOptions::default()),
};
let cropped = crop_decoded(&decoded, view, stencil, viewport, output)?;
// cropped.bytes, cropped.width, cropped.height, cropped.format
```

`output_size_with` predicts the final dimensions for a `SizeTarget` the same way `output_size` does for the native size.

## Configurable

| Prop | Type | Purpose |
|---|---|---|
| `src` | `Arc<str>` (`#[props(into)]` — accepts `&str`, `String`, or `Arc<str>`) | Image source — any URL the webview can render: an object URL on the web, an asset-handler path on desktop. |
| `natural_size` | `Size` | The source's pixel dimensions after EXIF orientation (`natural_size`). |
| `view` | `ViewTransform` | Caller-owned offset, zoom, rotation. |
| `stencil` | `Stencil` | The crop window — `Stencil::rectangle`, `Stencil::square`, or `Stencil::circle`. |
| `viewport` | `Size` | Fixed on-screen footprint; defaults to `480.0 x 480.0`. |
| `dim_alpha` | `f32` | Opacity of the mask outside the stencil, `0.0`–`1.0`; defaults to `0.5`. |
| `cursor` | `CropperCursor` | `Move` (default), `Grab`, `AllScroll`, `Crosshair`, `None`, or `Custom(String)`. |
| `pan_direction` | `PanDirection` | `Image` (default) or `Frame` — whether a pan delta moves the image or the framing. |
| `classes` | `CropperClasses` | Class hooks for the viewport, image, and stencil elements. All empty by default. |
| `on_pan` | `EventHandler<Point>` | Raw pan delta, adjusted for `pan_direction`. |
| `on_zoom` | `EventHandler<f32>` | Raw wheel delta, sign-flipped so positive means zoom in. |

## Production hosts: the synchronous calls block the calling thread

`DecodedSource::decode` and `crop_decoded_to_png` are synchronous CPU work — resampling and PNG encoding included — and both block whichever thread calls them for as long as they run. On the web target that thread is the browser's main thread; on desktop it is the thread running the app's event loop. Use `crop_async` instead, which keeps both responsive. `output_size` lets a host predict the crop's output pixel dimensions from `natural`, `viewport`, `stencil` and `zoom` alone, without decoding or sampling any pixels, so it can be checked up front. `MAX_OUTPUT_PIXELS` is the ceiling `output_size` and `crop_decoded_to_png` enforce on that predicted output.

## Licence

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
