use leptos::prelude::*;

#[component]
pub fn EmptyState(
    icon: &'static str,
    message: &'static str,
    #[prop(optional)] children: Option<Children>,
) -> impl IntoView {
    view! {
        <div class="empty-state">
            <div class="empty-state__icon">{icon}</div>
            <p class="empty-state__message">{message}</p>
            {children.map(|c| view! { <div class="empty-state__action">{c()}</div> })}
        </div>
    }
}
