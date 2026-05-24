//! Region component: "?" label in the anchor cell corner.

use leptos::prelude::*;

const INK: &str = "#26221b";
const SERIF: &str = "'Fraunces', Georgia, serif";
const OP_INSET: f64 = 4.0;

/// "?" label rendered at the top-left of the region's anchor cell.
#[component]
pub fn Region(
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
