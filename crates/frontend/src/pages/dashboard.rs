use crate::server_fns::{
    get_dashboard_data, DashboardData, DashboardOverview, SiteReseedStats, TrendPoint,
    UserInfoAggregate,
};
use crate::utils::{format_bytes, format_duration};
use crate::ws::use_dashboard_ws;
use leptos::prelude::*;

#[component]
pub fn DashboardPage() -> impl IntoView {
    let (days, set_days) = signal(7i64);

    // Initial data via server function (works for SSR first paint).
    let dashboard_data = Resource::new(move || days.get(), |d| get_dashboard_data(d));

    // Real-time updates via WebSocket (only active after hydration on the client).
    let ws_update = use_dashboard_ws();

    // Signals that hold the latest data; seeded from the resource, then
    // overwritten by WS pushes for the fields the WS provides.
    let (overview, set_overview) = signal(None::<DashboardOverview>);
    let (site_stats, set_site_stats) = signal(None::<Vec<SiteReseedStats>>);
    let (trend, set_trend) = signal(None::<Vec<TrendPoint>>);
    let (user_info, set_user_info) = signal(None::<UserInfoAggregate>);
    let (load_error, set_load_error) = signal(None::<String>);

    // Seed signals whenever the resource resolves (initial load or day-selector change).
    Effect::new(move |_| {
        if let Some(result) = dashboard_data.get() {
            match result {
                Ok(data) => {
                    set_overview.set(Some(data.overview));
                    set_site_stats.set(Some(data.site_stats));
                    set_trend.set(Some(data.trend));
                    set_user_info.set(Some(data.user_info));
                    set_load_error.set(None);
                }
                Err(e) => {
                    set_load_error.set(Some(format!("{e}")));
                }
            }
        }
    });

    // Merge WS updates into the live signals.
    // The WS pushes overview + site_stats + user_info but NOT trend.
    Effect::new(move |_| {
        if let Some(update) = ws_update.get() {
            if let Some(o) = update.overview {
                set_overview.set(Some(o));
            }
            if let Some(s) = update.site_stats {
                set_site_stats.set(Some(s));
            }
            if let Some(u) = update.user_info {
                set_user_info.set(Some(u));
            }
        }
    });

    view! {
        <div class="dashboard">
            <div class="dashboard-header">
                <h1>"仪表盘"</h1>
                <div class="trend-selector">
                    <button
                        class:active=move || days.get() == 7
                        on:click=move |_| set_days.set(7)
                    >
                        "7天"
                    </button>
                    <button
                        class:active=move || days.get() == 30
                        on:click=move |_| set_days.set(30)
                    >
                        "30天"
                    </button>
                    <button
                        class:active=move || days.get() == 0
                        on:click=move |_| set_days.set(0)
                    >
                        "全部"
                    </button>
                </div>
            </div>

            <Suspense fallback=move || {
                view! { <p>"正在加载仪表盘..."</p> }
            }>
                {move || {
                    // Show error from initial load if nothing has been populated yet.
                    if let Some(err) = load_error.get() {
                        if overview.get().is_none() {
                            return Some(
                                view! {
                                    <div class="load-error">
                                        <span>{format!("仪表盘加载失败：{err}")}</span>
                                        <button
                                            class="btn btn--sm btn--outline"
                                            on:click=move |_| dashboard_data.refetch()
                                        >
                                            "重试"
                                        </button>
                                    </div>
                                }
                                    .into_any(),
                            );
                        }
                    }

                    // Wait until we have data (from either resource or WS).
                    let o = overview.get()?;
                    let ss = site_stats.get()?;
                    let tr = trend.get().unwrap_or_default();
                    let ui = user_info.get()?;

                    let data = DashboardData {
                        overview: o,
                        site_stats: ss.clone(),
                        trend: tr.clone(),
                        user_info: ui.clone(),
                    };

                    Some(
                        view! {
                            <div>
                                <OverviewCards data=data />
                                <TrendChart points=tr />
                                <SiteStatsTable stats=ss />
                                <UserInfoTable info=ui />
                            </div>
                        }
                            .into_any(),
                    )
                }}
            </Suspense>
        </div>
    }
}

#[component]
fn OverviewCards(data: DashboardData) -> impl IntoView {
    let o = data.overview;
    view! {
        <div class="stat-cards">
            <StatCard
                label="运行中任务"
                value=o.running_tasks.to_string()
                accent="blue"
            />
            <StatCard
                label="今日成功"
                value=o.today_success.to_string()
                accent="green"
            />
            <StatCard
                label="今日失败"
                value=o.today_failed.to_string()
                accent="red"
            />
            <StatCard
                label="站点数"
                value=o.total_sites.to_string()
                accent="purple"
            />
            <StatCard
                label="监控种子数"
                value=o.tracked_torrents.to_string()
                accent="teal"
            />
        </div>
    }
}

