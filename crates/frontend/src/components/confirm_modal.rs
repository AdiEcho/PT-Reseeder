use leptos::ev;
use leptos::html::Div;
use leptos::prelude::*;

#[component]
pub fn ConfirmModal(
    title: &'static str,
    message: String,
    on_confirm: impl Fn() + 'static + Clone,
    on_cancel: impl Fn() + 'static + Clone,
    #[prop(default = "确认")] confirm_label: &'static str,
    #[prop(default = "取消")] cancel_label: &'static str,
    #[prop(default = false)] danger: bool,
) -> impl IntoView {
    let on_cancel_esc = on_cancel.clone();
    let on_cancel_btn = on_cancel.clone();
    let on_cancel_overlay = on_cancel.clone();
    let overlay_ref = NodeRef::<Div>::new();

    // Focus overlay so Esc keydown is captured without global listeners.
    Effect::new(move |_| {
        if let Some(el) = overlay_ref.get() {
            let _ = el.focus();
        }
    });

    let on_keydown = move |e: ev::KeyboardEvent| {
        if e.key() == "Escape" {
            on_cancel_esc.clone()();
        }
    };

    let confirm_class = if danger {
        "btn btn--danger"
    } else {
        "btn btn-primary"
    };

    view! {
        <div
            class="confirm-overlay"
            tabindex="-1"
            node_ref=overlay_ref
            on:keydown=on_keydown
            on:click=move |_| on_cancel_overlay.clone()()
        >
            <div
                class="confirm-dialog"
                role="dialog"
                aria-modal="true"
                on:click=move |e| e.stop_propagation()
            >
                <h3>{title}</h3>
                <p>{message}</p>
                <div class="form-actions" style="justify-content: flex-end;">
                    <button
                        class="btn btn--outline"
                        on:click=move |_| on_cancel_btn.clone()()
                    >
                        {cancel_label}
                    </button>
                    <button
                        class=confirm_class
                        on:click=move |_| on_confirm.clone()()
                    >
                        {confirm_label}
                    </button>
                </div>
            </div>
        </div>
    }
}
