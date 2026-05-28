//! Provisional cage component: "?" label in the anchor cell corner.

#![allow(dead_code)] // Leptos #[component] macro generates structs from props; dead_code can't see the macro's use of
// them

use leptos::prelude::*;

use crate::theme::{INK, OP_INSET, SERIF};

/// "?" label rendered at the top-left of the provisional cage's anchor cell.
#[component]
pub fn ProvisionalCage(
    /// Top-left x of the anchor cell.
    x: f64,
    /// Top-left y of the anchor cell.
    y: f64,
    /// Font size for the label.
    op_f: f64,
) -> impl IntoView {
    view! {
        <text
            x={x + OP_INSET} y={y + OP_INSET}
            text-anchor="start"
            dominant-baseline="hanging"
            font-family=SERIF
            font-size=op_f
            font-weight="700"
            fill=INK
        >"?"</text>
    }
}
