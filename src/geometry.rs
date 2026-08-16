//! Plain geometry types shared across `dioxus-cropper`. No `dioxus`
//! dependency here — kept host-agnostic on purpose.

/// A 2D offset, in pixels. In [`Cropper`](crate::Cropper) this is the
/// image's displacement from the centre of the viewport — positive `x`
/// moves the image right, positive `y` moves it down.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    /// The horizontal coordinate.
    pub x: f32,
    /// The vertical coordinate.
    pub y: f32,
}

impl Point {
    /// The origin, `(0.0, 0.0)`.
    pub const ZERO: Point = Point { x: 0.0, y: 0.0 };

    /// Builds a point from its coordinates.
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// The caller-controlled view transform: where the image sits inside the
/// viewport (`offset`), how large it renders (`zoom`), and how it's turned
/// (`rotation`, in degrees).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewTransform {
    /// The image's displacement from the centre of the viewport, in screen
    /// pixels.
    pub offset: Point,
    /// The scale applied on top of the `object-fit: contain` fit — `1.0` is
    /// "fit exactly", greater than `1.0` zooms in.
    pub zoom: f32,
    /// The rotation applied to the image, in degrees, clockwise on screen.
    pub rotation: f32,
}

/// A 2D size, in pixels. Distinct from [`Point`] even though the fields are
/// shaped identically — a `Point` is a position (can be negative, relative
/// to a centre); a `Size` is always a non-negative extent (an image's
/// natural width/height, the viewport's fixed footprint).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    /// The extent along the horizontal axis.
    pub width: f32,
    /// The extent along the vertical axis.
    pub height: f32,
}

impl Size {
    /// Builds a size from its width and height.
    pub fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

/// The scale factor CSS `object-fit: contain` would apply to fit `natural`
/// entirely inside `viewport` while preserving aspect ratio: the SMALLER of
/// the two per-axis ratios, so the more constraining axis governs and the
/// other axis leaves visible margin. Not clamped to `1.0` — a `natural`
/// smaller than `viewport` is scaled UP to fill it. Callers only ever pass a
/// successfully decoded image's real (non-zero) dimensions, so this never
/// divides by zero.
pub fn contain_scale(natural: Size, viewport: Size) -> f32 {
    (viewport.width / natural.width).min(viewport.height / natural.height)
}

/// The axis-aligned bounding box of `size` rotated by `rotation_deg` about
/// its own centre. Standard rotated-rectangle bounding box: each output axis
/// picks up a contribution from both input axes, weighted by the rotation's
/// cosine and sine.
///
/// Used by both [`min_zoom_to_cover`] (is the displayed image at least as
/// large as the stencil, after rotation) and [`clamp_offset`] (how far can
/// the caller-controlled offset travel before the rotated image's edge
/// enters the stencil).
///
/// A bounding-box test is a *weaker* guarantee than true rotated-rectangle
/// coverage: at intermediate angles the axis-aligned box can fully enclose
/// the stencil while the rotated rectangle's own corners still clip into
/// it — the box is looser than the shape it bounds.
pub fn rotated_bounding_box(size: Size, rotation_deg: f32) -> Size {
    let (sin, cos) = rotation_deg.to_radians().sin_cos();
    Size::new(
        size.width * cos.abs() + size.height * sin.abs(),
        size.width * sin.abs() + size.height * cos.abs(),
    )
}

/// The lowest `zoom` at which the displayed image — `natural * fit_scale *
/// zoom`, rotated by `rotation_deg` — covers `stencil` on both axes. Below
/// this zoom, no `offset` exists that keeps the stencil fully inside the
/// image: the displayed (rotated) bounding box is narrower or shorter than
/// the stencil on at least one axis, so the hole shows empty space no matter
/// where the image is panned.
///
/// Returns the coverage-only bound — it does not know about a caller's own
/// zoom floor. Callers combine the two:
/// `effective_min_zoom = caller_min_zoom.max(min_zoom_to_cover(...))`.
///
/// Uses [`rotated_bounding_box`] and therefore inherits its bounding-box
/// caveat — see that function's doc.
pub fn min_zoom_to_cover(
    natural: Size,
    fit_scale: f32,
    rotation_deg: f32,
    stencil: Stencil,
) -> f32 {
    let displayed_at_unit_zoom = Size::new(natural.width * fit_scale, natural.height * fit_scale);
    let bbox = rotated_bounding_box(displayed_at_unit_zoom, rotation_deg);
    (stencil.width() / bbox.width).max(stencil.height() / bbox.height)
}

/// Clamps `offset` so the displayed image — `natural * fit_scale * zoom`,
/// rotated by `rotation_deg` — keeps `stencil` fully covered. The allowed
/// range per axis is half the overhang between the rotated bounding box and
/// the stencil; when the caller has already ensured `zoom >=
/// min_zoom_to_cover(...)` for the same inputs, that overhang is
/// non-negative on both axes and the clamp is a real (non-empty) range. If
/// it hasn't (a caller skipping the zoom floor), the range collapses to a
/// single point rather than going negative — `.max(0.0)` — so this never
/// produces an inverted `clamp` bound.
///
/// Uses [`rotated_bounding_box`] and therefore inherits its bounding-box
/// caveat — see that function's doc.
pub fn clamp_offset(
    offset: Point,
    natural: Size,
    fit_scale: f32,
    zoom: f32,
    rotation_deg: f32,
    stencil: Stencil,
) -> Point {
    let displayed = Size::new(
        natural.width * fit_scale * zoom,
        natural.height * fit_scale * zoom,
    );
    let bbox = rotated_bounding_box(displayed, rotation_deg);
    let max_x = ((bbox.width - stencil.width()) / 2.0).max(0.0);
    let max_y = ((bbox.height - stencil.height()) / 2.0).max(0.0);
    Point::new(offset.x.clamp(-max_x, max_x), offset.y.clamp(-max_y, max_y))
}

/// The identity view: no offset, unit zoom, no rotation — the image sits
/// centred, unscaled, unturned. This is the state every consumer resets to
/// when the underlying image changes: `offset`/`zoom`/`rotation` describe
/// how *this particular image* sits, so a freshly loaded image should never
/// inherit the previous one's pan, zoom or rotation.
impl Default for ViewTransform {
    fn default() -> Self {
        Self {
            offset: Point::ZERO,
            zoom: 1.0,
            rotation: 0.0,
        }
    }
}

/// The stencil's outline. `Square` and `Circle` are not merely a `Rectangle`
/// with equal sides passed in by convention — [`Stencil`]'s constructors are
/// the only way to build one, so a circle with unequal width and height is
/// not a value this type can hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StencilShape {
    /// An axis-aligned rectangle; width and height are independent.
    Rectangle,
    /// A rectangle with equal width and height.
    Square,
    /// A circle; width and height are equal and give its diameter.
    Circle,
}