#[component]
fn StatCard(label: &'static str, value: String, accent: &'static str) -> impl IntoView {
    let class = format!("stat-card stat-card--{accent}");
    view! {
        <div class=class>
            <div class="stat-card__value">{value}</div>
            <div class="stat-card__label">{label}</div>
        </div>
    }
}

#[component]
fn TrendChart(points: Vec<TrendPoint>) -> impl IntoView {
    if points.is_empty() {
        return view! { <div class="trend-chart"><p>"暂无趋势数据。"</p></div> }.into_any();
    }

    let max_val = points
        .iter()
        .map(|p| p.succeeded.max(p.failed))
        .max()
        .unwrap_or(1)
        .max(1) as f64;

    let width = 800.0_f64;
    let height = 200.0_f64;
    let padding = 40.0_f64;
    let chart_w = width - padding * 2.0;
    let chart_h = height - padding * 2.0;
    let n = points.len() as f64;
    let step = if n > 1.0 {
        chart_w / (n - 1.0)
    } else {
        chart_w
    };

    let success_path = points
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let x = padding + i as f64 * step;
            let y = padding + chart_h - (p.succeeded as f64 / max_val * chart_h);
            if i == 0 {
                format!("M{x:.1},{y:.1}")
            } else {
                format!("L{x:.1},{y:.1}")
            }
        })
        .collect::<String>();

    let failed_path = points
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let x = padding + i as f64 * step;
            let y = padding + chart_h - (p.failed as f64 / max_val * chart_h);
            if i == 0 {
                format!("M{x:.1},{y:.1}")
            } else {
                format!("L{x:.1},{y:.1}")
            }
        })
        .collect::<String>();

    let x_labels: Vec<(f64, String)> = points
        .iter()
        .enumerate()
        .filter(|(i, _)| {
            let total = points.len();
            if total <= 10 {
                true
            } else {
                i % (total / 7).max(1) == 0 || *i == total - 1
            }
        })
        .map(|(i, p)| {
            let x = padding + i as f64 * step;
            let label = if p.date.len() >= 10 {
                p.date[5..10].to_string()
            } else {
                p.date.clone()
            };
            (x, label)
        })
        .collect();

    view! {
        <div class="trend-chart">
            <h2>"辅种趋势"</h2>
            <svg
                viewBox=format!("0 0 {width} {height}")
                class="trend-svg"
                preserveAspectRatio="xMidYMid meet"
            >
                // Y axis
                <line
                    x1=format!("{padding}")
                    y1=format!("{padding}")
                    x2=format!("{padding}")
                    y2=format!("{}", padding + chart_h)
                    stroke="var(--chart-axis)"
                    stroke-width="1"
                />
                // X axis
                <line
                    x1=format!("{padding}")
                    y1=format!("{}", padding + chart_h)
                    x2=format!("{}", padding + chart_w)
                    y2=format!("{}", padding + chart_h)
                    stroke="var(--chart-axis)"
                    stroke-width="1"
                />

                // Success line
                <path d=success_path fill="none" stroke="var(--chart-success)" stroke-width="2" />
                // Failed line
                <path d=failed_path fill="none" stroke="var(--chart-failed)" stroke-width="2" />

                // X labels
                {x_labels
                    .into_iter()
                    .map(|(x, label)| {
                        view! {
                            <text
                                x=format!("{x:.1}")
                                y=format!("{}", padding + chart_h + 16.0)
                                text-anchor="middle"
                                font-size="11"
                                fill="var(--chart-label)"
                            >
                                {label}
                            </text>
                        }
                    })
                    .collect::<Vec<_>>()}

                // Legend
                <circle cx=format!("{}", padding + 10.0) cy="12" r="4" fill="var(--chart-success)" />
                <text x=format!("{}", padding + 18.0) y="16" font-size="12" fill="var(--chart-label)">
                    "成功"
                </text>
                <circle cx=format!("{}", padding + 80.0) cy="12" r="4" fill="var(--chart-failed)" />
                <text x=format!("{}", padding + 88.0) y="16" font-size="12" fill="var(--chart-label)">
                    "失败"
                </text>
            </svg>
        </div>
    }
    .into_any()
}

