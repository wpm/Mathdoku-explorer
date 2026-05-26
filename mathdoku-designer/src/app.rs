#![allow(
    clippy::future_not_send,       // WASM async is inherently single-threaded
    clippy::unwrap_used,           // serde_wasm_bindgen / JsCast are infallible here
    clippy::items_after_statements, // use wasm_bindgen::JsCast inside async blocks
    clippy::too_many_lines         // App component is inherently long
)]

use mathdoku::Puzzle;
use mathdoku_designer_shared::{DocState, ViewState};
use leptos::prelude::*;
use wasm_bindgen::prelude::*;

use crate::theme::{ACCENT, BG, INK, INK2, LINE, SANS as SANS_FONT};

// ---- Tauri glue ----
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    async fn invoke(cmd: &str, args: JsValue) -> JsValue;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"])]
    async fn listen(event: &str, handler: &js_sys::Function) -> JsValue;
}

#[derive(serde::Serialize)]
struct NewPuzzleArgs {
    n: usize,
}

#[derive(serde::Serialize)]
struct PathArgs {
    path: String,
}

async fn get_doc_state() -> DocState {
    let v = invoke("get_doc_state", JsValue::NULL).await;
    serde_wasm_bindgen::from_value(v).unwrap_or_default()
}

async fn call_new_puzzle(n: usize) -> Option<Puzzle> {
    let args = serde_wasm_bindgen::to_value(&NewPuzzleArgs { n }).unwrap();
    let result = invoke("new_puzzle", args).await;
    serde_wasm_bindgen::from_value(result).ok()
}

async fn call_generate_puzzle(n: usize) -> Option<Puzzle> {
    let args = serde_wasm_bindgen::to_value(&NewPuzzleArgs { n }).unwrap();
    let result = invoke("generate_puzzle", args).await;
    serde_wasm_bindgen::from_value(result).ok()
}

async fn call_save_puzzle() -> Option<String> {
    let state = get_doc_state().await;
    let path = match state.path {
        Some(p) => Some(p),
        None => pick_save_path().await,
    };
    if let Some(path) = path {
        let args = serde_wasm_bindgen::to_value(&PathArgs { path: path.clone() }).unwrap();
        invoke("save_puzzle", args).await;
        return Some(path);
    }
    None
}

async fn call_save_as_puzzle() -> Option<String> {
    if let Some(path) = pick_save_path().await {
        let args = serde_wasm_bindgen::to_value(&PathArgs { path: path.clone() }).unwrap();
        invoke("save_puzzle", args).await;
        return Some(path);
    }
    None
}

async fn eval_promise_string(js: &str) -> Option<String> {
    use wasm_bindgen::JsCast;
    let v = js_sys::eval(js).ok()?;
    wasm_bindgen_futures::JsFuture::from(v.dyn_into::<js_sys::Promise>().unwrap())
        .await
        .ok()
        .and_then(|v| v.as_string())
}

async fn call_load_puzzle() -> Option<Puzzle> {
    let path = eval_promise_string(
        r"window.__TAURI__.dialog.open({ filters: [{ name: 'Mathdoku', extensions: ['mathdoku'] }] })",
    )
    .await?;
    let args = serde_wasm_bindgen::to_value(&PathArgs { path }).unwrap();
    let result = invoke("load_puzzle", args).await;
    serde_wasm_bindgen::from_value(result).ok()
}

fn basename(path: &str) -> &str {
    path.rsplit(&['/', '\\']).next().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::basename;

    #[test]
    fn unix_path() {
        assert_eq!(basename("/home/user/puzzle.mathdoku"), "puzzle.mathdoku");
    }

    #[test]
    fn windows_path() {
        assert_eq!(basename(r"C:\Users\user\puzzle.mathdoku"), "puzzle.mathdoku");
    }

    #[test]
    fn bare_filename() {
        assert_eq!(basename("puzzle.mathdoku"), "puzzle.mathdoku");
    }

    #[test]
    fn empty_string() {
        assert_eq!(basename(""), "");
    }
}

