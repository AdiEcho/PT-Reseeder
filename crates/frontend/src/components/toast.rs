use leptos::prelude::*;
use std::sync::OnceLock;

#[derive(Clone, Debug)]
pub enum ToastType {
    Success,
    Error,
    Info,
}

#[derive(Clone, Debug)]
pub struct Toast {
    pub id: u64,
    pub message: String,
    pub toast_type: ToastType,
}

fn toast_signal() -> RwSignal<Vec<Toast>> {
    static TOASTS: OnceLock<RwSignal<Vec<Toast>>> = OnceLock::new();
    TOASTS.get_or_init(|| RwSignal::new(Vec::new())).clone()
}

fn toast_counter() -> RwSignal<u64> {
    static COUNTER: OnceLock<RwSignal<u64>> = OnceLock::new();
    COUNTER.get_or_init(|| RwSignal::new(0)).clone()
}

pub fn show_toast(message: impl Into<String>, toast_type: ToastType) {
    let counter = toast_counter();
    let id = counter.get_untracked() + 1;
    counter.set(id);

    let toast = Toast {
        id,
        message: message.into(),
        toast_type,
    };

    toast_signal().update(|toasts| {
        toasts.push(toast);
    });

    // Auto-dismiss after 3 seconds (client-side only)
    #[cfg(target_arch = "wasm32")]
    {
        let dismiss_id = id;
        leptos::task::spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(3000).await;
            dismiss_toast(dismiss_id);
        });
    }
}

pub fn dismiss_toast(id: u64) {
    toast_signal().update(|toasts| {
        toasts.retain(|t| t.id != id);
    });
}

/// Mount this once at the top level (e.g. in App or AppLayout).
#[component]
pub fn ToastContainer() -> impl IntoView {
    let toasts = toast_signal();

    view! {
        <div class="toast-container">
            {move || {
                toasts
                    .get()
                    .into_iter()
                    .map(|toast| {
                        let id = toast.id;
                        let class = match toast.toast_type {
                            ToastType::Success => "toast toast--success",
                            ToastType::Error => "toast toast--error",
                            ToastType::Info => "toast toast--info",
                        };
                        let icon = match toast.toast_type {
                            ToastType::Success => "✓",
                            ToastType::Error => "✕",
                            ToastType::Info => "ℹ",
                        };
                        view! {
                            <div class=class>
                                <span class="toast__icon">{icon}</span>
                                <span class="toast__message">{toast.message}</span>
                                <button
                                    class="toast__close"
                                    on:click=move |_| dismiss_toast(id)
                                >
                                    "×"
                                </button>
                            </div>
                        }
                    })
                    .collect::<Vec<_>>()
            }}
        </div>
    }
}
