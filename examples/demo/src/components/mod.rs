//! Pure presentational pieces for the rail and stage. Every component here
//! is props-in / markup-out — no signal reads, no business logic. `main.rs`
//! owns all state and wires these together.

pub mod position_group;
pub mod readout;
pub mod result_strip;
pub mod rotate_group;
pub mod shape_group;
pub mod source_group;
pub mod stage;
pub mod tuning_group;
pub mod zoom_group;

pub use position_group::PositionGroup;
pub use readout::StageReadout;
pub use result_strip::ResultStrip;
pub use rotate_group::RotateGroup;
pub use shape_group::ShapeGroup;
pub use source_group::SourceGroup;
pub use stage::{Stage, StageSource};
pub use tuning_group::TuningGroup;
pub use zoom_group::ZoomGroup;
