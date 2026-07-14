use crate::server_fns::{
    get_app_config, update_app_config, ConfigEntry, FETCH_SEEDING_SIZE_CONFIG_KEY,
};
use crate::components::toast::{show_toast, ToastType};
use leptos::prelude::*;

/// Known settings with human-readable labels and optional masking.
const KNOWN_KEYS: &[(&str, &str, bool)] = &[
    ("jackett_url", "Jackett URL", false),
    ("jackett_api_key", "Jackett API Key", true),
    ("session_ttl_hours", "会话有效期（小时）", false),
    (
        FETCH_SEEDING_SIZE_CONFIG_KEY,
        "获取做种大小（额外请求）",
        false,
    ),
    ("log_dir", "日志目录", false),
    ("log_retention_days", "日志保留天数", false),
    ("log_min_level", "最低日志级别", false),
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
                <h1>"设置"</h1>
            </div>

            <Suspense fallback=move || {
                view! { <p>"正在加载设置..."</p> }
            }>
                {move || {
                    config
                        .get()
                        .map(|result| {
                            match result {
                                Err(e) => {
                                    view! {
                                        <div class="load-error">
                                            <span>{format!("设置加载失败：{e}")}</span>
                                            <button
                                                class="btn btn--sm btn--outline"
                                                on:click=move |_| refetch()
                                            >
                                                "重试"
                                            </button>
                                        </div>
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
            <h2>"应用配置"</h2>
            <div class="table-wrap">
                <table class="stats-table">
                    <thead>
                        <tr>
                            <th>"设置项"</th>
                            <th>"值"</th>
                            <th>"更新时间"</th>
                            <th>"操作"</th>
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
    let (save_result, set_save_result) = signal(None::<Result<(), String>>);

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
                    {if key == FETCH_SEEDING_SIZE_CONFIG_KEY {
                        view! {
                            <label style="display: inline-flex; align-items: center; gap: 8px;">
                                <input
                                    type="checkbox"
                                    prop:checked=move || value.get() == "true"
                                    on:change=move |ev| {
                                        set_value.set(if event_target_checked(&ev) {
                                            "true".to_string()
                                        } else {
                                            "false".to_string()
                                        });
                                        set_save_result.set(None);
                                    }
                                />
                                <span>{move || if value.get() == "true" { "已开启" } else { "已关闭" }}</span>
                            </label>
                        }
                            .into_any()
                    } else {
                        view! {
                            <input
                                type=move || if revealed.get() { "text" } else { "password" }
                                class="input"
                                prop:value=move || value.get()
                                on:input=move |ev| {
                                    set_value.set(event_target_value(&ev));
                                    set_save_result.set(None);
                                }
                            />
                        }
                            .into_any()
                    }}
                    {if is_secret {
                        Some(
                            view! {
                                <button
                                    class="btn btn--gray btn--sm"
                                    on:click=move |_| set_revealed.update(|r| *r = !*r)
                                >
                                    {move || if revealed.get() { "隐藏" } else { "显示" }}
                                </button>
                            },
                        )
                    } else {
                        None
                    }}
                </div>
                {if key == FETCH_SEEDING_SIZE_CONFIG_KEY {
                    Some(view! {
                        <div class="text-muted" style="font-size: 12px; margin-top: 4px;">
                            "开启后，NexusPHP 用户信息刷新会额外请求一次当前做种列表。"
                        </div>
                    })
                } else {
                    None
                }}
                {move || {
                    save_result.get().map(|r| match r {
                        Ok(()) => view! {
                            <span class="text-green" style="font-size: 12px;">"已保存"</span>
                        }.into_any(),
                        Err(msg) => view! {
                            <span class="text-red" style="font-size: 12px;">{msg}</span>
                        }.into_any(),
                    })
                }}
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
                            set_save_result.set(None);
                            leptos::task::spawn_local(async move {
                                match update_app_config(k, v).await {
                                    Ok(_) => {
                                        show_toast("设置已保存", ToastType::Success);
                                        set_save_result.set(Some(Ok(())));
                                        on_saved();
                                    }
                                    Err(e) => {
                                        show_toast(format!("保存失败：{e}"), ToastType::Error);
                                        set_save_result.set(Some(Err(format!("保存失败：{e}"))));
                                    }
                                }
                                set_saving.set(false);
                            });
                        }
                    }
                >
                    {move || if saving.get() { "保存中..." } else { "保存" }}
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
    let (add_error, set_add_error) = signal(None::<String>);
    let (add_success, set_add_success) = signal(false);

    view! {
        <div class="add-setting-form">
            <h3>"添加设置项"</h3>
            {move || {
                add_error.get().map(|msg| view! {
                    <div class="form-alert form-alert--error">{msg}</div>
                })
            }}
            {move || {
                if add_success.get() {
                    Some(view! {
                        <div class="form-alert form-alert--success">"设置项已添加"</div>
                    })
                } else {
                    None
                }
            }}
            <div class="form-row">
                <input
                    type="text"
                    placeholder="设置项名称"
                    class="input"
                    prop:value=move || new_key.get()
                    on:input=move |ev| {
                        set_new_key.set(event_target_value(&ev));
                        set_add_error.set(None);
                        set_add_success.set(false);
                    }
                />
                <input
                    type="text"
                    placeholder="值"
                    class="input"
                    prop:value=move || new_value.get()
                    on:input=move |ev| {
                        set_new_value.set(event_target_value(&ev));
                        set_add_error.set(None);
                        set_add_success.set(false);
                    }
                />
                <button
                    class="btn btn--green"
                    disabled=move || saving.get() || new_key.get().trim().is_empty()
                    on:click=move |_| {
                        let k = new_key.get();
                        let v = new_value.get();
                        if k.trim().is_empty() {
                            set_add_error.set(Some("设置项名称不能为空".into()));
                            return;
                        }
                        set_saving.set(true);
                        set_add_error.set(None);
                        set_add_success.set(false);
                        leptos::task::spawn_local(async move {
                            match update_app_config(k, v).await {
                                Ok(_) => {
                                    show_toast("设置项已添加", ToastType::Success);
                                    set_add_success.set(true);
                                    set_new_key.set(String::new());
                                    set_new_value.set(String::new());
                                    on_saved();
                                }
                                Err(e) => {
                                    show_toast(format!("添加失败：{e}"), ToastType::Error);
                                    set_add_error.set(Some(format!("添加失败：{e}")));
                                }
                            }
                            set_saving.set(false);
                        });
                    }
                >
                    {move || if saving.get() { "添加中..." } else { "添加" }}
                </button>
            </div>
        </div>
    }
}
