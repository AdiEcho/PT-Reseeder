use leptos::prelude::*;

/// Available color schemes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Theme {
    Dark,
    Light,
}

impl Theme {
    #[cfg(target_arch = "wasm32")]
    fn as_str(self) -> &'static str {
        match self {
            Theme::Dark => "dark",
            Theme::Light => "light",
        }
    }

    fn icon(self) -> &'static str {
        // Show the icon of the mode you'll switch *to*.
        match self {
            Theme::Dark => "☀",
            Theme::Light => "☾",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Theme::Dark => "切换到浅色模式",
            Theme::Light => "切换到深色模式",
        }
    }

    fn toggle(self) -> Theme {
        match self {
            Theme::Dark => Theme::Light,
            Theme::Light => Theme::Dark,
        }
    }
}

#[cfg(target_arch = "wasm32")]
const STORAGE_KEY: &str = "pt-reseeder-theme";

/// Use a deterministic first render so SSR and hydration produce matching HTML.
fn initial_theme() -> Theme {
    Theme::Dark
}

/// Read the persisted theme from localStorage. Falls back to `Dark` when the
/// value is missing or when localStorage is unavailable.
#[cfg(target_arch = "wasm32")]
fn read_stored_theme() -> Theme {
    let Some(window) = web_sys::window() else {
        return Theme::Dark;
    };
    let Some(storage) = window.local_storage().ok().flatten() else {
        return Theme::Dark;
    };
    match storage.get_item(STORAGE_KEY).ok().flatten().as_deref() {
        Some("light") => Theme::Light,
        _ => Theme::Dark,
    }
}

/// Persist the theme and apply it to `<html data-theme="...">`.
#[cfg(target_arch = "wasm32")]
fn apply_theme(theme: Theme) {
    let Some(window) = web_sys::window() else {
        return;
    };
    if let Some(storage) = window.local_storage().ok().flatten() {
        let _ = storage.set_item(STORAGE_KEY, theme.as_str());
    }
    let Some(document) = window.document() else {
        return;
    };
    if let Some(html) = document.document_element() {
        let _ = html.set_attribute("data-theme", theme.as_str());
    }
}

/// SSR cannot access the DOM or localStorage.
#[cfg(not(target_arch = "wasm32"))]
fn apply_theme(_theme: Theme) {}

#[cfg(target_arch = "wasm32")]
fn sync_stored_theme(theme: RwSignal<Theme>) {
    Effect::new(move |_| {
        let stored = read_stored_theme();
        theme.set(stored);
        apply_theme(stored);
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn sync_stored_theme(_theme: RwSignal<Theme>) {}

/// A compact icon button that toggles between dark and light mode.
#[component]
pub fn ThemeToggle() -> impl IntoView {
    let initial = initial_theme();
    let theme = RwSignal::new(initial);
    sync_stored_theme(theme);

    view! {
        <button
            type="button"
            class="theme-toggle"
            title=move || theme.get().label()
            aria-label=move || theme.get().label()
            on:click=move |_| {
                let next = theme.get().toggle();
                theme.set(next);
                apply_theme(next);
            }
        >
            {move || theme.get().icon()}
        </button>
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn ssr_uses_dark_theme_without_browser_api() {
        assert_eq!(initial_theme(), Theme::Dark);
        apply_theme(Theme::Light);
    }
}
