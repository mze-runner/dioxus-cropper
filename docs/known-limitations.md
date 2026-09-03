# Known limitations

## EXIF orientation is not applied

Phone-camera JPEGs commonly store their rotation as EXIF metadata instead
of rotating the pixel data. Browsers apply that metadata when rendering
(`image-orientation: from-image` is the default), but this crate's decoder
does not, and header-reported dimensions are pre-orientation. For a source
with EXIF orientation 5–8 the browser shows the image upright while the
crop samples the unrotated pixel buffer — the crop can come out rotated or
mirrored relative to the preview, with width and height swapped.

Workaround until this is supported: strip or apply EXIF orientation before
handing the file to the cropper (most image tooling can re-save an image
with the rotation baked into the pixels).
