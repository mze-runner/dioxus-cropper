# dioxus-cropper

[![CI](https://github.com/mze-runner/dioxus-cropper/actions/workflows/ci.yml/badge.svg)](https://github.com/mze-runner/dioxus-cropper/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/dioxus-cropper.svg)](https://crates.io/crates/dioxus-cropper)
[![docs.rs](https://img.shields.io/docsrs/dioxus-cropper)](https://docs.rs/dioxus-cropper)

A headless image cropper component for Dioxus.

## Headless

`Cropper` renders a positioned, clipped image and a stencil overlay; it owns no view state and ships no CSS, no icons, and no strings. The host owns pan/zoom/rotation state, supplies every control (zoom slider, rotate button, confirm button), and styles the result with its own classes.

## Installation

```toml
[dependencies]
dioxus-cropper = "0.0.2"
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
            src: "data:image/png;base64,",
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

`natural_size` must be the source image's real decoded pixel dimensions (`DecodedSource::natural_size`). `view`, `stencil` and `viewport` describe the current state; the component does not persist or mutate any of them.

## Producing a crop

Cropping is a separate call, not something `Cropper` does internally. `crop_to_png` decodes the source bytes and samples the same region `Cropper` is displaying, returning PNG bytes plus the output's pixel dimensions:

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

## Configurable

| Prop | Type | Purpose |
|---|---|---|
| `src` | `Arc<str>` (`#[props(into)]` — accepts `&str`, `String`, or `Arc<str>`) | Image source — URL or data URI. |
| `natural_size` | `Size` | The decoded image's real pixel dimensions. |
| `view` | `ViewTransform` | Caller-owned offset, zoom, rotation. |
| `stencil` | `Stencil` | The crop window — `Stencil::rectangle`, `Stencil::square`, or `Stencil::circle`. |
| `viewport` | `Size` | Fixed on-screen footprint; defaults to `480.0 x 480.0`. |
| `dim_alpha` | `f32` | Opacity of the mask outside the stencil, `0.0`–`1.0`; defaults to `0.5`. |
| `cursor` | `CropperCursor` | `Move` (default), `Grab`, `AllScroll`, `Crosshair`, `None`, or `Custom(String)`. |
| `pan_direction` | `PanDirection` | `Image` (default) or `Frame` — whether a pan delta moves the image or the framing. |
| `classes` | `CropperClasses` | Class hooks for the viewport, image, and stencil elements. All empty by default. |
| `on_pan` | `EventHandler<Point>` | Raw pan delta, adjusted for `pan_direction`. |
| `on_zoom` | `EventHandler<f32>` | Raw wheel delta, sign-flipped so positive means zoom in. |

## Production hosts: decode and crop block the calling thread

`DecodedSource::decode` and `crop_decoded_to_png` are synchronous CPU work — resampling and PNG encoding included — and both block whichever thread calls them for as long as they run. On the web target that thread is the browser's main thread, so a call in progress freezes the page: no repaint, no input handling, until it returns.

A production web host should run both calls in a Web Worker and message the result back to the main thread, keeping the page responsive while a large image decodes or crops. `output_size` lets a host predict the crop's output pixel dimensions from `natural`, `viewport`, `stencil` and `zoom` alone, without decoding or sampling any pixels, so it can be checked up front. `MAX_OUTPUT_PIXELS` is the ceiling `output_size` and `crop_decoded_to_png` enforce on that predicted output.

## Licence

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