async fn pick_save_path() -> Option<String> {
    eval_promise_string(
        r"window.__TAURI__.dialog.save({ filters: [{ name: 'Mathdoku', extensions: ['mathdoku'] }] })",
    )
    .await
}

async fn call_quit() {
    invoke("quit_app", JsValue::NULL).await;
}

// ---- modal styles ----

const fn overlay_style() -> &'static str {
    "position:fixed;inset:0;background:rgba(0,0,0,0.35);z-index:2000;\
     display:flex;align-items:center;justify-content:center;"
}

fn dialog_style(min_w: u32, max_w: u32) -> String {
    format!(
        "background:{BG};border:0.5px solid {LINE};border-radius:8px;\
         box-shadow:0 4px 24px rgba(0,0,0,0.2);padding:24px 28px;\
         font-family:{SANS_FONT};min-width:{min_w}px;max-width:{max_w}px;"
    )
}

fn title_style() -> String {
    format!("font-size:16px;font-weight:600;color:{INK};margin:0 0 10px 0;")
}

fn body_style() -> String {
    format!("font-size:13.5px;color:{INK2};margin:0 0 16px 0;")
}

fn neutral_btn_style() -> String {
    format!(
        "padding:6px 16px;border:0.5px solid {LINE};border-radius:5px;\
         background:{BG};color:{INK};font-family:{SANS_FONT};font-size:13px;cursor:pointer;"
    )
}

fn primary_btn_style() -> String {
    // Same appearance as neutral; focus ring distinguishes keyboard focus.
    neutral_btn_style()
}

// ---- SizeModal ----

