# Crop pipeline performance

Measurements from the `benches/crop.rs` criterion suite (`cargo bench`) on
an Apple M3 (8 cores, 16 GB), macOS. Fixtures are the suite's deterministic
synthetic images; decode and crop input fixtures are PNG-encoded with
pinned settings (`Default` compression, adaptive filtering) so their bytes
do not depend on the library's own encoder configuration.

All crop benchmarks use the demo's default framing — 480×480 viewport,
centred 240 px square stencil, zoom 1.0, no pan — so the output side is
`240 / fit_scale` source pixels: 512, 1732 and 3840 for the three sources.

## 0.0.2 → 0.0.3

Both versions benchmarked with the identical harness and fixtures,
sequentially in one session. The `encode_png` rows execute the same code
path in both versions (0.0.3 pins the encoder settings the 0.0.2 defaults
resolved to, so encoded bytes are identical); their measured spread —
here up to roughly +25% against the run that went second — is the
measurement noise bound for this table. Crop deltas larger than that bound
are real; smaller ones are not distinguishable from noise.

| Benchmark | 0.0.2 | 0.0.3 |
|---|---:|---:|
| decode/3464×2310 (8 MP) | 89.6 ms | 114.5 ms |
| decode/7680×4320 (33 MP) | 368.2 ms | 504.4 ms |
| crop/1024×1024, 0° | 3.40 ms | 1.62 ms |
| crop/1024×1024, 90° | 2.23 ms | 1.90 ms |
| crop/1024×1024, 37° | 2.12 ms | 1.96 ms |
| crop/3464×2310, 0° | 28.2 ms | 27.0 ms |
| crop/3464×2310, 90° | 29.3 ms | 27.7 ms |
| crop/3464×2310, 37° | 26.9 ms | 56.6 ms† |
| crop/7680×4320, 0° | 150.8 ms | 117.6 ms |
| crop/7680×4320, 90° | 206.0 ms | 193.4 ms |
| crop/7680×4320, 37° | 170.7 ms | 127.2 ms |
| encode_png/512×512 | 1.42 ms | 1.51 ms |
| encode_png/1732×1732 | 18.3 ms | 20.1 ms |
| encode_png/3840×3840 | 95.2 ms | 120.2 ms |

† Confidence interval 38–77 ms in this run; an outlier against the
adjacent sizes and rotations.

Reading the table:

- The `crop` rows measure sampling *plus* PNG encode, and encode dominates
  at these output sizes, which caps the visible end-to-end delta. The
  sampling stage itself changed most: at 0° it is a block row copy in
  0.0.3, and the 8K rows beat 0.0.2 by more than the noise bound even
  measured on the disadvantaged (second, warmer) half of the session.
- The `decode` and `encode_png` rows compare identical code and identical
  bytes between the versions; their deltas are machine noise, not library
  changes. Decode remains the dominant fixed cost for large sources, which
  is why `DecodedSource` caching (decode once, crop many times) matters
  more than any constant factor here.
- PNG stream structure has outsized effects on both encode and decode
  time: the encoder's content heuristics can emit stored (uncompressed)
  streams that decode an order of magnitude faster than densely compressed
  ones. This is why the fixture encoding is pinned, and why absolute
  numbers here do not transfer to other content.

These numbers are native (`aarch64-apple-darwin`) on a passively cooled
machine — thermal state shifts absolute times between sessions by 2× or
more, so only same-session comparisons are meaningful. On `wasm32` in a
browser the same relative shape holds with larger absolute times; the demo
shows per-stage wall times (read, probe, decode, crop, url) live under the
stage for exactly this reason.
