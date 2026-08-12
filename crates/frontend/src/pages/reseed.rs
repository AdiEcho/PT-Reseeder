use crate::components::async_view::AsyncView;
use crate::components::empty_state::EmptyState;
use crate::components::resizable_th::ResizableTh;
use crate::server_fns::{
    get_reseed_run_detail, get_reseed_runs, DryRunPreviewItemInfo, ReseedRunDetail, ReseedRunInfo,
};
use crate::utils::format_bytes;
use leptos::prelude::*;
use leptos_router::components::A;

fn truncate_utf8(value: &str, max_bytes: usize) -> &str {
    let mut end = value.len().min(max_bytes);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

fn run_status_class(status: &str) -> &'static str {
    match status {
        "success" => "text-green",
        "dry_run" => "text-blue",
        "failed" | "error" => "text-red",
        "partial" => "text-yellow",
        "running" => "text-blue",
        "skipped" => "text-muted",
        _ => "text-muted",
    }
}

fn run_status_label(status: &str, dry_run: bool) -> &'static str {
    if dry_run || status == "dry_run" {
        return "试运行";
    }
    match status {
        "success" => "成功",
        "failed" | "error" => "失败",
        "partial" => "部分成功",
        "running" => "运行中",
        "skipped" => "已跳过",
        _ => "未知",
    }
}

fn format_duration_ms(ms: Option<i64>) -> String {
    match ms {
        Some(v) if v >= 0 => format!("{:.1} 秒", v as f64 / 1000.0),
        _ => "-".into(),
    }
}

