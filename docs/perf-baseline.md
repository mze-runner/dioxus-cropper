# Crop pipeline performance

Measurements from the `benches/crop.rs` criterion suite (`cargo bench`),
taken on an Apple M3 (8 cores, 16 GB), macOS. Fixtures are the suite's own
deterministic synthetic images (gradient plus mild noise — see the bench
module doc), so runs are comparable across machines and commits, if not
identical.

All crop benchmarks use the demo's default framing — 480×480 viewport,
centred 240 px square stencil, zoom 1.0, no pan — so the output side is
`240 / fit_scale` source pixels: 512, 1732 and 3840 for the three sources.

## 0.0.2 → 0.0.3

Baseline is the 0.0.2 pipeline (`load_from_memory` + `to_rgba8`, per-pixel
closure sampling in f32, default PNG encoder settings). The 0.0.3 column is
the reworked pipeline: decode through `ImageReader` with explicit limits and
`into_rgba8`, a row-copy fast path for unrotated crops, f64 incremental
stepping for rotated ones, and fast-compression/no-filter PNG encoding into
a pre-sized buffer.

| Benchmark | 0.0.2 | 0.0.3 | Change |
|---|---:|---:|---:|
| decode/3464×2310 (8 MP) | 59.7 ms | 58.9 ms | −1% |
| decode/7680×4320 (33 MP) | 282.0 ms | 244.1 ms | −13% |
| crop/1024×1024, 0° | 2.30 ms | 0.95 ms | −59% |
| crop/1024×1024, 90° | 2.02 ms | 1.17 ms | −42% |
| crop/1024×1024, 37° | 2.09 ms | 1.13 ms | −46% |
| crop/3464×2310, 0° | 25.1 ms | 16.2 ms | −36% |
| crop/3464×2310, 90° | 27.1 ms | 19.4 ms | −29% |
| crop/3464×2310, 37° | 25.6 ms | 18.9 ms | −26% |
| crop/7680×4320, 0° | 131.4 ms | 76.3 ms | −42% |
| crop/7680×4320, 90° | 168.7 ms | 134.4 ms | −20% |
| crop/7680×4320, 37° | 132.0 ms | 93.5 ms | −29% |
| encode_png/512×512 | 1.19 ms | 0.94 ms | −21% |
| encode_png/1732×1732 | 15.6 ms | 15.0 ms | −4% |
| encode_png/3840×3840 | 82.7 ms | 79.5 ms | −4% |

Notes on reading the table:

- The `crop` rows measure sampling *plus* PNG encode; `crop` minus
  `encode_png` at the matching output size approximates the sampling loop
  alone. At 0° the sampling stage is now a block row copy and all but
  vanishes — the remaining crop cost is almost entirely PNG encode. The
  subtraction is content-dependent (the full-frame synthetic fixture and an
  actual crop's pixels deflate differently), which is why crop/7680×4320 at
  0° can come in *under* encode_png/3840×3840 — treat the split as
  indicative, not exact.
- The `encode_png` gain from fast-compression/no-filter settings is modest
  because the PNG stack's default deflate is already speed-oriented; the
  settings still buy a measurable margin and the pre-sized output buffer
  removes reallocation churn on multi-megabyte results.
- The `decode` gain comes from `into_rgba8` dropping a full-buffer copy;
  decode remains the dominant fixed cost for large sources, which is why
  `DecodedSource` caching (decode once, crop many times) matters more than
  any constant-factor win here.
- Encoded output is larger than under the 0.0.2 defaults (speed-oriented
  deflate, no pre-filtering). The pixels are identical; a caller archiving
  crops can re-encode with a heavier compressor without loss.

These numbers are native (`aarch64-apple-darwin`). On `wasm32` in a browser
the same relative shape holds but absolute times are larger; the demo shows
per-stage wall times (read, probe, decode, crop, url) live under the stage
for exactly this reason.
