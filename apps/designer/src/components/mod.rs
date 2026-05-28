//! SVG components that render a Mathdoku puzzle.
//!
//! [`Puzzle`] is the only item exported from this module. All submodules are
//! internal; components wire up via Leptos context rather than direct imports.

pub mod cage;
pub mod cage_stats;
pub mod cell;
pub mod operation_selector;
pub mod provisional_cage;
pub mod puzzle;
pub mod selection;
pub mod solution_count;

pub use operation_selector::PendingCommit;
pub use puzzle::Puzzle;
