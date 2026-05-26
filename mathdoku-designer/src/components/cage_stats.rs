//! `CageStats` component: viable multiset/tuple counts for the active cage.

use leptos::prelude::*;
use mathdoku_designer_shared::Mode;

use super::puzzle::GridContext;

fn pluralize(n: usize, singular: &str, plural: &str) -> String {
    if n == 1 {
        format!("1 {singular}")
    } else {
        format!("{n} {plural}")
    }
}

#[cfg(test)]
mod tests {
    use super::pluralize;

    #[test]
    fn pluralize_one() {
        assert_eq!(pluralize(1, "Multiset", "Multisets"), "1 Multiset");
        assert_eq!(pluralize(1, "Tuple", "Tuples"), "1 Tuple");
    }

    #[test]
    fn pluralize_zero_and_many() {
        assert_eq!(pluralize(0, "Multiset", "Multisets"), "0 Multisets");
        assert_eq!(pluralize(7, "Tuple", "Tuples"), "7 Tuples");
    }
}

/// Displays viable multiset and tuple counts for the cage that contains the
/// current selection. Renders nothing when the selection is not in a cage.
#[component]
#[allow(clippy::panic)]
pub fn CageStats() -> impl IntoView {
    let ctx =
        use_context::<GridContext>().unwrap_or_else(|| panic!("CageStats must be inside Puzzle"));
    let GridContext {
        mode,
        selected_cell,
        selected_slot,
        cell_slot,
        puzzle_ref,
        ..
    } = ctx;

    move || {
        let slot_idx = match mode.get() {
            Mode::Cell => {
                let (r, c) = selected_cell.get();
                cell_slot
                    .get(r)
                    .and_then(|row| row.get(c))
                    .copied()
                    .flatten()
            }
            Mode::Slot => Some(selected_slot.get()),
        };

        let viable = slot_idx.and_then(|i| puzzle_ref.viable_counts(i));

        viable.map(|(multisets, tuples)| {
            let text = format!(
                "{}, {}",
                pluralize(multisets, "Multiset", "Multisets"),
                pluralize(tuples, "Tuple", "Tuples"),
            );
            view! { <div class="cage-stats">{text}</div> }
        })
    }
}
