//! Cage component: operation label in the anchor cell corner.

use mathdoku::{Operation, Operator};
use leptos::prelude::*;

use crate::theme::{INK, OP_INSET, SERIF};

/// Formats a cage operation as a short label: `"+5"`, `"−2"`, `"×12"`, `"÷3"`, or `"7"`.
pub fn op_label(op: Operation) -> String {
    let t = op.target;
    match op.operator {
        Operator::Add => format!("+{t}"),
        Operator::Subtract => format!("\u{2212}{t}"),
        Operator::Multiply => format!("\u{00d7}{t}"),
        Operator::Divide => format!("\u{00f7}{t}"),
        Operator::Given => format!("{t}"),
    }
}

/// Operation label rendered at the top-left of the cage's anchor cell.
#[component]
pub fn Cage(
    /// Top-left x of the anchor cell.
    x: f64,
    /// Top-left y of the anchor cell.
    y: f64,
    /// Font size for the op label.
    op_f: f64,
    operation: Operation,
) -> impl IntoView {
    let text = op_label(operation);
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