#[component]
pub fn ReseedPage() -> impl IntoView {
    let (version, set_version) = signal(0u64);
    let (selected_log_id, set_selected_log_id) = signal(None::<i64>);
    // "all" | "dry_run" | "real"
    let (mode_filter, set_mode_filter) = signal("all".to_string());

    let runs = Resource::new(move || version.get(), |_| get_reseed_runs(100));

    let detail = Resource::new(
        move || selected_log_id.get(),
        |log_id| async move {
            match log_id {
                Some(id) => get_reseed_run_detail(id).await.ok().flatten(),
                None => None,
            }
        },
    );

    view! {
        <div class="dashboard">
            <div class="dashboard-header">
                <h1>"辅种结果"</h1>
                <div class="trend-selector">
                    <button
                        class:active=move || mode_filter.get() == "all"
                        on:click=move |_| set_mode_filter.set("all".into())
                    >
                        "全部"
                    </button>
                    <button
                        class:active=move || mode_filter.get() == "real"
                        on:click=move |_| set_mode_filter.set("real".into())
                    >
                        "正式运行"
                    </button>
                    <button
                        class:active=move || mode_filter.get() == "dry_run"
                        on:click=move |_| set_mode_filter.set("dry_run".into())
                    >
                        "试运行"
                    </button>
                    <button
                        class="btn btn--sm btn--outline"
                        on:click=move |_| set_version.update(|v| *v += 1)
                    >
                        "刷新"
                    </button>
                </div>
            </div>

            <p class="text-muted">
                "展示每次辅种任务识别到的种子明细：时间、站点、标题、链接、本地目录与大小。试运行只预览，不会写入下载器。"
            </p>

            <div class="stats-table-section">
                <h2>"运行记录"</h2>
                <AsyncView
                    resource=runs
                    error_label="辅种结果"
                    on_retry=move || set_version.update(|v| *v += 1)
                    render={move |list: Vec<ReseedRunInfo>| {
                        let filter = mode_filter.get();
                        let filtered: Vec<ReseedRunInfo> = list
                            .into_iter()
                            .filter(|run| match filter.as_str() {
                                "dry_run" => run.dry_run || run.status == "dry_run",
                                "real" => !(run.dry_run || run.status == "dry_run"),
                                _ => true,
                            })
                            .collect();

                        if filtered.is_empty() {
                            return view! {
                                <EmptyState
                                    icon="🌱"
                                    message="还没有辅种运行记录。去「任务」页创建辅种任务并执行试运行或正式运行。"
                                />
                            }
                            .into_any();
                        }

                        view! {
                            <div class="table-wrap">
                                <table class="stats-table stats-table--resizable">
                                    <thead>
                                        <tr>
                                            <ResizableTh col_key="reseed-runs-time" default_width=160>
                                                "时间"
                                            </ResizableTh>
                                            <ResizableTh col_key="reseed-runs-task" default_width=220>
                                                "任务"
                                            </ResizableTh>
                                            <ResizableTh col_key="reseed-runs-status" default_width=90>
                                                "状态"
                                            </ResizableTh>
                                            <ResizableTh col_key="reseed-runs-matched" default_width=80>
                                                "识别数"
                                            </ResizableTh>
                                            <ResizableTh
                                                col_key="reseed-runs-success"
                                                default_width=70
                                                class="table-col--secondary"
                                            >
                                                "成功"
                                            </ResizableTh>
                                            <ResizableTh
                                                col_key="reseed-runs-failed"
                                                default_width=70
                                                class="table-col--secondary"
                                            >
                                                "失败"
                                            </ResizableTh>
                                            <ResizableTh
                                                col_key="reseed-runs-size"
                                                default_width=110
                                                class="table-col--secondary"
                                            >
                                                "总大小"
                                            </ResizableTh>
                                            <ResizableTh
                                                col_key="reseed-runs-duration"
                                                default_width=90
                                                class="table-col--secondary"
                                            >
                                                "耗时"
                                            </ResizableTh>
                                            <ResizableTh col_key="reseed-runs-actions" default_width=150>
                                                "操作"
                                            </ResizableTh>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {filtered
                                            .into_iter()
                                            .map(|run| {
                                                let log_id = run.log_id;
                                                let sc = run_status_class(&run.status);
                                                let label = run_status_label(&run.status, run.dry_run);
                                                let ts = truncate_utf8(&run.created_at, 19).to_string();
                                                let size = run
                                                    .total_size
                                                    .map(format_bytes)
                                                    .unwrap_or_else(|| "-".into());
                                                let duration = format_duration_ms(run.duration_ms);
                                                let selected = move || {
                                                    selected_log_id.get() == Some(log_id)
                                                };
                                                view! {
                                                    <tr
                                                        class="clickable-row"
                                                        class:row-selected=selected
                                                        on:click=move |_| {
                                                            set_selected_log_id.update(|cur| {
                                                                *cur = if *cur == Some(log_id) {
                                                                    None
                                                                } else {
                                                                    Some(log_id)
                                                                };
                                                            });
                                                        }
                                                    >
                                                        <td class="text-muted">{ts}</td>
                                                        <td>
                                                            <div>{run.task_name.clone()}</div>
                                                            <div class="text-muted table-subtext">
                                                                {format!("任务 #{}", run.task_id)}
                                                            </div>
                                                        </td>
                                                        <td class=sc>{label}</td>
                                                        <td>{run.matched_count}</td>
                                                        <td class="text-green table-col--secondary">
                                                            {run.succeeded_count}
                                                        </td>
                                                        <td class="text-red table-col--secondary">
                                                            {run.failed_count}
                                                        </td>
                                                        <td class="text-muted table-col--secondary">{size}</td>
                                                        <td class="text-muted table-col--secondary">{duration}</td>
                                                        <td class="table__action-cell">
                                                            <button
                                                                class="btn btn--sm btn--outline"
                                                                on:click=move |ev| {
                                                                    ev.stop_propagation();
                                                                    set_selected_log_id.set(Some(log_id));
                                                                }
                                                            >
                                                                "明细"
                                                            </button>
                                                            <A
                                                                href=format!("/logs?task_id={}", run.task_id)
                                                                attr:class="btn btn--sm btn--outline"
                                                                on:click=move |ev| ev.stop_propagation()
                                                            >
                                                                "日志"
                                                            </A>
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
                />
            </div>

            {move || {
                selected_log_id.get().map(|_| {
                    view! {
                        <Suspense fallback=move || {
                            view! {
                                <div class="stats-table-section">
                                    <p class="text-muted">"正在加载明细..."</p>
                                </div>
                            }
                        }>
                            {move || {
                                detail.get().map(|maybe| match maybe {
                                    Some(d) => view! {
                                        <ReseedRunDetailPanel
                                            detail=d
                                            on_close=move || set_selected_log_id.set(None)
                                        />
                                    }.into_any(),
                                    None => view! {
                                        <div class="stats-table-section">
                                            <p class="text-muted">
                                                "未找到该次运行的明细（可能是旧日志，没有结构化结果）。"
                                            </p>
                                        </div>
                                    }.into_any(),
                                })
                            }}
                        </Suspense>
                    }
                })
            }}
        </div>
    }
}

#[component]
fn ReseedRunDetailPanel<F>(detail: ReseedRunDetail, on_close: F) -> impl IntoView
where
    F: Fn() + Clone + 'static,
{
    let run = detail.run;
    let items = detail.items;
    let status_label = run_status_label(&run.status, run.dry_run);
    let status_class = run_status_class(&run.status);
    let size = run
        .total_size
        .map(format_bytes)
        .unwrap_or_else(|| "-".into());
    let duration = format_duration_ms(run.duration_ms);
    let on_close_btn = on_close;
    let mode_text = if run.dry_run {
        "试运行（未写入下载器）"
    } else {
        "正式运行"
    };

    view! {
        <div class="stats-table-section">
            <div class="form-actions form-actions--split">
                <h2>
                    {format!(
                        "{} · {} · 识别 {} 条",
                        run.task_name, status_label, run.matched_count
                    )}
                </h2>
                <button class="btn btn--sm btn--outline" on:click=move |_| on_close_btn()>
                    "关闭明细"
                </button>
            </div>
            <div class="text-muted table-subtext">
                {format!(
                    "时间 {} · 成功 {} · 失败 {} · 总大小 {} · 耗时 {} · {}",
                    run.created_at,
                    run.succeeded_count,
                    run.failed_count,
                    size,
                    duration,
                    mode_text
                )}
            </div>
            <p>
                <span class=status_class>{status_label}</span>
            </p>
            <ReseedItemsTable items=items />
        </div>
    }
}

#[component]
fn ReseedItemsTable(items: Vec<DryRunPreviewItemInfo>) -> impl IntoView {
    if items.is_empty() {
        return view! {
            <p class="text-muted">
                "没有可展示的种子明细。旧版运行日志可能只有汇总数字，没有逐条结果；重新跑一次任务即可生成明细。"
            </p>
        }
        .into_any();
    }

    view! {
        <div class="table-wrap">
            <table class="stats-table stats-table--resizable">
                <thead>
                    <tr>
                        <ResizableTh col_key="reseed-items-site" default_width=120>
                            "站点"
                        </ResizableTh>
                        <ResizableTh col_key="reseed-items-title" default_width=240>
                            "识别到的种子"
                        </ResizableTh>
                        <ResizableTh col_key="reseed-items-link" default_width=90>
                            "种子链接"
                        </ResizableTh>
                        <ResizableTh col_key="reseed-items-path" default_width=260>
                            "本地目录"
                        </ResizableTh>
                        <ResizableTh col_key="reseed-items-size" default_width=100>
                            "目录大小"
                        </ResizableTh>
                        <ResizableTh
                            col_key="reseed-items-tid"
                            default_width=100
                            class="table-col--secondary"
                        >
                            "Torrent ID"
                        </ResizableTh>
                        <ResizableTh
                            col_key="reseed-items-hash"
                            default_width=140
                            class="table-col--secondary"
                        >
                            "Pieces Hash"
                        </ResizableTh>
                    </tr>
                </thead>
                <tbody>
                    {items
                        .into_iter()
                        .map(|item| {
                            let title = item
                                .title
                                .clone()
                                .filter(|t| !t.is_empty())
                                .unwrap_or_else(|| {
                                    item.torrent_id
                                        .map(|id| format!("Torrent #{id}"))
                                        .unwrap_or_else(|| item.pieces_hash.clone())
                                });
                            let title_attr = item.title.clone().unwrap_or_default();
                            let size = item
                                .total_size
                                .map(format_bytes)
                                .unwrap_or_else(|| "-".into());
                            let tid = item
                                .torrent_id
                                .map(|v| v.to_string())
                                .unwrap_or_else(|| "-".into());
                            let hash_full = item.pieces_hash.clone();
                            let hash = truncate_utf8(&item.pieces_hash, 12).to_string();
                            let detail_url = item.detail_url.clone();
                            let save_path_title = item.save_path.clone();
                            let save_path = item.save_path;
                            view! {
                                <tr>
                                    <td>{item.site_name}</td>
                                    <td title=title_attr>{title}</td>
                                    <td>
                                        {match detail_url {
                                            Some(url) if !url.is_empty() => {
                                                let url_title = url.clone();
                                                view! {
                                                    <a
                                                        href=url
                                                        target="_blank"
                                                        rel="noopener noreferrer"
                                                        title=url_title
                                                    >
                                                        "打开"
                                                    </a>
                                                }
                                                .into_any()
                                            }
                                            _ => view! {
                                                <span class="text-muted" title="无公开详情链接">
                                                    "-"
                                                </span>
                                            }
                                            .into_any(),
                                        }}
                                    </td>
                                    <td class="text-muted" title=save_path_title>
                                        {save_path}
                                    </td>
                                    <td>{size}</td>
                                    <td class="text-muted table-col--secondary">{tid}</td>
                                    <td class="text-muted table-col--secondary" title=hash_full>
                                        {hash}
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
}
