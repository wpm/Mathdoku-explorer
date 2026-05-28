//! `SolutionCount` component: number of solutions when the puzzle is complete.

use leptos::prelude::*;
use leptos::task::spawn_local;

use super::puzzle::InteractionState;

/// Displays the number of solutions right-aligned below the puzzle when the
/// puzzle is complete. Shows a busy indicator (`…`) while the solver runs.
///
/// Delegates completeness to the library: `solution_count()` returns `None`
/// immediately for incomplete puzzles without running the solver.
#[component]
#[allow(clippy::panic)]
pub fn SolutionCount() -> impl IntoView {
    let ctx = use_context::<InteractionState>()
        .unwrap_or_else(|| panic!("SolutionCount must be inside Puzzle"));
    // None = still computing, Some(n) = solver finished with count n.
    let count: RwSignal<Option<usize>> = RwSignal::new(None);
    // Whether the async call has resolved (distinguishes "solving" from "incomplete").
    let resolved: RwSignal<bool> = RwSignal::new(false);

    spawn_local(async move {
        let n = ctx.partial_solution.solution_count();
        count.set(n);
        resolved.set(true);
    });

    move || {
        if resolved.get() {
            // Resolved: show count if complete, nothing if incomplete.
            count
                .get()
                .map(|n| view! { <div class="solution-count">{pluralize_solutions(n)}</div> })
        } else {
            // Still solving — show busy indicator only if the puzzle is complete.
            // We don't yet know, so show "…" and let it resolve momentarily.
            Some(view! { <div class="solution-count">{"…".to_owned()}</div> })
        }
    }
}

fn pluralize_solutions(n: usize) -> String {
    if n == 1 {
        "1 solution".to_owned()
    } else {
        format!("{n} solutions")
    }
}

#[cfg(test)]
mod tests {
    use super::pluralize_solutions;

    #[test]
    fn one_solution_is_singular() {
        assert_eq!(pluralize_solutions(1), "1 solution");
    }

    #[test]
    fn zero_solutions_is_plural() {
        assert_eq!(pluralize_solutions(0), "0 solutions");
    }

    #[test]
    fn many_solutions_is_plural() {
        assert_eq!(pluralize_solutions(3), "3 solutions");
    }
}
