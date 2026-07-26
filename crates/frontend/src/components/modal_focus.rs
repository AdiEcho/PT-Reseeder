use leptos::html::Div;
use leptos::prelude::*;

/// Wires up focus management for a modal overlay.
///
/// Returns a `NodeRef` to attach to the overlay element, which needs
/// `tabindex="-1"` to be focusable. On mount the overlay takes focus so `Esc`
/// keydown lands without a global listener; on unmount focus returns to
/// whatever element opened the modal, so keyboard and screen-reader users are
/// not dropped back at the top of the document.
///
/// The restore half is wasm-only: `web_sys::HtmlElement` is neither `Send` nor
/// `Sync`, so it cannot live in a `StoredValue`, and a thread-local is the
/// simplest store that works on the single-threaded web target. On the server
/// this compiles down to focus-on-mount alone, which is inert there anyway
/// because effects do not run during SSR.
pub fn use_modal_focus() -> NodeRef<Div> {
    let overlay_ref = NodeRef::<Div>::new();

    Effect::new(move |_| {
        if let Some(el) = overlay_ref.get() {
            remember_opener();
            let _ = el.focus();
        }
    });

    on_cleanup(restore_opener);

    overlay_ref
}

#[cfg(target_arch = "wasm32")]
thread_local! {
    /// The element focused when the most recent modal opened. A plain slot
    /// rather than a stack: modals in this app are never nested.
    static OPENER: std::cell::RefCell<Option<web_sys::HtmlElement>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(target_arch = "wasm32")]
fn remember_opener() {
    use wasm_bindgen::JsCast;

    let active = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.active_element())
        .and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok());
    OPENER.with(|slot| *slot.borrow_mut() = active);
}

#[cfg(target_arch = "wasm32")]
fn restore_opener() {
    if let Some(el) = OPENER.with(|slot| slot.borrow_mut().take()) {
        let _ = el.focus();
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn remember_opener() {}

#[cfg(not(target_arch = "wasm32"))]
fn restore_opener() {}
