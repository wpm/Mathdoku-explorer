//! Cell component: background rect and domain digit display.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
)]

use leptos::prelude::*;

use crate::theme::{GREEN, INK, INK3, SANS};

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
    /// The correct solution value for this cell, if known. When present and the
    /// domain has multiple candidates, this value is rendered larger and in green.
    solution_value: Option<u8>,
) -> impl IntoView {
    let glyphs = cell_glyphs(x, y, cell, &domain, top_margin, n, solution_value);

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

const DOMAIN_EDGE: f64 = 4.0;

/// A positioned digit to render in a cell: `(x, y, label, font_size, fill, font_weight)`.
type Glyph = (f64, f64, String, f64, &'static str, &'static str);

/// Computes the positioned digit glyphs for a cell's domain.
///
/// A singleton domain renders one large centred digit; multiple candidates are
/// laid out as die-style pips (or a square sub-grid for counts above nine). When
/// `solution_value` matches a candidate it is drawn larger and in green.
fn cell_glyphs(
    x: f64,
    y: f64,
    cell: f64,
    domain: &[u8],
    top_margin: f64,
    n: usize,
    solution_value: Option<u8>,
) -> Vec<Glyph> {
    let zone_w = 2.0f64.mul_add(-DOMAIN_EDGE, cell);
    let zone_h = cell - top_margin - DOMAIN_EDGE;
    let domain_f = (zone_h / 3.5).clamp(7.0, zone_h);

    let mut glyphs: Vec<Glyph> = Vec::new();

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
        let solution_f = (domain_f * 1.35).min(zone_h);
        if let Some(pips) = domain_layout(domain.len()) {
            for (i, &(fx, fy)) in pips.iter().enumerate() {
                if let Some(&v) = domain.get(i) {
                    let is_solution = solution_value == Some(v);
                    glyphs.push((
                        f64::from(fx).mul_add(zone_w, zone_x),
                        f64::from(fy).mul_add(zone_h, zone_y),
                        v.to_string(),
                        if is_solution { solution_f } else { domain_f },
                        if is_solution { GREEN } else { INK3 },
                        if is_solution { "600" } else { "normal" },
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
                let is_solution = solution_value == Some(v);
                glyphs.push((
                    (sc as f64 + 0.5).mul_add(sub_w, zone_x),
                    (sr as f64 + 0.5).mul_add(sub_h, zone_y),
                    v.to_string(),
                    if is_solution { solution_f } else { domain_f },
                    if is_solution { GREEN } else { INK3 },
                    if is_solution { "600" } else { "normal" },
                ));
            }
        }
    }

    glyphs
}

fn domain_layout(count: usize) -> Option<&'static [(f32, f32)]> {
    LAYOUTS.get(count.wrapping_sub(1)).copied()
}

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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::{cell_glyphs, domain_layout};
    use crate::theme::{GREEN, INK, INK3};

    #[test]
    fn layout_count_matches_pip_count_for_one_through_nine() {
        for count in 1..=9 {
            let pips = domain_layout(count);
            assert!(pips.is_some(), "expected a layout for count {count}");
            assert_eq!(
                pips.map(<[(f32, f32)]>::len),
                Some(count),
                "layout for count {count} has the wrong number of pips"
            );
        }
    }

    #[test]
    fn layout_zero_is_none() {
        assert!(domain_layout(0).is_none());
    }

    #[test]
    fn layout_above_nine_is_none() {
        assert!(domain_layout(10).is_none());
        assert!(domain_layout(100).is_none());
    }

    #[test]
    fn layout_pips_are_within_unit_square() {
        for count in 1..=9 {
            if let Some(pips) = domain_layout(count) {
                for &(x, y) in pips {
                    assert!((0.0..=1.0).contains(&x), "x={x} out of range");
                    assert!((0.0..=1.0).contains(&y), "y={y} out of range");
                }
            }
        }
    }

    #[test]
    fn glyphs_empty_domain_produces_nothing() {
        let glyphs = cell_glyphs(0.0, 0.0, 60.0, &[], 16.0, 4, None);
        assert!(glyphs.is_empty());
    }

    #[test]
    fn glyphs_singleton_is_one_centered_ink_digit() {
        let glyphs = cell_glyphs(10.0, 20.0, 60.0, &[5], 16.0, 4, None);
        assert_eq!(glyphs.len(), 1);
        let (cx, cy, ref label, _font, fill, weight) = glyphs[0];
        // Centred within the cell.
        assert!((cx - 40.0).abs() < f64::EPSILON);
        assert!((cy - 50.0).abs() < f64::EPSILON);
        assert_eq!(label, "5");
        assert_eq!(fill, INK);
        assert_eq!(weight, "600");
    }

    #[test]
    fn glyphs_multi_domain_uses_pip_layout() {
        let glyphs = cell_glyphs(0.0, 0.0, 60.0, &[1, 2, 3], 16.0, 4, None);
        assert_eq!(glyphs.len(), 3);
        let labels: Vec<&str> = glyphs.iter().map(|g| g.2.as_str()).collect();
        assert_eq!(labels, vec!["1", "2", "3"]);
    }

    #[test]
    fn glyphs_highlight_solution_value_in_green() {
        let glyphs = cell_glyphs(0.0, 0.0, 60.0, &[1, 2, 3], 16.0, 4, Some(2));
        // The candidate equal to the solution value is green and bold; others grey.
        let two = glyphs.iter().find(|g| g.2 == "2").unwrap();
        let one = glyphs.iter().find(|g| g.2 == "1").unwrap();
        assert_eq!(two.4, GREEN);
        assert_eq!(two.5, "600");
        assert_eq!(one.4, INK3);
        assert_eq!(one.5, "normal");
    }

    #[test]
    fn glyphs_more_than_nine_use_square_fallback() {
        let domain: Vec<u8> = (1..=10).collect();
        let glyphs = cell_glyphs(0.0, 0.0, 120.0, &domain, 16.0, 10, None);
        // The fallback grid renders every candidate.
        assert_eq!(glyphs.len(), 10);
    }
}
