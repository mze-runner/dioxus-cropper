//! Demo-local UI enums. Each folds into one of the crate's own geometry or
//! config types before it reaches `Cropper`.

use dioxus_cropper::geometry::{Size, Stencil};
use dioxus_cropper::CropperCursor;

/// Which of the crate's three stencil shapes the demo currently frames
/// with. Sizes are the demo's own arbitrary values, not something the
/// crate prescribes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShapeKind {
    Rectangle,
    #[default]
    Square,
    Circle,
}

impl ShapeKind {
    pub fn stencil(self) -> Stencil {
        match self {
            Self::Rectangle => Stencil::rectangle(320.0, 200.0),
            Self::Square => Stencil::square(240.0),
            Self::Circle => Stencil::circle(240.0),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Rectangle => "Rectangle",
            Self::Square => "Square",
            Self::Circle => "Circle",
        }
    }
}

/// The `viewport` footprint presets on the tuning rail — square, so the
/// stage stays visually square in every state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewportPreset {
    Small,
    #[default]
    Medium,
    Large,
}

impl ViewportPreset {
    pub fn size(self) -> Size {
        match self {
            Self::Small => Size::new(360.0, 360.0),
            Self::Medium => Size::new(480.0, 480.0),
            Self::Large => Size::new(600.0, 600.0),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Small => "Small",
            Self::Medium => "Medium",
            Self::Large => "Large",
        }
    }

    pub const ALL: [Self; 3] = [Self::Small, Self::Medium, Self::Large];
}

/// The `CropperCursor` variants surfaced on the tuning rail. Excludes
/// `Custom`, which takes an arbitrary CSS string — the demo has no text
/// input for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorChoice {
    #[default]
    Move,
    Grab,
    AllScroll,
    Crosshair,
    None,
}

impl CursorChoice {
    pub fn cursor(self) -> CropperCursor {
        match self {
            Self::Move => CropperCursor::Move,
            Self::Grab => CropperCursor::Grab,
            Self::AllScroll => CropperCursor::AllScroll,
            Self::Crosshair => CropperCursor::Crosshair,
            Self::None => CropperCursor::None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Move => "Move",
            Self::Grab => "Grab",
            Self::AllScroll => "All-scroll",
            Self::Crosshair => "Crosshair",
            Self::None => "None",
        }
    }

    pub const ALL: [Self; 5] = [
        Self::Move,
        Self::Grab,
        Self::AllScroll,
        Self::Crosshair,
        Self::None,
    ];
}
