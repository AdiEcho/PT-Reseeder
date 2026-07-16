use leptos::prelude::*;

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

#[derive(Clone, Copy)]
struct ToastState {
    toasts: RwSignal<Vec<Toast>>,
    counter: RwSignal<u64>,
}

impl ToastState {
    fn new() -> Self {
        Self {
            toasts: RwSignal::new(Vec::new()),
            counter: RwSignal::new(0),
        }
    }
}

pub fn provide_toast_context() {
    provide_context(ToastState::new());
}

fn toast_state() -> Option<ToastState> {
    use_context::<ToastState>()
}

pub fn show_toast(message: impl Into<String>, toast_type: ToastType) {
    let Some(state) = toast_state() else {
        return;
    };
    let id = state.counter.get_untracked() + 1;
    state.counter.set(id);

    let toast = Toast {
        id,
        message: message.into(),
        toast_type,
    };

    state.toasts.update(|toasts| {
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
    let Some(state) = toast_state() else {
        return;
    };
    state.toasts.update(|toasts| {
        toasts.retain(|t| t.id != id);
    });
}

/// Mount this once at the top level (e.g. in App or AppLayout).
#[component]
pub fn ToastContainer() -> impl IntoView {
    let toasts = toast_state()
        .expect("toast context should be provided by App")
        .toasts;

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

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;

    #[test]
    fn toast_state_is_scoped_to_each_reactive_owner() {
        for _ in 0..2 {
            let owner = Owner::new();
            owner.with(|| {
                provide_toast_context();
                show_toast("test", ToastType::Info);

                let state = toast_state().expect("toast context should exist");
                assert_eq!(state.toasts.get_untracked().len(), 1);
                assert_eq!(state.counter.get_untracked(), 1);
            });
        }
    }
}