#[component]
fn SizeModal(
    default_n: usize,
    on_empty: Callback<usize>,
    on_random: Callback<usize>,
    on_cancel: Callback<()>,
) -> impl IntoView {
    let chosen = RwSignal::new(default_n);
    let select_style = format!(
        "padding:4px 8px;border:0.5px solid {LINE};border-radius:4px;\
         font-family:{SANS_FONT};font-size:13px;background:{BG};color:{INK};"
    );

    let _esc = window_event_listener(leptos::ev::keydown, move |ev| {
        if ev.key() == "Escape" {
            on_cancel.run(());
        }
    });

    // Tab trap: intercept Tab/Shift-Tab on the dialog so focus never escapes to
    // the grid SVG behind the overlay.  The four focusable children are the
    // <select> and the three buttons (DOM order matches Tab order).
    let trap_tab = move |ev: leptos::ev::KeyboardEvent| {
        if ev.key() != "Tab" {
            return;
        }
        use wasm_bindgen::JsCast;
        let dialog = ev
            .current_target()
            .and_then(|t| t.dyn_into::<web_sys::HtmlElement>().ok());
        let Some(dialog) = dialog else { return };
        let focusable = dialog
            .query_selector_all("select, button")
            .ok()
            .map(|nl| {
                (0..nl.length())
                    .filter_map(|i| nl.item(i)?.dyn_into::<web_sys::HtmlElement>().ok())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if focusable.is_empty() {
            return;
        }
        let doc = web_sys::window().and_then(|w| w.document());
        let active = doc.and_then(|d| d.active_element());
        let current_idx = active.and_then(|a| {
            focusable
                .iter()
                .position(|el| el.is_same_node(Some(a.as_ref())))
        });
        ev.prevent_default();
        let len = focusable.len();
        let next = if ev.shift_key() {
            current_idx.map_or(len - 1, |i| if i == 0 { len - 1 } else { i - 1 })
        } else {
            current_idx.map_or(0, |i| (i + 1) % len)
        };
        let _ = focusable[next].focus();
    };

    view! {
        <div
            style=overlay_style()
            on:mousedown=move |ev: leptos::ev::MouseEvent| {
                if ev.target() == ev.current_target() { on_cancel.run(()); }
            }
        >
            // `tabindex="-1"` lets this div receive the keydown event for the trap.
            <div style=dialog_style(280, 380) tabindex="-1" on:keydown=trap_tab>
                // Focus ring for buttons: inline styles cannot express :focus-visible,
                // so a scoped <style> block provides it.
                <style>
                    ".sz-btn:focus-visible { outline: 2px solid "
                    {ACCENT}
                    "; outline-offset: 2px; }"
                </style>
                <p style=title_style()>"New puzzle"</p>
                <p style=body_style()>"Choose a grid size."</p>
                <div style="display:flex;align-items:center;gap:8px;margin-bottom:20px;">
                    <label style=format!("font-size:13px;color:{INK};")>
                        "Size: "
                        <select
                            autofocus=true
                            style=select_style
                            on:change=move |ev: leptos::ev::Event| {
                                if let Ok(n) = event_target_value(&ev).parse::<usize>() {
                                    chosen.set(n);
                                }
                            }
                            prop:value=move || chosen.get().to_string()
                        >
                            <option value="3">"3"</option>
                            <option value="4">"4"</option>
                            <option value="5">"5"</option>
                            <option value="6">"6"</option>
                            <option value="7">"7"</option>
                            <option value="8">"8"</option>
                            <option value="9">"9"</option>
                        </select>
                    </label>
                </div>
                <div style="display:flex;justify-content:flex-end;gap:10px;">
                    <button class="sz-btn" style=neutral_btn_style() on:click=move |_| on_cancel.run(())>
                        "Cancel"
                    </button>
                    <button
                        class="sz-btn"
                        style=neutral_btn_style()
                        on:click=move |_| on_random.run(chosen.get_untracked())
                    >
                        "Random"
                    </button>
                    <button
                        class="sz-btn"
                        style=primary_btn_style()
                        on:click=move |_| on_empty.run(chosen.get_untracked())
                    >
                        "Empty"
                    </button>
                </div>
            </div>
        </div>
    }
}

// ---- UnsavedChangesModal ----

#[component]
fn UnsavedChangesModal(
    on_save: Callback<()>,
    on_discard: Callback<()>,
    on_cancel: Callback<()>,
) -> impl IntoView {
    view! {
        <div
            style=overlay_style()
            on:mousedown=move |ev: leptos::ev::MouseEvent| {
                if ev.target() == ev.current_target() { on_cancel.run(()); }
            }
        >
            <div style=dialog_style(340, 420)>
                <p style=title_style()>"Save changes before closing?"</p>
                <p style=body_style()>"This puzzle has unsaved changes."</p>
                <div style="display:flex;justify-content:flex-end;gap:10px;flex-wrap:wrap;">
                    <button style=neutral_btn_style() on:click=move |_| on_discard.run(())>
                        "Don\u{2019}t Save"
                    </button>
                    <button style=neutral_btn_style() on:click=move |_| on_cancel.run(())>
                        "Cancel"
                    </button>
                    <button
                        autofocus=true
                        style=primary_btn_style()
                        on:click=move |_| on_save.run(())
                    >
                        "Save"
                    </button>
                </div>
            </div>
        </div>
    }
}

// ---- App ----

#[component]
pub fn App() -> impl IntoView {
    let show_size_modal = RwSignal::new(false);
    let show_unsaved_modal = RwSignal::new(false);
    let puzzle = RwSignal::new(None::<Puzzle>);
    let view_state = RwSignal::new(ViewState::default());
    let current_path: RwSignal<Option<String>> = RwSignal::new(None);

    // Check if a puzzle was already restored from the recent file on startup.
    leptos::task::spawn_local(async move {
        let result = invoke("get_puzzle", JsValue::NULL).await;
        if let Ok(p) = serde_wasm_bindgen::from_value::<Puzzle>(result) {
            let vs = invoke("get_view_state", JsValue::NULL).await;
            if let Ok(v) = serde_wasm_bindgen::from_value::<ViewState>(vs) {
                view_state.set(v);
            }
            let ds = get_doc_state().await;
            current_path.set(ds.path);
            puzzle.set(Some(p));
        }
    });

    leptos::task::spawn_local(async move {
        let new_cb = Closure::wrap(Box::new(move |_: JsValue| {
            show_size_modal.set(true);
        }) as Box<dyn Fn(JsValue)>);
        // Event names must match the EVENT_* constants in src-tauri/src/lib.rs.
        listen("menu-new", new_cb.as_ref().unchecked_ref()).await;
        new_cb.forget();

        let save_cb = Closure::wrap(Box::new(move |_: JsValue| {
            leptos::task::spawn_local(async move {
                if let Some(path) = call_save_puzzle().await {
                    current_path.set(Some(path));
                }
            });
        }) as Box<dyn Fn(JsValue)>);
        listen("menu-save", save_cb.as_ref().unchecked_ref()).await;
        save_cb.forget();

        let save_as_cb = Closure::wrap(Box::new(move |_: JsValue| {
            leptos::task::spawn_local(async move {
                if let Some(path) = call_save_as_puzzle().await {
                    current_path.set(Some(path));
                }
            });
        }) as Box<dyn Fn(JsValue)>);
        listen("menu-save-as", save_as_cb.as_ref().unchecked_ref()).await;
        save_as_cb.forget();

        let load_cb = Closure::wrap(Box::new(move |_: JsValue| {
            leptos::task::spawn_local(async move {
                if let Some(p) = call_load_puzzle().await {
                    let ds = get_doc_state().await;
                    current_path.set(ds.path);
                    view_state.set(ViewState::default());
                    puzzle.set(Some(p));
                }
            });
        }) as Box<dyn Fn(JsValue)>);
        listen("menu-open", load_cb.as_ref().unchecked_ref()).await;
        load_cb.forget();

        let close_cb = Closure::wrap(Box::new(move |_: JsValue| {
            show_unsaved_modal.set(true);
        }) as Box<dyn Fn(JsValue)>);
        listen("request-close", close_cb.as_ref().unchecked_ref()).await;
        close_cb.forget();
    });

    let on_empty = Callback::new(move |n: usize| {
        show_size_modal.set(false);
        leptos::task::spawn_local(async move {
            if let Some(p) = call_new_puzzle(n).await {
                current_path.set(None);
                view_state.set(ViewState::default());
                puzzle.set(Some(p));
            }
        });
    });
    let on_random = Callback::new(move |n: usize| {
        show_size_modal.set(false);
        leptos::task::spawn_local(async move {
            if let Some(p) = call_generate_puzzle(n).await {
                current_path.set(None);
                view_state.set(ViewState::default());
                puzzle.set(Some(p));
            }
        });
    });
    let on_create_cancel = Callback::new(move |(): ()| show_size_modal.set(false));

    let on_unsaved_save = Callback::new(move |(): ()| {
        show_unsaved_modal.set(false);
        leptos::task::spawn_local(async move {
            call_save_puzzle().await;
            call_quit().await;
        });
    });
    let on_unsaved_discard = Callback::new(move |(): ()| {
        show_unsaved_modal.set(false);
        leptos::task::spawn_local(async move { call_quit().await });
    });
    let on_unsaved_cancel = Callback::new(move |(): ()| show_unsaved_modal.set(false));

    Effect::new(move |_| {
        let title = current_path
            .get()
            .map(|p| basename(&p).to_owned())
            .unwrap_or_default();
        leptos::task::spawn_local(async move {
            #[derive(serde::Serialize)]
            struct TitleArgs {
                title: String,
            }
            let args = serde_wasm_bindgen::to_value(&TitleArgs { title }).unwrap();
            invoke("set_window_title", args).await;
        });
    });

    view! {
        <main class="app-main">
            {move || puzzle.get().map(|p| {
            let on_puzzle_change = Callback::new(move |(new_puzzle, new_view): (Puzzle, mathdoku_designer_shared::ViewState)| {
                view_state.set(new_view);
                puzzle.set(Some(new_puzzle));
            });
            view! { <crate::components::Puzzle puzzle=p initial_view=view_state.get() on_puzzle_change=on_puzzle_change /> }
        })}
            {move || show_size_modal.get().then(|| view! {
                <SizeModal
                    default_n=puzzle.get().map_or(4, |p| p.n())
                    on_empty=on_empty
                    on_random=on_random
                    on_cancel=on_create_cancel
                />
            })}
            {move || show_unsaved_modal.get().then(|| view! {
                <UnsavedChangesModal
                    on_save=on_unsaved_save
                    on_discard=on_unsaved_discard
                    on_cancel=on_unsaved_cancel
                />
            })}
        </main>
    }
}
