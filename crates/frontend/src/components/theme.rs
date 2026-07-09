use leptos::prelude::*;
use std::sync::OnceLock;

/// Available color schemes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Theme {
    Dark,
    Light,
}

impl Theme {
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
            Theme::Dark => "切换到白天模式",
            Theme::Light => "切换到暗黑模式",
        }
    }

    fn toggle(self) -> Theme {
        match self {
            Theme::Dark => Theme::Light,
            Theme::Light => Theme::Dark,
        }
    }
}

const STORAGE_KEY: &str = "pt-reseeder-theme";

/// Read the persisted theme from localStorage. Falls back to `Dark` when the
/// value is missing or when localStorage is unavailable (SSR, sandboxed).
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

/// Global theme signal. Initialized lazily from localStorage (default dark),
/// then kept in sync with `data-theme` on `<html>`.
fn theme_signal() -> RwSignal<Theme> {
    static THEME: OnceLock<RwSignal<Theme>> = OnceLock::new();
    THEME
        .get_or_init(|| {
            let initial = read_stored_theme();
            // Apply the initial theme on the client so SSR (which always emits
            // dark) is reconciled with the user's stored preference on hydrate.
            apply_theme(initial);
            RwSignal::new(initial)
        })
        .clone()
}

/// A compact icon button that toggles between dark and light mode.
#[component]
pub fn ThemeToggle() -> impl IntoView {
    let theme = theme_signal();

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