/// The crop window: a shape and a size, centred inside the viewport.
///
/// Fields are private; [`Stencil::rectangle`], [`Stencil::square`] and
/// [`Stencil::circle`] are the only constructors, which is what keeps
/// `Square`/`Circle` always equal-sided.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stencil {
    shape: StencilShape,
    width: f32,
    height: f32,
}

impl Stencil {
    /// Builds a rectangular stencil with independent width and height.
    pub fn rectangle(width: f32, height: f32) -> Self {
        Self {
            shape: StencilShape::Rectangle,
            width,
            height,
        }
    }

    /// Builds a square stencil with the given side length.
    pub fn square(size: f32) -> Self {
        Self {
            shape: StencilShape::Square,
            width: size,
            height: size,
        }
    }

    /// Builds a circular stencil with the given diameter.
    pub fn circle(diameter: f32) -> Self {
        Self {
            shape: StencilShape::Circle,
            width: diameter,
            height: diameter,
        }
    }

    /// This stencil's shape.
    pub fn shape(&self) -> StencilShape {
        self.shape
    }

    /// This stencil's width, in pixels.
    pub fn width(&self) -> f32 {
        self.width
    }

    /// This stencil's height, in pixels.
    pub fn height(&self) -> f32 {
        self.height
    }
}

/// Normalises a rotation angle into `[0, 360)` degrees — a full turn is
/// geometrically identical to zero, and a negative angle maps to its
/// positive equivalent. Normalising an already-in-range value is the
/// identity: applying a 90° step four times returns to exactly the starting
/// angle, with no accumulated float error.
pub fn normalize_rotation(deg: f32) -> f32 {
    deg.rem_euclid(360.0)
}
