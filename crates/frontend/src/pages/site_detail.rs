use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

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

    // Edit URL state
    let (editing_url, set_editing_url) = signal(false);
    let (edit_url, set_edit_url) = signal(String::new());
    let (edit_api_url, set_edit_api_url) = signal(String::new());

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

    // Update URL action
    let update_url_action = Action::new(move |args: &(i64, String, String)| {
        let (id, u, au) = args.clone();
        async move { crate::server_fns::update_site_url(id, u, au).await }
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
        if update_url_action.value().get().is_some() {
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
                    .map(|_| {
                        view! {
                            <div class="form-alert form-alert--success">"连通测试完成"</div>
                        }
                    })
            }}
            {move || {
                update_url_action
                    .value()
                    .get()
                    .and_then(|r| r.err())
                    .map(|e| {
                        view! {
                            <p class="error">{format!("URL 更新失败：{e}")}</p>
                        }
                    })
            }}
            {move || {
                update_url_action
                    .value()
                    .get()
                    .and_then(|r| r.ok())
                    .map(|_| {
                        view! {
                            <div class="form-alert form-alert--success">"站点 URL 已更新"</div>
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

                                        // Site URL section with edit support
                                        <div class="stats-table-section">
                                            <h2>"站点 URL"</h2>
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
                                                                    update_url_action.dispatch((current_site_id, edit_url.get_untracked(), edit_api_url.get_untracked()));
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
                                                                set_editing_url.set(true);
                                                            }
                                                        >
                                                            "编辑 URL"
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

                                        // Probe detail JSON
                                        {match probe_detail {
                                            Some(json) => {
                                                view! {
                                                    <div class="stats-table-section">
                                                        <h2>"连通详情"</h2>
                                                        <pre class="probe-detail-json">{json}</pre>
                                                    </div>
                                                }
                                                    .into_any()
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
