# Crop pipeline performance

Measurements from the `benches/crop.rs` criterion suite on an Apple M3
(8 cores, 16 GB), macOS, native `aarch64-apple-darwin`. Fixtures are the
suite's deterministic synthetic images; decode and crop input fixtures are
PNG-encoded with pinned settings (`Default` compression, adaptive
filtering) so their bytes do not depend on the library's own encoder
configuration.

All crop benchmarks use the demo's default framing — 480×480 viewport,
centred 240 px square stencil, zoom 1.0, no pan — so the output side is
`240 / fit_scale` source pixels: 512, 1732 and 3840 for the three sources.

## How the numbers are taken

Absolute times on this machine move by 2× or more between sessions
(thermal state, background load), and a single sequential `cargo bench`
per version can land the two versions in different conditions. To keep a
version comparison honest:

1. Build the bench binary for each version (`cargo bench --no-run`; the
   0.0.2 checkout gets the same `benches/crop.rs` and the same criterion
   dev-dependency, since it predates the suite).
2. For each benchmark name, run the two binaries back to back with
   `--bench '^<name>$'`, alternating versions.
3. Repeat the whole pass three times and keep the best median per
   benchmark and version.

Interleaving means both versions see the same machine state within
seconds of each other; best-of-three discards passes hit by background
load. The `decode` and `encode_png` groups run identical code with
identical bytes in both versions, so their rows double as a noise check:
if they differ by more than a few percent, the run is not trustworthy.

## 0.0.2 → 0.0.3

| Benchmark | 0.0.2 | 0.0.3 | 0.0.2 ÷ 0.0.3 |
|---|---:|---:|---:|
| decode/3464×2310 (8 MP) | 97.1 ms | 89.5 ms | 1.09 |
| decode/7680×4320 (33 MP) | 371.0 ms | 371.5 ms | 1.00 |
| crop/1024×1024, 0° | 2.01 ms | 1.19 ms | 1.69 |
| crop/1024×1024, 90° | 1.98 ms | 1.44 ms | 1.37 |
| crop/1024×1024, 37° | 1.99 ms | 1.44 ms | 1.39 |
| crop/3464×2310, 0° | 24.6 ms | 16.9 ms | 1.45 |
| crop/3464×2310, 90° | 27.1 ms | 22.2 ms | 1.22 |
| crop/3464×2310, 37° | 25.0 ms | 19.8 ms | 1.27 |
| crop/7680×4320, 0° | 124.9 ms | 88.2 ms | 1.42 |
| crop/7680×4320, 90° | 160.0 ms | 138.4 ms | 1.16 |
| crop/7680×4320, 37° | 120.6 ms | 95.9 ms | 1.26 |
| encode_png/512×512 | 1.14 ms | 1.15 ms | 1.00 |
| encode_png/1732×1732 | 15.6 ms | 14.8 ms | 1.06 |
| encode_png/3840×3840 | 74.7 ms | 81.2 ms | 0.92 |

Reading the table:

- `decode` and `encode_png` are unchanged between the versions; their
  rows sit within 9% of each other, which is this run's noise bound.
- Every `crop` row is faster in 0.0.3, by 16% to 69%, all beyond the
  noise bound. The `crop` rows measure sampling *plus* PNG encode, and
  encode dominates at these output sizes, so the end-to-end delta
  understates the sampling-stage change: at 0° the sampling stage is a
  block row copy in 0.0.3, and subtracting the matching `encode_png` row
  shows it.
- Decode remains the dominant fixed cost for large sources, which is why
  `DecodedSource` caching (decode once, crop many times) matters more
  than any constant factor here.
- PNG stream structure has outsized effects on both encode and decode
  time: the encoder's content heuristics can emit stored (uncompressed)
  streams that decode an order of magnitude faster than densely
  compressed ones. This is why the fixture encoding is pinned, and why
  absolute numbers here do not transfer to other content.

On `wasm32` in a browser the same relative shape holds with larger
absolute times; the demo shows per-stage wall times (read, probe,
decode, crop, url) live under the stage for exactly this reason.
