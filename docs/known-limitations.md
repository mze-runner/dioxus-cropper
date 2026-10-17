# Known limitations

## Browser canvas size limit

`crop_async` on wasm32 encodes through a 2D canvas. Some engines cap canvas
area below `MAX_OUTPUT_PIXELS` (WebKit at about 16.7 megapixels); a larger
output fails with `CropError::Encode` instead of producing an image.