#[component]
fn SiteStatsTable(stats: Vec<SiteReseedStats>) -> impl IntoView {
    view! {
        <div class="stats-table-section">
            <h2>"站点辅种统计"</h2>
            {if stats.is_empty() {
                view! { <p>"暂无辅种记录。"</p> }.into_any()
            } else {
                view! {
                    <div class="table-wrap">
                        <table class="stats-table">
                            <thead>
                                <tr>
                                    <th>"站点"</th>
                                    <th>"匹配数"</th>
                                    <th>"成功"</th>
                                    <th>"失败"</th>
                                    <th>"跳过"</th>
                                    <th>"成功率"</th>
                                    <th>"状态"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {stats
                                    .into_iter()
                                    .map(|s| {
                                        let (status_class, status_label) = match s.breaker_status.as_str() {
                                            "tripped" => ("text-red", "已暂停"),
                                            "ok" => ("text-green", "正常"),
                                            _ => ("text-muted", "—"),
                                        };
                                        view! {
                                            <tr>
                                                <td>{s.site_name}</td>
                                                <td>{s.matched}</td>
                                                <td class="text-green">{s.succeeded}</td>
                                                <td class="text-red">{s.failed}</td>
                                                <td>{s.skipped}</td>
                                                <td>{format!("{:.1}%", s.success_rate)}</td>
                                                <td class=status_class>{status_label}</td>
                                            </tr>
                                        }
                                    })
                                    .collect::<Vec<_>>()}
                            </tbody>
                        </table>
                    </div>
                }
                    .into_any()
            }}
        </div>
    }
}

#[component]
fn UserInfoTable(info: UserInfoAggregate) -> impl IntoView {
    view! {
        <div class="user-info-section">
            <h2>"各站点账号概览"</h2>

            <div class="stat-cards stat-cards--summary">
                <StatCard
                    label="总上传"
                    value=format_bytes(info.total_uploaded)
                    accent="green"
                />
                <StatCard
                    label="总下载"
                    value=format_bytes(info.total_downloaded)
                    accent="blue"
                />
                <StatCard
                    label="总做种数"
                    value=info.total_seeding.to_string()
                    accent="purple"
                />
                <StatCard
                    label="总积分"
                    value=format!("{:.1}", info.total_bonus)
                    accent="teal"
                />
            </div>

            {if info.sites.is_empty() {
                view! { <p>"暂无用户统计数据。"</p> }.into_any()
            } else {
                view! {
                    <div class="table-wrap">
                        <table class="stats-table">
                            <thead>
                                <tr>
                                    <th>"站点"</th>
                                    <th>"上传量"</th>
                                    <th>"下载量"</th>
                                    <th>"分享率"</th>
                                    <th>"积分"</th>
                                    <th>"等级"</th>
                                    <th>"做种数"</th>
                                    <th>"吸血数"</th>
                                    <th>"做种体积"</th>
                                    <th>"上传时间统计"</th>
                                    <th>"更新时间"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {info
                                    .sites
                                    .into_iter()
                                    .map(|s| {
                                        view! {
                                            <tr>
                                                <td>{s.site_name}</td>
                                                <td>
                                                    {s.uploaded.map(format_bytes).unwrap_or_else(|| "-".into())}
                                                </td>
                                                <td>
                                                    {s
                                                        .downloaded
                                                        .map(format_bytes)
                                                        .unwrap_or_else(|| "-".into())}
                                                </td>
                                                <td>
                                                    {s
                                                        .ratio
                                                        .map(|r| format!("{:.3}", r))
                                                        .unwrap_or_else(|| "-".into())}
                                                </td>
                                                <td>
                                                    {s
                                                        .bonus
                                                        .map(|b| format!("{:.1}", b))
                                                        .unwrap_or_else(|| "-".into())}
                                                </td>
                                                <td>
                                                    {s.user_class.clone().unwrap_or_else(|| "-".into())}
                                                </td>
                                                <td>
                                                    {s
                                                        .seeding_count
                                                        .map(|c: i64| c.to_string())
                                                        .unwrap_or_else(|| "-".into())}
                                                </td>
                                                <td>
                                                    {s
                                                        .leeching_count
                                                        .map(|c: i64| c.to_string())
                                                        .unwrap_or_else(|| "-".into())}
                                                </td>
                                                <td>
                                                    {s
                                                        .seeding_size
                                                        .map(format_bytes)
                                                        .unwrap_or_else(|| "-".into())}
                                                </td>
                                                <td>
                                                    {s
                                                        .upload_time_seconds
                                                        .map(format_duration)
                                                        .unwrap_or_else(|| "-".into())}
                                                </td>
                                                <td class="text-muted">
                                                    {if s.fetched_at.len() >= 16 {
                                                        s.fetched_at[..16].to_string()
                                                    } else {
                                                        s.fetched_at.clone()
                                                    }}
                                                </td>
                                            </tr>
                                        }
                                    })
                                    .collect::<Vec<_>>()}
                            </tbody>
                        </table>
                    </div>
                }
                    .into_any()
            }}
        </div>
    }
}
