use crate::components::confirm_modal::ConfirmModal;
use crate::components::empty_state::EmptyState;
use crate::components::toast::{show_toast, ToastType};
use leptos::prelude::*;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct ProbeDetail {
    api_reachable: Option<ProbeFieldDetail>,
    #[serde(default)]
    user_info_fields: Vec<ProbeFieldDetail>,
    #[serde(default)]
    passkey_error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProbeFieldDetail {
    field_name: String,
    success: bool,
    #[serde(default)]
    value_preview: Option<String>,
    error: Option<String>,
}

fn probe_field_label(field_name: &str) -> &str {
    match field_name {
        "api_reachable" => "辅种 API",
        "uploaded" => "上传量",
        "downloaded" => "下载量",
        "ratio" => "分享率",
        "bonus" => "积分/魔力值",
        "user_class" => "用户等级",
        "seeding_count" => "做种数",
        "leeching_count" => "下载中数量",
        "seeding_size" => "做种体积",
        "upload_time_seconds" => "做种时间",
        _ => field_name,
    }
}

fn probe_error_label(error: Option<&str>) -> String {
    match error {
        None | Some("field not parsed") => "未获取到（站点可能不支持或页面结构已变化）".to_string(),
        Some(error) => error.to_string(),
    }
}

fn probe_success_details(detail_json: Option<&str>) -> Vec<(String, String)> {
    let Some(detail_json) = detail_json else {
        return Vec::new();
    };
    let Ok(detail) = serde_json::from_str::<ProbeDetail>(detail_json) else {
        return Vec::new();
    };

    detail
        .user_info_fields
        .into_iter()
        .filter(|field| field.success)
        .filter_map(|field| {
            field
                .value_preview
                .map(|v| (probe_field_label(&field.field_name).to_string(), v))
        })
        .collect()
}

fn probe_failure_details(detail_json: Option<&str>) -> Vec<String> {
    let Some(detail_json) = detail_json else {
        return Vec::new();
    };
    let Ok(detail) = serde_json::from_str::<ProbeDetail>(detail_json) else {
        return Vec::new();
    };

    let mut failures = Vec::new();
    if let Some(field) = detail.api_reachable.filter(|field| !field.success) {
        failures.push(format!(
            "{}：{}",
            probe_field_label(&field.field_name),
            probe_error_label(field.error.as_deref())
        ));
    }
    failures.extend(
        detail
            .user_info_fields
            .into_iter()
            .filter(|field| !field.success)
            .map(|field| {
                format!(
                    "{}：{}",
                    probe_field_label(&field.field_name),
                    probe_error_label(field.error.as_deref())
                )
            }),
    );
    if let Some(error) = detail.passkey_error {
        failures.push(format!(
            "Passkey：{}",
            probe_error_label(Some(error.as_str()))
        ));
    }
    failures
}

#[component]
pub fn SitesPage() -> impl IntoView {
    let (show_form, set_show_form) = signal(false);
    let (confirm_delete_id, set_confirm_delete_id) = signal(None::<i64>);
    let (edit_url_site, set_edit_url_site) = signal(None::<(i64, String, String)>);

    // Form field signals
    let (selected_preset, set_selected_preset) = signal(String::new());
    let (name, set_name) = signal(String::new());
    let (url, set_url) = signal(String::new());
    let (api_url, set_api_url) = signal(String::new());
    let (adapter_type, set_adapter_type) = signal("NexusPHP".to_string());
    let (auth_type, set_auth_type) = signal("cookie".to_string());
    let (cookie, set_cookie) = signal(String::new());
    let (passkey, set_passkey) = signal(String::new());
    let (is_custom, set_is_custom) = signal(true);

    // Edit site form signals
    let (edit_url, set_edit_url) = signal(String::new());
    let (edit_api_url, set_edit_api_url) = signal(String::new());
    let (edit_cookie, set_edit_cookie) = signal(String::new());
    let (edit_passkey, set_edit_passkey) = signal(String::new());

    // Load sites list
    let sites = Resource::new(|| (), |_| crate::server_fns::get_sites());

    // Load site definitions for preset selector
    let definitions = Resource::new(|| (), |_| crate::server_fns::get_site_definitions());

    // Create site action
    let create_action = Action::new(move |_: &()| {
        let n = name.get_untracked();
        let u = url.get_untracked();
        let au = api_url.get_untracked();
        let at = adapter_type.get_untracked();
        let aht = auth_type.get_untracked();
        let c = cookie.get_untracked();
        let p = passkey.get_untracked();
        async move { crate::server_fns::create_site(n, u, au, at, aht, c, p).await }
    });

    // Validate site action
    let validate_action = Action::new(move |_: &()| {
        // 预设模式下用 site id（selected_preset），自定义模式下用用户输入的 name
        let n = if is_custom.get_untracked() {
            name.get_untracked()
        } else {
            selected_preset.get_untracked()
        };
        let u = url.get_untracked();
        let au = api_url.get_untracked();
        let at = adapter_type.get_untracked();
        let c = cookie.get_untracked();
        let p = passkey.get_untracked();
        async move { crate::server_fns::validate_site(n, u, au, at, c, p).await }
    });

    // Delete site action
    let delete_action = Action::new(move |id: &i64| {
        let id = *id;
        async move { crate::server_fns::delete_site(id).await }
    });

    // Probe site action
    let probe_action = Action::new(move |id: &i64| {
        let id = *id;
        async move { crate::server_fns::probe_site(id).await }
    });

    // Update site action
    let update_site_action = Action::new(move |args: &(i64, String, String, String, String)| {
        let (id, u, au, c, p) = args.clone();
        async move { crate::server_fns::update_site(id, u, au, c, p).await }
    });

    // Refetch sites after create/delete/probe/update
    Effect::new(move |_| {
        if let Some(result) = create_action.value().get() {
            match result {
                Ok(_) => {
                    show_toast("站点创建成功", ToastType::Success);
                    sites.refetch();
                    // Reset form
                    set_selected_preset.set(String::new());
                    set_name.set(String::new());
                    set_url.set(String::new());
                    set_api_url.set(String::new());
                    set_adapter_type.set("NexusPHP".to_string());
                    set_auth_type.set("cookie".to_string());
                    set_cookie.set(String::new());
                    set_passkey.set(String::new());
                    set_is_custom.set(true);
                    set_show_form.set(false);
                }
                Err(e) => show_toast(format!("创建失败：{e}"), ToastType::Error),
            }
        }
    });

    Effect::new(move |_| {
        if let Some(result) = delete_action.value().get() {
            match result {
                Ok(_) => {
                    show_toast("站点已删除", ToastType::Success);
                    sites.refetch();
                }
                Err(e) => show_toast(format!("删除失败：{e}"), ToastType::Error),
            }
        }
    });

    Effect::new(move |_| {
        if let Some(result) = probe_action.value().get() {
            match result {
                Ok(_) => {
                    show_toast("连通测试完成", ToastType::Info);
                    sites.refetch();
                }
                Err(e) => show_toast(format!("连通测试失败：{e}"), ToastType::Error),
            }
        }
    });

    Effect::new(move |_| {
        if let Some(result) = update_site_action.value().get() {
            match result {
                Ok(_) => {
                    show_toast("站点已更新", ToastType::Success);
                    sites.refetch();
                    set_edit_url_site.set(None);
                }
                Err(e) => show_toast(format!("更新失败：{e}"), ToastType::Error),
            }
        }
    });

    view! {
        <div class="dashboard">
            <div class="dashboard-header">
                <h1>"站点管理"</h1>
                <button
                    class="btn btn-primary"
                    on:click=move |_| set_show_form.update(|v| *v = !*v)
                >
                    {move || if show_form.get() { "取消" } else { "添加站点" }}
                </button>
            </div>

            // Add Site form
            {move || {
                if show_form.get() {
                    view! {
                        <div class="form-section">
                            <h2>"添加新站点"</h2>
                            <div class="form-grid">
                                // Preset selector
                                <div class="form-group" style="grid-column: 1 / -1;">
                                    <label>"选择站点预设"</label>
                                    <Suspense fallback=move || view! { <select disabled=true><option>"加载中..."</option></select> }>
                                        {move || {
                                            definitions.get().map(|result| match result {
                                                Ok(defs) => {
                                                    let defs_for_change = defs.clone();
                                                    view! {
                                                        <select
                                                            prop:value=move || selected_preset.get()
                                                            on:change=move |ev| {
                                                                let val = event_target_value(&ev);
                                                                set_selected_preset.set(val.clone());
                                                                if val == "__custom__" || val.is_empty() {
                                                                    set_is_custom.set(true);
                                                                    if val == "__custom__" {
                                                                        set_name.set(String::new());
                                                                        set_url.set(String::new());
                                                                        set_api_url.set(String::new());
                                                                        set_adapter_type.set("NexusPHP".to_string());
                                                                    }
                                                                } else if let Some(def) = defs_for_change.iter().find(|d| d.id == val) {
                                                                    set_is_custom.set(false);
                                                                    set_name.set(def.name.clone());
                                                                    set_url.set(def.url.clone());
                                                                    set_api_url.set(def.api_url.clone().unwrap_or_default());
                                                                    // Capitalize adapter for display
                                                                    let adapter_display = match def.adapter.as_str() {
                                                                        "nexusphp" => "NexusPHP",
                                                                        "unit3d" => "Unit3D",
                                                                        "gazelle" => "Gazelle",
                                                                        "zhuque" => "Zhuque",
                                                                        "mteam" => "MTeam",
                                                                        other => other,
                                                                    };
                                                                    set_adapter_type.set(adapter_display.to_string());
                                                                }
                                                            }
                                                        >
                                                            <option value="">"-- 请选择站点 --"</option>
                                                            {defs.into_iter().map(|def| {
                                                                let id = def.id.clone();
                                                                let label = format!("{} ({})", def.name, def.adapter);
                                                                view! {
                                                                    <option value=id>{label}</option>
                                                                }
                                                            }).collect::<Vec<_>>()}
                                                            <option value="__custom__">"🔧 自定义站点"</option>
                                                        </select>
                                                    }
                                                    .into_any()
                                                }
                                                Err(e) => {
                                                    view! {
                                                        <p class="error">{format!("预设加载失败：{e}")}</p>
                                                    }
                                                    .into_any()
                                                }
                                            })
                                        }}
                                    </Suspense>
                                </div>
                                <div class="form-group">
                                    <label>"名称" <span class="required">"*"</span></label>
                                    <input
                                        type="text"
                                        placeholder="站点名称"
                                        prop:value=move || name.get()
                                        disabled=move || !is_custom.get()
                                        on:input=move |ev| {
                                            set_name.set(event_target_value(&ev))
                                        }
                                    />
                                </div>
                                <div class="form-group">
                                    <label>"URL" <span class="required">"*"</span></label>
                                    <input
                                        type="text"
                                        placeholder="https://example.com"
                                        prop:value=move || url.get()
                                        on:input=move |ev| {
                                            set_url.set(event_target_value(&ev))
                                        }
                                    />
                                </div>
                                <div class="form-group">
                                    <label>"API URL"</label>
                                    <input
                                        type="text"
                                        placeholder="https://example.com/api"
                                        prop:value=move || api_url.get()
                                        on:input=move |ev| {
                                            set_api_url.set(event_target_value(&ev))
                                        }
                                    />
                                </div>
                                <div class="form-group">
                                    <label>"站点架构"</label>
                                    <select
                                        prop:value=move || adapter_type.get()
                                        disabled=move || !is_custom.get()
                                        on:change=move |ev| {
                                            set_adapter_type.set(event_target_value(&ev))
                                        }
                                    >
                                        <option value="NexusPHP">"NexusPHP"</option>
                                        <option value="Unit3D">"Unit3D"</option>
                                        <option value="Gazelle">"Gazelle"</option>
                                        <option value="MTeam">"MTeam"</option>
                                        <option value="Zhuque">"Zhuque"</option>
                                    </select>
                                </div>
                                <div class="form-group">
                                    <label>"认证方式"</label>
                                    <select
                                        prop:value=move || auth_type.get()
                                        on:change=move |ev| {
                                            set_auth_type.set(event_target_value(&ev))
                                        }
                                    >
                                        <option value="cookie">"Cookie"</option>
                                        <option value="passkey">"Passkey"</option>
                                        <option value="apikey">"API Key"</option>
                                    </select>
                                </div>
                                <div class="form-group">
                                    <label>"Cookie"</label>
                                    <input
                                        type="text"
                                        placeholder="会话 Cookie"
                                        prop:value=move || cookie.get()
                                        on:input=move |ev| {
                                            set_cookie.set(event_target_value(&ev))
                                        }
                                    />
                                </div>
                                <div class="form-group">
                                    <label>"Passkey"</label>
                                    <input
                                        type="text"
                                        placeholder="Passkey"
                                        prop:value=move || passkey.get()
                                        on:input=move |ev| {
                                            set_passkey.set(event_target_value(&ev))
                                        }
                                    />
                                </div>
                            </div>
                            <div class="form-actions">
                                <button
                                    class="btn btn-primary"
                                    disabled=move || create_action.pending().get()
                                    on:click=move |_| { create_action.dispatch(()); }
                                >
                                    {move || {
                                        if create_action.pending().get() {
                                            "创建中..."
                                        } else {
                                            "创建站点"
                                        }
                                    }}
                                </button>
                                <button
                                    class="btn btn--outline"
                                    disabled=move || validate_action.pending().get()
                                    on:click=move |_| { validate_action.dispatch(()); }
                                >
                                    {move || {
                                        if validate_action.pending().get() {
                                            "校验中..."
                                        } else {
                                            "校验连通"
                                        }
                                    }}
                                </button>
                            </div>
                            // Validate result display
                            {move || {
                                validate_action.value().get().and_then(|r| r.ok()).map(|result| {
                                    let (class, icon) = match result.status.as_str() {
                                        "ok" => ("form-alert form-alert--success", "✅ "),
                                        "partial" => ("form-alert form-alert--warning", "⚠️ "),
                                        _ => ("form-alert form-alert--error", "❌ "),
                                    };
                                    let successes = probe_success_details(result.detail_json.as_deref());
                                    let failures = probe_failure_details(result.detail_json.as_deref());
                                    view! {
                                        <div class=class>
                                            <div>{format!("{}{}", icon, result.message)}</div>
                                            {(!successes.is_empty()).then(|| view! {
                                                <div class="probe-success-section">
                                                    <div class="probe-section-title">"获取到的个人信息："</div>
                                                    <ul class="probe-success-list">
                                                        {successes
                                                            .into_iter()
                                                            .map(|(label, value)| view! {
                                                                <li>
                                                                    <span class="probe-field-label">{label}</span>
                                                                    "："
                                                                    <span class="probe-field-value">{value}</span>
                                                                </li>
                                                            })
                                                            .collect::<Vec<_>>()}
                                                    </ul>
                                                </div>
                                            })}
                                            {(!failures.is_empty()).then(|| view! {
                                                <div class="probe-failure-section">
                                                    <div class="probe-section-title">"未获取到的项目："</div>
                                                    <ul class="probe-failure-list">
                                                        {failures
                                                            .into_iter()
                                                            .map(|failure| view! { <li>{failure}</li> })
                                                            .collect::<Vec<_>>()}
                                                    </ul>
                                                </div>
                                            })}
                                        </div>
                                    }
                                })
                            }}
                        </div>
                    }
                        .into_any()
                } else {
                    view! { <div></div> }.into_any()
                }
            }}

            // Error display for actions
            {move || {
                create_action
                    .value()
                    .get()
                    .and_then(|r| r.err())
                    .map(|e| {
                        view! { <p class="error">{format!("创建失败：{e}")}</p> }
                    })
            }}
            {move || {
                delete_action
                    .value()
                    .get()
                    .and_then(|r| r.err())
                    .map(|e| {
                        view! { <p class="error">{format!("删除失败：{e}")}</p> }
                    })
            }}
            {move || {
                validate_action
                    .value()
                    .get()
                    .and_then(|r| r.err())
                    .map(|e| {
                        view! { <p class="error">{format!("校验失败：{e}")}</p> }
                    })
            }}
            {move || {
                probe_action
                    .value()
                    .get()
                    .and_then(|r| r.err())
                    .map(|e| {
                        view! { <p class="error">{format!("连通测试失败：{e}")}</p> }
                    })
            }}
            {move || {
                probe_action
                    .value()
                    .get()
                    .and_then(|r| r.ok())
                    .map(|result| {
                        let (class, icon) = match result.status.as_str() {
                            "ok" => ("form-alert form-alert--success", "✅ "),
                            "partial" => ("form-alert form-alert--warning", "⚠️ "),
                            _ => ("form-alert form-alert--error", "❌ "),
                        };
                        let successes = probe_success_details(result.detail_json.as_deref());
                        let failures = probe_failure_details(result.detail_json.as_deref());
                        view! {
                            <div class=class>
                                <div>{format!("{}{}", icon, result.message)}</div>
                                {(!successes.is_empty()).then(|| view! {
                                    <div class="probe-success-section">
                                        <div class="probe-section-title">"获取到的个人信息："</div>
                                        <ul class="probe-success-list">
                                            {successes
                                                .into_iter()
                                                .map(|(label, value)| view! {
                                                    <li>
                                                        <span class="probe-field-label">{label}</span>
                                                        "："
                                                        <span class="probe-field-value">{value}</span>
                                                    </li>
                                                })
                                                .collect::<Vec<_>>()}
                                        </ul>
                                    </div>
                                })}
                                {(!failures.is_empty()).then(|| view! {
                                    <div class="probe-failure-section">
                                        <div class="probe-section-title">"未获取到的项目："</div>
                                        <ul class="probe-failure-list">
                                            {failures
                                                .into_iter()
                                                .map(|failure| view! { <li>{failure}</li> })
                                                .collect::<Vec<_>>()}
                                        </ul>
                                    </div>
                                })}
                            </div>
                        }
                    })
            }}
            {move || {
                update_site_action
                    .value()
                    .get()
                    .and_then(|r| r.err())
                    .map(|e| {
                        view! { <p class="error">{format!("更新失败：{e}")}</p> }
                    })
            }}
            {move || {
                update_site_action
                    .value()
                    .get()
                    .and_then(|r| r.ok())
                    .map(|_| {
                        view! { <div class="form-alert form-alert--success">"站点已更新"</div> }
                    })
            }}

            // Sites table
            <Suspense fallback=move || view! { <p>"正在加载站点..."</p> }>
                {move || {
                    sites
                        .get()
                        .map(|result| match result {
                            Ok(sites_list) => {
                                if sites_list.is_empty() {
                                    view! {
                                        <div class="stats-table-section">
                                            <EmptyState icon="◈" message="尚未配置任何站点，请在上方添加。" />
                                        </div>
                                    }
                                        .into_any()
                                } else {
                                    view! {
                                        <div class="stats-table-section">
                                            <h2>"站点列表"</h2>
                                            <div class="table-wrap">
                                                <table class="stats-table">
                                                    <thead>
                                                        <tr>
                                                            <th>"名称"</th>
                                                            <th>"URL"</th>
                                                            <th>"架构"</th>
                                                            <th>"连通状态"</th>
                                                            <th>"启用"</th>
                                                            <th>"操作"</th>
                                                        </tr>
                                                    </thead>
                                                    <tbody>
                                                        {sites_list
                                                            .into_iter()
                                                            .map(|site| {
                                                                let site_id = site.id;
                                                                let site_url = site.url.clone();
                                                                let site_api_url = site.api_url.clone().unwrap_or_default();
                                                                let detail_href = format!(
                                                                    "/sites/{}",
                                                                    site.id,
                                                                );
                                                                let (probe_class, probe_label) = match site
                                                                    .probe_status
                                                                    .as_str()
                                                                {
                                                                    "ok" => ("text-green", "正常"),
                                                                    "partial" => ("text-yellow", "部分可用"),
                                                                    "failed" => ("text-red", "失败"),
                                                                    "pending" => ("text-muted", "检测中"),
                                                                    _ => ("text-muted", "未知"),
                                                                };
                                                                view! {
                                                                    <tr>
                                                                        <td>
                                                                            <a href=detail_href>{site.name}</a>
                                                                        </td>
                                                                        <td class="text-muted">{site.url}</td>
                                                                        <td>{site.adapter_type}</td>
                                                                        <td class=probe_class>{probe_label}</td>
                                                                        <td>
                                                                            {if site.enabled {
                                                                                view! { <span class="text-green">"是"</span> }
                                                                                    .into_any()
                                                                            } else {
                                                                                view! { <span class="text-red">"否"</span> }
                                                                                    .into_any()
                                                                            }}
                                                                        </td>
                                                                        <td class="actions-cell">
                                                                            <button
                                                                                class="btn btn--sm btn--outline"
                                                                                on:click=move |_| {
                                                                                    set_edit_url.set(site_url.clone());
                                                                                    set_edit_api_url.set(site_api_url.clone());
                                                                                    set_edit_cookie.set(String::new());
                                                                                    set_edit_passkey.set(String::new());
                                                                                    set_edit_url_site.set(Some((site_id, site_url.clone(), site_api_url.clone())));
                                                                                }
                                                                            >
                                                                                "编辑"
                                                                            </button>
                                                                            <button
                                                                                class="btn btn--sm btn--outline"
                                                                                on:click=move |_| { probe_action.dispatch(site_id); }
                                                                            >
                                                                                "连通测试"
                                                                            </button>
                                                                            <button
                                                                                class="btn btn--sm btn--danger"
                                                                                on:click=move |_| { set_confirm_delete_id.set(Some(site_id)); }
                                                                            >
                                                                                "删除"
                                                                            </button>
                                                                        </td>
                                                                    </tr>
                                                                }
                                                            })
                                                            .collect::<Vec<_>>()}
                                                    </tbody>
                                                </table>
                                            </div>
                                        </div>
                                    }
                                        .into_any()
                                }
                            }
                            Err(e) => {
                                view! {
                                    <div class="load-error">
                                        <span>{format!("站点加载失败：{e}")}</span>
                                        <button
                                            class="btn btn--sm btn--outline"
                                            on:click=move |_| sites.refetch()
                                        >
                                            "重试"
                                        </button>
                                    </div>
                                }
                                    .into_any()
                            }
                        })
                }}
            </Suspense>

            // Edit site dialog (Esc closes via ConfirmModal-style overlay)
            {move || {
                edit_url_site.get().map(|(edit_id, _, _)| {
                    view! {
                        <EditSiteModal
                            edit_id=edit_id
                            edit_url=edit_url
                            set_edit_url=set_edit_url
                            edit_api_url=edit_api_url
                            set_edit_api_url=set_edit_api_url
                            edit_cookie=edit_cookie
                            set_edit_cookie=set_edit_cookie
                            edit_passkey=edit_passkey
                            set_edit_passkey=set_edit_passkey
                            pending=Signal::derive(move || update_site_action.pending().get())
                            on_cancel=move || set_edit_url_site.set(None)
                            on_save=move || {
                                update_site_action.dispatch((
                                    edit_id,
                                    edit_url.get_untracked(),
                                    edit_api_url.get_untracked(),
                                    edit_cookie.get_untracked(),
                                    edit_passkey.get_untracked(),
                                ));
                            }
                        />
                    }
                })
            }}

            // Delete confirmation dialog
            {move || {
                confirm_delete_id.get().map(|del_id| {
                    view! {
                        <ConfirmModal
                            title="确认删除"
                            message="确定要删除该站点吗？此操作不可撤销。".to_string()
                            on_confirm=move || {
                                delete_action.dispatch(del_id);
                                set_confirm_delete_id.set(None);
                            }
                            on_cancel=move || set_confirm_delete_id.set(None)
                            confirm_label="确认删除"
                            danger=true
                        />
                    }
                })
            }}
        </div>
    }
}

#[component]
fn EditSiteModal(
    edit_id: i64,
    edit_url: ReadSignal<String>,
    set_edit_url: WriteSignal<String>,
    edit_api_url: ReadSignal<String>,
    set_edit_api_url: WriteSignal<String>,
    edit_cookie: ReadSignal<String>,
    set_edit_cookie: WriteSignal<String>,
    edit_passkey: ReadSignal<String>,
    set_edit_passkey: WriteSignal<String>,
    pending: Signal<bool>,
    on_cancel: impl Fn() + 'static + Clone,
    on_save: impl Fn() + 'static + Clone,
) -> impl IntoView {
    let _ = edit_id;
    let on_cancel_esc = on_cancel.clone();
    let on_cancel_btn = on_cancel.clone();
    let on_cancel_overlay = on_cancel.clone();
    let overlay_ref = NodeRef::<leptos::html::Div>::new();

    Effect::new(move |_| {
        if let Some(el) = overlay_ref.get() {
            let _ = el.focus();
        }
    });

    let on_keydown = move |e: leptos::ev::KeyboardEvent| {
        if e.key() == "Escape" {
            on_cancel_esc.clone()();
        }
    };

    view! {
        <div
            class="confirm-overlay"
            tabindex="-1"
            node_ref=overlay_ref
            on:keydown=on_keydown
            on:click=move |_| on_cancel_overlay.clone()()
        >
            <div class="confirm-dialog" role="dialog" aria-modal="true" on:click=move |e| e.stop_propagation()>
                <h3>"编辑站点"</h3>
                <div class="form-grid">
                    <div class="form-group">
                        <label>"URL"</label>
                        <input
                            type="text"
                            prop:value=move || edit_url.get()
                            on:input=move |ev| set_edit_url.set(event_target_value(&ev))
                        />
                    </div>
                    <div class="form-group">
                        <label>"API URL"</label>
                        <input
                            type="text"
                            prop:value=move || edit_api_url.get()
                            on:input=move |ev| set_edit_api_url.set(event_target_value(&ev))
                        />
                    </div>
                    <div class="form-group">
                        <label>"Cookie"</label>
                        <input
                            type="text"
                            placeholder="留空则保持不变"
                            prop:value=move || edit_cookie.get()
                            on:input=move |ev| set_edit_cookie.set(event_target_value(&ev))
                        />
                    </div>
                    <div class="form-group">
                        <label>"Passkey"</label>
                        <input
                            type="text"
                            placeholder="留空则保持不变"
                            prop:value=move || edit_passkey.get()
                            on:input=move |ev| set_edit_passkey.set(event_target_value(&ev))
                        />
                    </div>
                </div>
                <div class="form-actions">
                    <button
                        class="btn btn--outline"
                        disabled=move || pending.get()
                        on:click=move |_| on_cancel_btn.clone()()
                    >
                        "取消"
                    </button>
                    <button
                        class="btn btn-primary"
                        disabled=move || pending.get()
                        on:click=move |_| on_save.clone()()
                    >
                        {move || if pending.get() { "保存中..." } else { "保存" }}
                    </button>
                </div>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::{probe_failure_details, probe_success_details};

    #[test]
    fn probe_failure_details_formats_failed_items() {
        let detail = r#"{
            "api_reachable":{"field_name":"api_reachable","success":false,"value_preview":null,"error":"HTTP 403"},
            "user_info_fields":[
                {"field_name":"uploaded","success":true,"value_preview":"1.00 TB","error":null},
                {"field_name":"ratio","success":false,"value_preview":null,"error":"field not parsed"}
            ],
            "passkey_available":false,
            "passkey_error":"authentication failed: cookie expired"
        }"#;

        assert_eq!(
            probe_failure_details(Some(detail)),
            vec![
                "辅种 API：HTTP 403",
                "分享率：未获取到（站点可能不支持或页面结构已变化）",
                "Passkey：authentication failed: cookie expired",
            ]
        );
    }

    #[test]
    fn probe_success_details_extracts_values() {
        let detail = r#"{
            "api_reachable":null,
            "user_info_fields":[
                {"field_name":"uploaded","success":true,"value_preview":"1.00 TB","error":null},
                {"field_name":"downloaded","success":true,"value_preview":"500.00 GB","error":null},
                {"field_name":"ratio","success":false,"value_preview":null,"error":"field not parsed"}
            ]
        }"#;

        assert_eq!(
            probe_success_details(Some(detail)),
            vec![
                ("上传量".to_string(), "1.00 TB".to_string()),
                ("下载量".to_string(), "500.00 GB".to_string()),
            ]
        );
    }

    #[test]
    fn probe_success_details_handles_missing_json() {
        assert!(probe_success_details(None).is_empty());
        assert!(probe_success_details(Some("not-json")).is_empty());
    }

    #[test]
    fn probe_failure_details_handles_missing_or_invalid_json() {
        assert!(probe_failure_details(None).is_empty());
        assert!(probe_failure_details(Some("not-json")).is_empty());
    }

    #[test]
    fn probe_failure_details_ignores_absent_optional_passkey() {
        let detail = r#"{
            "api_reachable":null,
            "user_info_fields":[],
            "passkey_available":false
        }"#;

        assert!(probe_failure_details(Some(detail)).is_empty());
    }
}
