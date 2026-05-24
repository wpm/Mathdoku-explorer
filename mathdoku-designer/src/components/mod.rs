//! SVG components that render a Mathdoku puzzle.
//!
//! [`Puzzle`] is the top-level component. It owns all interaction state and
//! provides a [`GridContext`] to its children so they can read mode and
//! selection without prop-drilling.

pub mod cage;
pub mod cage_stats;
pub mod cell;
pub mod puzzle;
pub mod region;
pub mod selection;
pub mod solution_count;

pub use puzzle::Puzzle;
