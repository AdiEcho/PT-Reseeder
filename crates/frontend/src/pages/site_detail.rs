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

    view! {
        <div class="dashboard">
            <div class="dashboard-header">
                <h1>"Site Detail"</h1>
                <a href="/sites" class="btn btn-outline">"Back to Sites"</a>
            </div>

            // Error display for actions
            {move || {
                refresh_action
                    .value()
                    .get()
                    .and_then(|r| r.err())
                    .map(|e| {
                        view! {
                            <p class="error">{format!("Refresh failed: {e}")}</p>
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
                            <p class="error">{format!("Probe failed: {e}")}</p>
                        }
                    })
            }}

            <Suspense fallback=move || view! { <p>"Loading site detail..."</p> }>
                {move || {
                    detail
                        .get()
                        .map(|result| match result {
                            Ok(data) => {
                                let site = data.site;
                                let site_name = site.name.clone();
                                let site_url = site.url.clone();
                                let site_adapter = site.adapter_type.clone();
                                let site_auth_type = site.auth_type.clone();
                                let site_probe_status = site.probe_status.clone();
                                let user_stats = data.user_stats;
                                let probe_detail = data.probe_detail;
                                let (_probe_class, probe_label) = match site_probe_status.as_str()
                                {
                                    "ok" => ("text-green", "OK"),
                                    "failed" => ("text-red", "Failed"),
                                    "pending" => ("text-muted", "Pending"),
                                    _ => ("text-muted", "Unknown"),
                                };
                                view! {
                                    <div>
                                        // Site info cards
                                        <div class="stat-cards">
                                            <div class="stat-card stat-card--blue">
                                                <div class="stat-card__value">{site_name.clone()}</div>
                                                <div class="stat-card__label">"Name"</div>
                                            </div>
                                            <div class="stat-card stat-card--purple">
                                                <div class="stat-card__value">{site_adapter.clone()}</div>
                                                <div class="stat-card__label">"Adapter"</div>
                                            </div>
                                            <div class="stat-card stat-card--teal">
                                                <div class="stat-card__value">{site_auth_type.clone()}</div>
                                                <div class="stat-card__label">"Auth Type"</div>
                                            </div>
                                            <div class={format!("stat-card stat-card--{}", if site_probe_status == "ok" { "green" } else { "red" })}>
                                                <div class="stat-card__value">{probe_label}</div>
                                                <div class="stat-card__label">"Probe Status"</div>
                                            </div>
                                        </div>

                                        // Site URL
                                        <div class="stats-table-section">
                                            <h2>"Site URL"</h2>
                                            <p>
                                                <a href=site_url.clone() target="_blank" rel="noopener">
                                                    {site_url.clone()}
                                                </a>
                                            </p>
                                        </div>

                                        // Action buttons
                                        <div class="form-actions">
                                            <button
                                                class="btn btn-primary"
                                                on:click=move |_| { refresh_action.dispatch(()); }
                                            >
                                                "Refresh Stats"
                                            </button>
                                            <button
                                                class="btn btn-outline"
                                                on:click=move |_| { probe_action.dispatch(()); }
                                            >
                                                "Re-probe"
                                            </button>
                                        </div>

                                        // User stats
                                        {match user_stats {
                                            Some(stats) => {
                                                view! {
                                                    <div class="stats-table-section">
                                                        <h2>"User Statistics"</h2>
                                                        <div class="table-wrap">
                                                            <table class="stats-table">
                                                                <thead>
                                                                    <tr>
                                                                        <th>"Field"</th>
                                                                        <th>"Value"</th>
                                                                    </tr>
                                                                </thead>
                                                                <tbody>
                                                                    <tr>
                                                                        <td>"Uploaded"</td>
                                                                        <td class="text-green">
                                                                            {stats
                                                                                .uploaded
                                                                                .map(format_bytes)
                                                                                .unwrap_or_else(|| "-".into())}
                                                                        </td>
                                                                    </tr>
                                                                    <tr>
                                                                        <td>"Downloaded"</td>
                                                                        <td class="text-blue">
                                                                            {stats
                                                                                .downloaded
                                                                                .map(format_bytes)
                                                                                .unwrap_or_else(|| "-".into())}
                                                                        </td>
                                                                    </tr>
                                                                    <tr>
                                                                        <td>"Ratio"</td>
                                                                        <td>
                                                                            {stats
                                                                                .ratio
                                                                                .map(|r| format!("{:.3}", r))
                                                                                .unwrap_or_else(|| "-".into())}
                                                                        </td>
                                                                    </tr>
                                                                    <tr>
                                                                        <td>"Bonus"</td>
                                                                        <td>
                                                                            {stats
                                                                                .bonus
                                                                                .map(|b| format!("{:.1}", b))
                                                                                .unwrap_or_else(|| "-".into())}
                                                                        </td>
                                                                    </tr>
                                                                    <tr>
                                                                        <td>"Class"</td>
                                                                        <td>
                                                                            {stats
                                                                                .user_class
                                                                                .clone()
                                                                                .unwrap_or_else(|| "-".into())}
                                                                        </td>
                                                                    </tr>
                                                                    <tr>
                                                                        <td>"Seeding"</td>
                                                                        <td>
                                                                            {stats
                                                                                .seeding_count
                                                                                .map(|c| c.to_string())
                                                                                .unwrap_or_else(|| "-".into())}
                                                                        </td>
                                                                    </tr>
                                                                    <tr>
                                                                        <td>"Leeching"</td>
                                                                        <td>
                                                                            {stats
                                                                                .leeching_count
                                                                                .map(|c| c.to_string())
                                                                                .unwrap_or_else(|| "-".into())}
                                                                        </td>
                                                                    </tr>
                                                                    <tr>
                                                                        <td>"Seeding Size"</td>
                                                                        <td>
                                                                            {stats
                                                                                .seeding_size
                                                                                .map(format_bytes)
                                                                                .unwrap_or_else(|| "-".into())}
                                                                        </td>
                                                                    </tr>
                                                                    <tr>
                                                                        <td>"Upload Time"</td>
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
                                                        <h2>"User Statistics"</h2>
                                                        <p>"No user stats available. Try refreshing."</p>
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
                                                        <h2>"Probe Detail"</h2>
                                                        <pre class="probe-detail-json">{json}</pre>
                                                    </div>
                                                }
                                                    .into_any()
                                            }
                                            None => {
                                                view! {
                                                    <div class="stats-table-section">
                                                        <h2>"Probe Detail"</h2>
                                                        <p>"No probe detail available."</p>
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
                                        {format!("Failed to load site detail: {e}")}
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
