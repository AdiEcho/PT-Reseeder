//! Draggable column-width header cell for `stats-table`.
//!
//! Widths are stored in localStorage under `pt-reseeder-col-width:<col_key>`
//! so they survive reloads. First paint always uses `default_width` so SSR and
//! hydration match; the stored value is applied in a client-only effect.

use leptos::ev;
use leptos::prelude::*;

const STORAGE_PREFIX: &str = "pt-reseeder-col-width:";

fn storage_key(col_key: &str) -> String {
    format!("{STORAGE_PREFIX}{col_key}")
}

#[cfg(target_arch = "wasm32")]
fn load_col_width(col_key: &str, default: i32) -> i32 {
    let Some(window) = web_sys::window() else {
        return default;
    };
    let Some(storage) = window.local_storage().ok().flatten() else {
        return default;
    };
    match storage.get_item(&storage_key(col_key)).ok().flatten() {
        Some(v) => v.parse::<i32>().unwrap_or(default).max(40),
        None => default,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn load_col_width(_col_key: &str, default: i32) -> i32 {
    default
}

#[cfg(target_arch = "wasm32")]
fn save_col_width(col_key: &str, width: i32) {
    let Some(window) = web_sys::window() else {
        return;
    };
    if let Some(storage) = window.local_storage().ok().flatten() {
        let _ = storage.set_item(&storage_key(col_key), &width.to_string());
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn save_col_width(_col_key: &str, _width: i32) {}

/// A `<th>` whose right edge can be dragged to resize the column.
///
/// Pair with a table that has class `stats-table--resizable` (`table-layout: fixed`).
#[component]
pub fn ResizableTh(
    /// Stable id used for localStorage, e.g. `"reseed-items-title"`.
    #[prop(into)]
    col_key: String,
    #[prop(default = 140)]
    default_width: i32,
    #[prop(default = 64)]
    min_width: i32,
    #[prop(optional, into)]
    class: Option<String>,
    children: Children,
) -> impl IntoView {
    let (width, set_width) = signal(default_width);
    let (dragging, set_dragging) = signal(false);
    let start_x = StoredValue::new(0i32);
    let start_w = StoredValue::new(default_width);

    // Apply persisted width after hydration (avoids SSR/client mismatch).
    {
        let col_key = col_key.clone();
        Effect::new(move |_| {
            let stored = load_col_width(&col_key, default_width);
            if stored != default_width {
                set_width.set(stored);
            }
        });
    }

    let on_pointer_down = {
        let col_key = col_key.clone();
        move |ev: ev::PointerEvent| {
            // Only primary button / touch.
            if ev.button() != 0 {
                return;
            }
            ev.prevent_default();
            ev.stop_propagation();
            start_x.set_value(ev.client_x());
            start_w.set_value(width.get_untracked());
            set_dragging.set(true);

            #[cfg(target_arch = "wasm32")]
            {
                use wasm_bindgen::JsCast;
                if let Some(target) = ev.target() {
                    if let Ok(el) = target.dyn_into::<web_sys::Element>() {
                        let _ = el.set_pointer_capture(ev.pointer_id());
                    }
                }
            }
            let _ = &col_key;
        }
    };

    let on_pointer_move = move |ev: ev::PointerEvent| {
        if !dragging.get_untracked() {
            return;
        }
        let dx = ev.client_x() - start_x.get_value();
        let next = (start_w.get_value() + dx).max(min_width);
        set_width.set(next);
    };

    let on_pointer_up = {
        let col_key = col_key.clone();
        move |ev: ev::PointerEvent| {
            if !dragging.get_untracked() {
                return;
            }
            set_dragging.set(false);
            let final_w = width.get_untracked().max(min_width);
            set_width.set(final_w);
            save_col_width(&col_key, final_w);

            #[cfg(target_arch = "wasm32")]
            {
                use wasm_bindgen::JsCast;
                if let Some(target) = ev.target() {
                    if let Ok(el) = target.dyn_into::<web_sys::Element>() {
                        let _ = el.release_pointer_capture(ev.pointer_id());
                    }
                }
            }
        }
    };

    let th_class = class.unwrap_or_default();
    let th_class = if th_class.is_empty() {
        "resizable-th".to_string()
    } else {
        format!("resizable-th {th_class}")
    };

    view! {
        <th
            class=th_class
            style:width=move || format!("{}px", width.get())
            style:min-width=move || format!("{}px", width.get())
            style:max-width=move || format!("{}px", width.get())
        >
            <div class="resizable-th__label">{children()}</div>
            <div
                class="th-resize-handle"
                class:is-dragging=move || dragging.get()
                title="拖动调整列宽"
                on:pointerdown=on_pointer_down
                on:pointermove=on_pointer_move
                on:pointerup=on_pointer_up
                on:pointercancel=on_pointer_up
            ></div>
        </th>
    }
}
