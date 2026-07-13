use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct ProbeDetail {
    api_reachable: Option<ProbeFieldDetail>,
    #[serde(default)]
    user_info_fields: Vec<ProbeFieldDetail>,
    #[serde(default)]
    passkey_available: Option<bool>,
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

fn format_bytes(bytes: i64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    const TB: f64 = GB * 1024.0;
    let b = bytes as f64;
    if b >= TB {
        format!("{:.2} TB", b / TB)
    } else if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.2} MB", b / MB)
    } else if b >= KB {
        format!("{:.2} KB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}

fn format_duration(seconds: i64) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    if days > 0 {
        format!("{}d {}h", days, hours)
    } else if hours > 0 {
        format!("{}h", hours)
    } else {
        let mins = seconds / 60;
        format!("{}m", mins)
    }
}

#[component]
pub fn SiteDetailPage() -> impl IntoView {
    let params = use_params_map();
    let site_id = move || {
        params
            .read()
            .get("id")
            .and_then(|id| id.parse::<i64>().ok())
            .unwrap_or(0)
    };

    // Edit state
    let (editing_url, set_editing_url) = signal(false);
    let (edit_url, set_edit_url) = signal(String::new());
    let (edit_api_url, set_edit_api_url) = signal(String::new());
    let (edit_cookie, set_edit_cookie) = signal(String::new());
    let (edit_passkey, set_edit_passkey) = signal(String::new());

    // Load site detail
    let detail = Resource::new(
        move || site_id(),
        |id| crate::server_fns::get_site_detail(id),
    );

    // Refresh stats action
    let refresh_action = Action::new(move |_: &()| {
        let id = site_id();
        async move { crate::server_fns::refresh_site_stats(id).await }
    });

    // Re-probe action
    let probe_action = Action::new(move |_: &()| {
        let id = site_id();
        async move { crate::server_fns::probe_site(id).await }
    });

    // Update site action
    let update_site_action = Action::new(move |args: &(i64, String, String, String, String)| {
        let (id, u, au, c, p) = args.clone();
        async move { crate::server_fns::update_site(id, u, au, c, p).await }
    });

    let refresh_pending = refresh_action.pending();
    let probe_pending = probe_action.pending();

    // Refetch detail after refresh or probe
    Effect::new(move |_| {
        if refresh_action.value().get().is_some() {
            detail.refetch();
        }
    });

    Effect::new(move |_| {
        if probe_action.value().get().is_some() {
            detail.refetch();
        }
    });

    Effect::new(move |_| {
        if update_site_action.value().get().is_some() {
            detail.refetch();
            set_editing_url.set(false);
        }
    });

    view! {
        <div class="dashboard">
            <div class="dashboard-header">
                <h1>"站点详情"</h1>
                <a href="/sites" class="btn btn-outline">"返回站点列表"</a>
            </div>

            // Error display for actions
            {move || {
                refresh_action
                    .value()
                    .get()
                    .and_then(|r| r.err())
                    .map(|e| {
                        view! {
                            <p class="error">{format!("刷新失败：{e}")}</p>
                        }
                    })
            }}
            {move || {
                refresh_action
                    .value()
                    .get()
                    .and_then(|r| r.ok())
                    .map(|_| {
                        view! {
                            <div class="form-alert form-alert--success">"统计数据已刷新"</div>
                        }
                    })
            }}
            {move || {
                probe_action
                    .value()
                    .get()
                    .and_then(|r| r.err())
                    .map(|e| {
                        view! {
                            <p class="error">{format!("连通测试失败：{e}")}</p>
                        }
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
                        view! {
                            <div class=class>
                                <div>{format!("{}{}", icon, result.message)}</div>
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
                        view! {
                            <p class="error">{format!("更新失败：{e}")}</p>
                        }
                    })
            }}
            {move || {
                update_site_action
                    .value()
                    .get()
                    .and_then(|r| r.ok())
                    .map(|_| {
                        view! {
                            <div class="form-alert form-alert--success">"站点已更新"</div>
                        }
                    })
            }}

            <Suspense fallback=move || view! { <p>"正在加载站点详情..."</p> }>
                {move || {
                    detail
                        .get()
                        .map(|result| match result {
                            Ok(data) => {
                                let site = data.site;
                                let site_name = site.name.clone();
                                let site_url = site.url.clone();
                                let site_api_url_display = site.api_url.clone().unwrap_or_default();
                                let site_url_for_edit = site.url.clone();
                                let site_api_url_for_edit = site.api_url.clone().unwrap_or_default();
                                let site_adapter = site.adapter_type.clone();
                                let site_auth_type = site.auth_type.clone();
                                let site_probe_status = site.probe_status.clone();
                                let user_stats = data.user_stats;
                                let probe_detail = data.probe_detail;
                                let (probe_card_class, probe_label) = match site_probe_status.as_str() {
                                    "ok" => ("green", "正常"),
                                    "partial" => ("warning", "部分可用"),
                                    "failed" => ("red", "失败"),
                                    "pending" => ("blue", "检测中"),
                                    _ => ("blue", "未知"),
                                };
                                let current_site_id = site.id;
                                view! {
                                    <div>
                                        // Site info cards
                                        <div class="stat-cards">
                                            <div class="stat-card stat-card--blue">
                                                <div class="stat-card__value">{site_name.clone()}</div>
                                                <div class="stat-card__label">"名称"</div>
                                            </div>
                                            <div class="stat-card stat-card--purple">
                                                <div class="stat-card__value">{site_adapter.clone()}</div>
                                                <div class="stat-card__label">"架构"</div>
                                            </div>
                                            <div class="stat-card stat-card--teal">
                                                <div class="stat-card__value">{site_auth_type.clone()}</div>
                                                <div class="stat-card__label">"认证方式"</div>
                                            </div>
                                            <div class={format!("stat-card stat-card--{}", probe_card_class)}>
                                                <div class="stat-card__value">{probe_label}</div>
                                                <div class="stat-card__label">"连通状态"</div>
                                            </div>
                                        </div>

                                        // Site info section with edit support
                                        <div class="stats-table-section">
                                            <h2>"站点信息"</h2>
                                            {move || {
                                                if editing_url.get() {
                                                    view! {
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
                                                                on:click=move |_| set_editing_url.set(false)
                                                            >
                                                                "取消"
                                                            </button>
                                                            <button
                                                                class="btn btn-primary"
                                                                on:click=move |_| {
                                                                    update_site_action.dispatch((current_site_id, edit_url.get_untracked(), edit_api_url.get_untracked(), edit_cookie.get_untracked(), edit_passkey.get_untracked()));
                                                                }
                                                            >
                                                                "保存"
                                                            </button>
                                                        </div>
                                                    }.into_any()
                                                } else {
                                                    let url_display = site_url.clone();
                                                    let url_href = site_url.clone();
                                                    let api_url_line = site_api_url_display.clone();
                                                    let url_for_btn = site_url_for_edit.clone();
                                                    let api_url_for_btn = site_api_url_for_edit.clone();
                                                    view! {
                                                        <p>
                                                            <a href=url_href target="_blank" rel="noopener">
                                                                {url_display}
                                                            </a>
                                                        </p>
                                                        {if !api_url_line.is_empty() {
                                                            Some(view! { <p class="text-muted">{format!("API: {}", api_url_line)}</p> })
                                                        } else {
                                                            None
                                                        }}
                                                        <button
                                                            class="btn btn--sm btn--outline"
                                                            on:click=move |_| {
                                                                set_edit_url.set(url_for_btn.clone());
                                                                set_edit_api_url.set(api_url_for_btn.clone());
                                                                set_edit_cookie.set(String::new());
                                                                set_edit_passkey.set(String::new());
                                                                set_editing_url.set(true);
                                                            }
                                                        >
                                                            "编辑"
                                                        </button>
                                                    }.into_any()
                                                }
                                            }}
                                        </div>

                                        // Action buttons
                                        <div class="form-actions">
                                            <button
                                                class="btn btn-primary"
                                                disabled=move || refresh_pending.get()
                                                on:click=move |_| { refresh_action.dispatch(()); }
                                            >
                                                {move || if refresh_pending.get() { "刷新中..." } else { "刷新统计" }}
                                            </button>
                                            <button
                                                class="btn btn-outline"
                                                disabled=move || probe_pending.get()
                                                on:click=move |_| { probe_action.dispatch(()); }
                                            >
                                                {move || if probe_pending.get() { "检测中..." } else { "重新检测" }}
                                            </button>
                                        </div>

                                        // User stats
                                        {match user_stats {
                                            Some(stats) => {
                                                view! {
                                                    <div class="stats-table-section">
                                                        <h2>"用户统计"</h2>
                                                        <div class="table-wrap">
                                                            <table class="stats-table">
                                                                <thead>
                                                                    <tr>
                                                                        <th>"指标"</th>
                                                                        <th>"当前值"</th>
                                                                    </tr>
                                                                </thead>
                                                                <tbody>
                                                                    <tr>
                                                                        <td>"上传量"</td>
                                                                        <td class="text-green">
                                                                            {stats
                                                                                .uploaded
                                                                                .map(format_bytes)
                                                                                .unwrap_or_else(|| "-".into())}
                                                                        </td>
                                                                    </tr>
                                                                    <tr>
                                                                        <td>"下载量"</td>
                                                                        <td class="text-blue">
                                                                            {stats
                                                                                .downloaded
                                                                                .map(format_bytes)
                                                                                .unwrap_or_else(|| "-".into())}
                                                                        </td>
                                                                    </tr>
                                                                    <tr>
                                                                        <td>"分享率"</td>
                                                                        <td>
                                                                            {stats
                                                                                .ratio
                                                                                .map(|r| format!("{:.3}", r))
                                                                                .unwrap_or_else(|| "-".into())}
                                                                        </td>
                                                                    </tr>
                                                                    <tr>
                                                                        <td>"积分"</td>
                                                                        <td>
                                                                            {stats
                                                                                .bonus
                                                                                .map(|b| format!("{:.1}", b))
                                                                                .unwrap_or_else(|| "-".into())}
                                                                        </td>
                                                                    </tr>
                                                                    <tr>
                                                                        <td>"用户等级"</td>
                                                                        <td>
                                                                            {stats
                                                                                .user_class
                                                                                .clone()
                                                                                .unwrap_or_else(|| "-".into())}
                                                                        </td>
                                                                    </tr>
                                                                    <tr>
                                                                        <td>"做种数"</td>
                                                                        <td>
                                                                            {stats
                                                                                .seeding_count
                                                                                .map(|c| c.to_string())
                                                                                .unwrap_or_else(|| "-".into())}
                                                                        </td>
                                                                    </tr>
                                                                    <tr>
                                                                        <td>"下载中"</td>
                                                                        <td>
                                                                            {stats
                                                                                .leeching_count
                                                                                .map(|c| c.to_string())
                                                                                .unwrap_or_else(|| "-".into())}
                                                                        </td>
                                                                    </tr>
                                                                    <tr>
                                                                        <td>"做种量"</td>
                                                                        <td>
                                                                            {stats
                                                                                .seeding_size
                                                                                .map(format_bytes)
                                                                                .unwrap_or_else(|| "-".into())}
                                                                        </td>
                                                                    </tr>
                                                                    <tr>
                                                                        <td>"做种时间"</td>
                                                                        <td>
                                                                            {stats
                                                                                .upload_time_seconds
                                                                                .map(format_duration)
                                                                                .unwrap_or_else(|| "-".into())}
                                                                        </td>
                                                                    </tr>
                                                                </tbody>
                                                            </table>
                                                        </div>
                                                    </div>
                                                }
                                                    .into_any()
                                            }
                                            None => {
                                                view! {
                                                    <div class="stats-table-section">
                                                        <h2>"用户统计"</h2>
                                                        <p>"暂无用户统计，请尝试刷新。"</p>
                                                    </div>
                                                }
                                                    .into_any()
                                            }
                                        }}

                                        // Probe detail
                                        {match probe_detail {
                                            Some(json) => {
                                                let parsed = serde_json::from_str::<ProbeDetail>(&json).ok();
                                                match parsed {
                                                    Some(detail) => {
                                                        let api_status = detail.api_reachable.as_ref().map(|f| {
                                                            if f.success { "✅ 可达".to_string() }
                                                            else { format!("❌ {}", f.error.as_deref().unwrap_or("不可达")) }
                                                        });
                                                        let successes: Vec<_> = detail.user_info_fields.iter()
                                                            .filter(|f| f.success)
                                                            .filter_map(|f| f.value_preview.as_ref().map(|v| (probe_field_label(&f.field_name), v.clone())))
                                                            .collect();
                                                        let failures: Vec<_> = detail.user_info_fields.iter()
                                                            .filter(|f| !f.success)
                                                            .map(|f| probe_field_label(&f.field_name).to_string())
                                                            .collect();
                                                        let passkey_info = match (detail.passkey_available, &detail.passkey_error) {
                                                            (Some(true), _) => Some("✅ 已获取"),
                                                            (_, Some(_)) => Some("❌ 获取失败"),
                                                            (Some(false), None) => Some("— 未提供"),
                                                            _ => None,
                                                        };
                                                        view! {
                                                            <div class="stats-table-section">
                                                                <h2>"连通详情"</h2>
                                                                {api_status.map(|status| view! {
                                                                    <div class="probe-detail-row">
                                                                        <span class="probe-field-label">"辅种 API"</span>
                                                                        "："
                                                                        <span>{status}</span>
                                                                    </div>
                                                                })}
                                                                {passkey_info.map(|info| view! {
                                                                    <div class="probe-detail-row">
                                                                        <span class="probe-field-label">"Passkey"</span>
                                                                        "："
                                                                        <span>{info}</span>
                                                                    </div>
                                                                })}
                                                                {(!successes.is_empty()).then(|| view! {
                                                                    <div class="probe-success-section">
                                                                        <div class="probe-section-title">"获取到的个人信息："</div>
                                                                        <div class="table-wrap">
                                                                            <table class="stats-table">
                                                                                <thead>
                                                                                    <tr>
                                                                                        <th>"指标"</th>
                                                                                        <th>"值"</th>
                                                                                    </tr>
                                                                                </thead>
                                                                                <tbody>
                                                                                    {successes
                                                                                        .into_iter()
                                                                                        .map(|(label, value)| view! {
                                                                                            <tr>
                                                                                                <td>{label}</td>
                                                                                                <td class="text-green">{value}</td>
                                                                                            </tr>
                                                                                        })
                                                                                        .collect::<Vec<_>>()}
                                                                                </tbody>
                                                                            </table>
                                                                        </div>
                                                                    </div>
                                                                })}
                                                                {(!failures.is_empty()).then(|| view! {
                                                                    <div class="probe-failure-section">
                                                                        <div class="probe-section-title">"未获取到的项目："</div>
                                                                        <ul class="probe-failure-list">
                                                                            {failures
                                                                                .into_iter()
                                                                                .map(|label| view! { <li>{label}</li> })
                                                                                .collect::<Vec<_>>()}
                                                                        </ul>
                                                                    </div>
                                                                })}
                                                            </div>
                                                        }
                                                            .into_any()
                                                    }
                                                    None => {
                                                        view! {
                                                            <div class="stats-table-section">
                                                                <h2>"连通详情"</h2>
                                                                <pre class="probe-detail-json">{json}</pre>
                                                            </div>
                                                        }
                                                            .into_any()
                                                    }
                                                }
                                            }
                                            None => {
                                                view! {
                                                    <div class="stats-table-section">
                                                        <h2>"连通详情"</h2>
                                                        <p>"暂无连通详情。"</p>
                                                    </div>
                                                }
                                                    .into_any()
                                            }
                                        }}
                                    </div>
                                }
                                    .into_any()
                            }
                            Err(e) => {
                                view! {
                                    <p class="error">
                                        {format!("站点详情加载失败：{e}")}
                                    </p>
                                }
                                    .into_any()
                            }
                        })
                }}
            </Suspense>
        </div>
    }
}
