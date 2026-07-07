use crate::server_fns::{get_app_config, update_app_config, ConfigEntry};
use leptos::prelude::*;

/// Known settings with human-readable labels and optional masking.
const KNOWN_KEYS: &[(&str, &str, bool)] = &[
    ("jackett_url", "Jackett URL", false),
    ("jackett_api_key", "Jackett API Key", true),
    ("session_ttl_hours", "Session TTL (hours)", false),
];

fn label_for_key(key: &str) -> String {
    KNOWN_KEYS
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, label, _)| label.to_string())
        .unwrap_or_else(|| key.to_string())
}

fn is_secret_key(key: &str) -> bool {
    KNOWN_KEYS
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, _, secret)| *secret)
        .unwrap_or(false)
}

#[component]
pub fn SettingsPage() -> impl IntoView {
    let (version, set_version) = signal(0u32);

    let config = Resource::new(move || version.get(), |_| get_app_config());

    let refetch = move || set_version.update(|v| *v += 1);

    view! {
        <div class="dashboard">
            <div class="dashboard-header">
                <h1>"Settings"</h1>
            </div>

            <Suspense fallback=move || {
                view! { <p>"Loading settings..."</p> }
            }>
                {move || {
                    config
                        .get()
                        .map(|result| {
                            match result {
                                Err(e) => {
                                    view! {
                                        <p class="error">
                                            {format!("Failed to load settings: {e}")}
                                        </p>
                                    }
                                        .into_any()
                                }
                                Ok(entries) => {
                                    view! { <SettingsTable entries=entries on_saved=refetch /> }
                                        .into_any()
                                }
                            }
                        })
                }}
            </Suspense>
        </div>
    }
}

#[component]
fn SettingsTable<F>(entries: Vec<ConfigEntry>, on_saved: F) -> impl IntoView
where
    F: Fn() + Copy + 'static,
{
    view! {
        <div class="stats-table-section">
            <h2>"Application Configuration"</h2>
            <div class="table-wrap">
                <table class="stats-table">
                    <thead>
                        <tr>
                            <th>"Setting"</th>
                            <th>"Value"</th>
                            <th>"Updated"</th>
                            <th>"Actions"</th>
                        </tr>
                    </thead>
                    <tbody>
                        {entries
                            .into_iter()
                            .map(|entry| {
                                view! { <SettingRow entry=entry on_saved=on_saved /> }
                            })
                            .collect::<Vec<_>>()}
                    </tbody>
                </table>
            </div>

            <AddSettingForm on_saved=on_saved />
        </div>
    }
}

#[component]
fn SettingRow<F>(entry: ConfigEntry, on_saved: F) -> impl IntoView
where
    F: Fn() + Copy + 'static,
{
    let key = entry.key.clone();
    let label = label_for_key(&key);
    let is_secret = is_secret_key(&key);

    let (value, set_value) = signal(entry.value.clone());
    let (revealed, set_revealed) = signal(!is_secret);
    let (saving, set_saving) = signal(false);

    let updated_display = if entry.updated_at.len() >= 16 {
        entry.updated_at[..16].to_string()
    } else {
        entry.updated_at.clone()
    };

    let save_key = key.clone();

    view! {
        <tr>
            <td>
                <strong>{label}</strong>
                <br />
                <span class="text-muted" style="font-size: 0.85em;">
                    {key.clone()}
                </span>
            </td>
            <td>
                <div class="setting-value">
                    <input
                        type=move || if revealed.get() { "text" } else { "password" }
                        class="input"
                        prop:value=move || value.get()
                        on:input=move |ev| set_value.set(event_target_value(&ev))
                    />
                    {if is_secret {
                        Some(
                            view! {
                                <button
                                    class="btn btn--gray btn--sm"
                                    on:click=move |_| set_revealed.update(|r| *r = !*r)
                                >
                                    {move || if revealed.get() { "Hide" } else { "Show" }}
                                </button>
                            },
                        )
                    } else {
                        None
                    }}
                </div>
            </td>
            <td class="text-muted">{updated_display}</td>
            <td>
                <button
                    class="btn btn--green btn--sm"
                    disabled=move || saving.get()
                    on:click={
                        let save_key = save_key.clone();
                        move |_| {
                            let k = save_key.clone();
                            let v = value.get();
                            set_saving.set(true);
                            leptos::task::spawn_local(async move {
                                let _ = update_app_config(k, v).await;
                                set_saving.set(false);
                                on_saved();
                            });
                        }
                    }
                >
                    {move || if saving.get() { "Saving..." } else { "Save" }}
                </button>
            </td>
        </tr>
    }
}

#[component]
fn AddSettingForm<F>(on_saved: F) -> impl IntoView
where
    F: Fn() + Copy + 'static,
{
    let (new_key, set_new_key) = signal(String::new());
    let (new_value, set_new_value) = signal(String::new());
    let (saving, set_saving) = signal(false);

    view! {
        <div class="add-setting-form">
            <h3>"Add Setting"</h3>
            <div class="form-row">
                <input
                    type="text"
                    placeholder="Key"
                    class="input"
                    prop:value=move || new_key.get()
                    on:input=move |ev| set_new_key.set(event_target_value(&ev))
                />
                <input
                    type="text"
                    placeholder="Value"
                    class="input"
                    prop:value=move || new_value.get()
                    on:input=move |ev| set_new_value.set(event_target_value(&ev))
                />
                <button
                    class="btn btn--green"
                    disabled=move || saving.get() || new_key.get().is_empty()
                    on:click=move |_| {
                        let k = new_key.get();
                        let v = new_value.get();
                        set_saving.set(true);
                        leptos::task::spawn_local(async move {
                            let _ = update_app_config(k, v).await;
                            set_saving.set(false);
                            set_new_key.set(String::new());
                            set_new_value.set(String::new());
                            on_saved();
                        });
                    }
                >
                    {move || if saving.get() { "Adding..." } else { "Add" }}
                </button>
            </div>
        </div>
    }
}
