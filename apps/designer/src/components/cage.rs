//! Cage component: operation label in the anchor cell corner.

use crate::theme::{INK, OP_INSET, SERIF};
use leptos::prelude::*;
use mathdoku::Operation;

/// Operation label rendered at the top-left of the cage's anchor cell.
#[component]
#[allow(clippy::needless_pass_by_value)] // Leptos component props must be owned
pub fn Cage(
    /// Top-left x of the anchor cell.
    x: f64,
    /// Top-left y of the anchor cell.
    y: f64,
    /// Font size for the op label.
    op_f: f64,
    operation: Operation,
) -> impl IntoView {
    let text = operation.to_string();
    view! {
        <text
            x={x + OP_INSET} y={y + OP_INSET}
            text-anchor="start"
            dominant-baseline="hanging"
            font-family=SERIF
            font-size=op_f
            font-weight="700"
            fill=INK
        >{text}</text>
    }
}
