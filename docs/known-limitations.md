# Known limitations

## Source formats

`natural_size` and the native decode accept PNG, JPEG, GIF and WebP. Other
formats a webview can display (AVIF, HEIC, BMP, TIFF) are rejected with
`CropError::Decode`. GIF and animated WebP crop their first frame.

## Browser canvas size limit

`crop_async` on wasm32 encodes through a 2D canvas. Some engines cap canvas
area below `MAX_OUTPUT_PIXELS` (WebKit at about 16.7 megapixels); a larger
output fails with `CropError::Encode` instead of producing an image.
