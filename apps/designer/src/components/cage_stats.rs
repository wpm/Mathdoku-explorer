//! `CageStats` component: viable multiset/tuple counts for the active cage.

use leptos::prelude::*;

use super::puzzle::InteractionState;

/// Displays viable multiset and tuple counts for the cage that contains the
/// current selection. Renders nothing when the selection is not in a cage.
#[component]
#[allow(clippy::panic)]
pub fn CageStats() -> impl IntoView {
    let ctx = use_context::<InteractionState>()
        .unwrap_or_else(|| panic!("CageStats must be inside Puzzle"));
    let InteractionState {
        designer_state,
        partial_solution,
        ..
    } = ctx;

    move || {
        let sel = designer_state.get().active;
        let cage_idx = partial_solution.cage_index_at(sel.row(), sel.column());
        let viable = cage_idx.and_then(|i| partial_solution.viable_counts(i));

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

fn pluralize(n: u64, singular: &str, plural: &str) -> String {
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
