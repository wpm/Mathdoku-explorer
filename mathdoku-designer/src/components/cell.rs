//! Cell component: background rect and domain digit display.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
)]

use leptos::prelude::*;

const INK: &str = "#26221b";
const INK3: &str = "#8b8476";
const SANS: &str = "'Inter', system-ui, sans-serif";
const DOMAIN_EDGE: f64 = 4.0;

// Pip positions as (x, y) fractions in [0,1]² for 1–9 domain values.
// 1–6 follow standard die faces; 7–9 are symmetric extensions.
const LAYOUTS: [&[(f32, f32)]; 9] = [
    /* 1 */ &[(0.5, 0.5)],
    /* 2 */ &[(0.25, 0.25), (0.75, 0.75)],
    /* 3 */ &[(0.25, 0.25), (0.5, 0.5), (0.75, 0.75)],
    /* 4 */ &[(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)],
    /* 5 */
    &[
        (0.25, 0.25),
        (0.75, 0.25),
        (0.5, 0.5),
        (0.25, 0.75),
        (0.75, 0.75),
    ],
    /* 6 */
    &[
        (0.25, 0.2),
        (0.75, 0.2),
        (0.25, 0.5),
        (0.75, 0.5),
        (0.25, 0.8),
        (0.75, 0.8),
    ],
    /* 7 */
    &[
        (0.25, 0.15),
        (0.75, 0.15),
        (0.25, 0.5),
        (0.5, 0.5),
        (0.75, 0.5),
        (0.25, 0.85),
        (0.75, 0.85),
    ],
    /* 8 */
    &[
        (0.25, 0.14),
        (0.75, 0.14),
        (0.25, 0.38),
        (0.75, 0.38),
        (0.25, 0.62),
        (0.75, 0.62),
        (0.25, 0.86),
        (0.75, 0.86),
    ],
    /* 9 */
    &[
        (0.25, 0.15),
        (0.5, 0.15),
        (0.75, 0.15),
        (0.25, 0.5),
        (0.5, 0.5),
        (0.75, 0.5),
        (0.25, 0.85),
        (0.5, 0.85),
        (0.75, 0.85),
    ],
];

fn domain_layout(count: usize) -> Option<&'static [(f32, f32)]> {
    LAYOUTS.get(count.wrapping_sub(1)).copied()
}

/// Background rect and domain digits for a single grid cell.
#[component]
#[allow(clippy::needless_pass_by_value)] // Leptos component props must be owned
pub fn Cell(
    x: f64,
    y: f64,
    cell: f64,
    domain: Vec<u8>,
    fill: &'static str,
    /// Top margin reserved for the cage op label.
    top_margin: f64,
    /// Grid dimension n (used for fallback layout).
    n: usize,
) -> impl IntoView {
    let zone_w = 2.0f64.mul_add(-DOMAIN_EDGE, cell);
    let zone_h = cell - top_margin - DOMAIN_EDGE;
    let domain_f = (zone_h / 3.5).clamp(7.0, zone_h);

    let mut glyphs: Vec<(f64, f64, String, f64, &'static str, &'static str)> = Vec::new();

    if domain.len() == 1 {
        let singleton_f = (cell * 0.5).max(12.0);
        glyphs.push((
            x + cell / 2.0,
            y + cell / 2.0,
            domain[0].to_string(),
            singleton_f,
            INK,
            "600",
        ));
    } else if !domain.is_empty() {
        let zone_x = x + DOMAIN_EDGE;
        let zone_y = y + top_margin;
        if let Some(pips) = domain_layout(domain.len()) {
            for (i, &(fx, fy)) in pips.iter().enumerate() {
                if let Some(&v) = domain.get(i) {
                    glyphs.push((
                        f64::from(fx).mul_add(zone_w, zone_x),
                        f64::from(fy).mul_add(zone_h, zone_y),
                        v.to_string(),
                        domain_f,
                        INK3,
                        "normal",
                    ));
                }
            }
        } else {
            // Fallback for count > 9: sub×sub grid.
            let sub = (n as f64).sqrt().ceil() as usize;
            let sub_w = zone_w / sub as f64;
            let sub_h = zone_h / sub as f64;
            for (i, &v) in domain.iter().enumerate() {
                let sr = i / sub;
                let sc = i % sub;
                glyphs.push((
                    (sc as f64 + 0.5).mul_add(sub_w, zone_x),
                    (sr as f64 + 0.5).mul_add(sub_h, zone_y),
                    v.to_string(),
                    domain_f,
                    INK3,
                    "normal",
                ));
            }
        }
    }

    view! {
        <rect x=x y=y width=cell height=cell fill=fill />
        {glyphs.into_iter().map(|(cx, cy, label, font_size, color, weight)| view! {
            <text
                x=cx y=cy
                text-anchor="middle"
                dominant-baseline="central"
                font-family=SANS
                font-size=font_size
                font-weight=weight
                fill=color
            >{label}</text>
        }).collect::<Vec<_>>()}
    }
}
