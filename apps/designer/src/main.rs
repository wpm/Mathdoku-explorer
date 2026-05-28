mod app;
mod cage_commit;
mod components;
pub mod feasibility;
pub mod geometry;
pub mod ipc;
pub mod keys;
pub mod partial_solution;
mod theme;

use app::App;
use leptos::prelude::*;

fn main() {
    console_error_panic_hook::set_once();
    mount_to_body(|| view! { <App /> });
}
