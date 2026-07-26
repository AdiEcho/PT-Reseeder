use leptos::prelude::*;

/// Renders the states of an async `Resource` load: pending, error (with retry),
/// and loaded.
///
/// Every server function in this crate returns `Result<T, ServerFnError>`, so
/// the error type is fixed rather than generic. Empty-state handling stays with
/// the caller, because "empty" is shaped differently per payload (`Vec::is_empty`,
/// `Option::is_none`, a zero count inside a struct) and the caller already needs
/// a branch to pick the icon and message.
#[component]
pub fn AsyncView<T, IV>(
    /// The resource whose state drives the rendering.
    resource: Resource<Result<T, ServerFnError>>,
    /// Prefix for the error message: `"站点"` renders as `"站点加载失败：{e}"`.
    error_label: &'static str,
    /// Invoked when the user clicks retry in the error state.
    on_retry: impl Fn() + Clone + Send + Sync + 'static,
    /// Renders the loaded value. Named `render` rather than `children` because
    /// Leptos reserves `children` for the element-child position, which cannot
    /// carry a closure that takes an argument.
    render: impl Fn(T) -> IV + Send + Sync + 'static,
) -> impl IntoView
where
    T: Clone + Send + Sync + 'static,
    IV: IntoView + 'static,
{
    view! {
        <Suspense fallback=move || {
            view! { <p class="load-pending">"加载中…"</p> }
        }>
            {move || {
                resource
                    .get()
                    .map(|result| match result {
                        Err(e) => {
                            let on_retry = on_retry.clone();
                            view! {
                                <div class="load-error">
                                    <span>{format!("{error_label}加载失败：{e}")}</span>
                                    <button
                                        class="btn btn--sm btn--outline"
                                        on:click=move |_| on_retry()
                                    >
                                        "重试"
                                    </button>
                                </div>
                            }
                                .into_any()
                        }
                        Ok(value) => render(value).into_any(),
                    })
            }}
        </Suspense>
    }
}
