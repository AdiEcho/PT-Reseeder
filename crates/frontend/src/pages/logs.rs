use crate::components::async_view::AsyncView;
use crate::components::empty_state::EmptyState;
use crate::server_fns::{
    get_log_files, get_logs, log_entry_matches_task_id, LogEntry, LogFileInfo, LogPage,
};
use crate::utils::{format_local_timestamp, local_tz_offset_minutes};
use crate::ws::use_logs_ws;
use leptos::prelude::*;
use leptos_router::{components::A, hooks::use_query_map};

fn parse_task_id(value: Option<String>) -> Option<i64> {
    value
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|task_id| *task_id > 0)
}

/// Advances the log viewer to the next page, stopping at the last page that the
/// current result set actually has.
///
/// Kept out of the `view!` body and compiled away on the server: during SSR the
/// `on:click` closure is invoked while the tree is being built, so writing
/// `current_page` there would advance the page nobody asked for. The server
/// would then serialise a different page than the one the client hydrates,
/// leaving the two with different row counts — an unrecoverable hydration
/// mismatch that takes down the whole WASM runtime, not just this page.
fn advance_page(set_current_page: WriteSignal<usize>, total_pages: Memo<usize>) {
    #[cfg(target_arch = "wasm32")]
    set_current_page.update(|page| {
        if *page < total_pages.get_untracked() {
            *page += 1;
        }
    });

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (set_current_page, total_pages);
    }
}

/// Steps the log viewer back one page, stopping at the first page.
///
/// Server-side no-op for the same reason as [`advance_page`].
fn retreat_page(set_current_page: WriteSignal<usize>) {
    #[cfg(target_arch = "wasm32")]
    set_current_page.update(|page| {
        if *page > 1 {
            *page -= 1;
        }
    });

    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = set_current_page;
    }
}

#[component]
pub fn LogsPage() -> impl IntoView {
    let query = use_query_map();
    let task_id = Memo::new(move |_| parse_task_id(query.read().get("task_id")));
    let (version, set_version) = signal(0u32);
    let (selected_file, set_selected_file) = signal(None::<String>);
    let (level_filter, set_level_filter) = signal(String::new());
    // Immediate input value; debounced into `keyword` to avoid refetch on every keystroke.
    let (keyword_input, set_keyword_input) = signal(String::new());
    let (keyword, set_keyword) = signal(String::new());
    let (keyword_seq, set_keyword_seq) = signal(0u64);
    let (current_page, set_current_page) = signal(1usize);
    let (auto_scroll, set_auto_scroll) = signal(true);
    let (live_lines, set_live_lines) = signal(Vec::<LogEntry>::new());

    // Viewer timezone offset, applied to the UTC timestamps the log lines carry.
    //
    // Starts as `None` — meaning "render the raw UTC string" — and is only
    // filled in from an effect after mount. The server has no way to know the
    // viewer's timezone, so anything else would make the first client render
    // disagree with the server markup. Leptos hydrates text nodes by adopting
    // the server's text without comparing it, so such a disagreement is not a
    // loud failure: the cell would keep showing UTC while the view believed it
    // had already written local time, and no later update would fix it.
    let (tz_offset, set_tz_offset) = signal(None::<i32>);
    Effect::new(move |_| {
        set_tz_offset.set(local_tz_offset_minutes());
    });

    // Debounce keyword updates (~400ms). Only the latest generation may commit.
    Effect::new(move |_| {
        let value = keyword_input.get();
        set_keyword_seq.update(|n| *n += 1);
        let my_seq = keyword_seq.get_untracked();
        #[cfg(target_arch = "wasm32")]
        {
            leptos::task::spawn_local(async move {
                gloo_timers::future::TimeoutFuture::new(400).await;
                // The page may have been navigated away from while this task was
                // sleeping, disposing every signal it captured. Reading a disposed
                // signal panics, so bail out instead of asserting they still live.
                let Some(latest_seq) = keyword_seq.try_get_untracked() else {
                    return;
                };
                let Some(committed) = keyword.try_get_untracked() else {
                    return;
                };
                if latest_seq == my_seq && committed != value {
                    let _ = set_keyword.try_set(value);
                    let _ = set_current_page.try_set(1);
                }
            });
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = my_seq;
            if keyword.get_untracked() != value {
                set_keyword.set(value);
                set_current_page.set(1);
            }
        }
    });

    let log_files = Resource::new(move || version.get(), |_| get_log_files());

    let logs = Resource::new(
        move || {
            (
                version.get(),
                selected_file.get(),
                current_page.get(),
                level_filter.get(),
                keyword.get(),
                task_id.get(),
            )
        },
        move |(_, file, page, level, kw, task_id)| {
            let level_opt = if level.is_empty() { None } else { Some(level) };
            let kw_opt = if kw.is_empty() { None } else { Some(kw) };
            get_logs(file, Some(page), Some(100), level_opt, kw_opt, task_id)
        },
    );

    // Page count for the current result set, derived from the resource rather
    // than captured inside the render closure. A captured copy goes stale as
    // soon as the resource reloads, which previously let "下一页" advance past
    // the last page and desynchronise SSR from hydration.
    let total_pages = Memo::new(move |_| {
        logs.get()
            .and_then(|result| result.ok())
            .map(|page| page.total_lines.div_ceil(page.page_size.max(1)).max(1))
            .unwrap_or(1)
    });

    // Keep the requested page inside the range the current result set spans, so
    // the viewer never asks the server for a page that does not exist.
    Effect::new(move |_| {
        let last = total_pages.get();
        if current_page.get() > last {
            set_current_page.set(last);
        }
    });

    // WebSocket live log subscription
    let ws_data = use_logs_ws();

    // Append live log lines from WebSocket
    Effect::new(move |_| {
        if let Some(entry) = ws_data.get() {
            if auto_scroll.get_untracked() {
                let level_f = level_filter.get_untracked();
                let kw_f = keyword.get_untracked();
                let task_id_f = task_id.get_untracked();

                let level_ok = level_f.is_empty() || entry.level.eq_ignore_ascii_case(&level_f);
                let task_ok = task_id_f
                    .map(|id| log_entry_matches_task_id(&entry, id))
                    .unwrap_or(true);
                let kw_ok = kw_f.is_empty()
                    || entry.message.contains(&kw_f)
                    || entry.target.contains(&kw_f);

                if level_ok && task_ok && kw_ok {
                    set_live_lines.update(|lines| {
                        lines.insert(0, entry);
                        if lines.len() > 500 {
                            lines.truncate(500);
                        }
                    });
                }
            }
        }
    });

    // Clear live lines when filters change or page navigated
    Effect::new(move |_| {
        let _ = (
            level_filter.get(),
            keyword.get(),
            task_id.get(),
            current_page.get(),
        );
        set_live_lines.set(Vec::new());
    });

    let refetch = move || {
        set_version.update(|v| *v += 1);
        set_live_lines.set(Vec::new());
    };

    view! {
        <div class="dashboard">
            <div class="dashboard-header">
                <h1>"日志"</h1>
                {move || {
                    task_id.get().map(|id| {
                        view! {
                            <div class="log-task-filter">
                                <span>{format!("任务 #{id}")}</span>
                                <A href="/logs" attr:class="btn btn--sm btn--outline">
                                    "清除筛选"
                                </A>
                            </div>
                        }
                    })
                }}
            </div>

            // Toolbar
            <div class="log-toolbar">
                // File selector
                <Suspense fallback=|| ()>
                    {move || {
                        log_files
                            .get()
                            .map(|result| {
                                match result {
                                    Err(e) => view! {
                                        <span class="field-error">{format!("日志文件列表加载失败：{e}")}</span>
                                    }
                                    .into_any(),
                                    Ok(files) => {
                                        view! {
                                            <select
                                                class="input"
                                                on:change=move |ev| {
                                                    let val = event_target_value(&ev);
                                                    if val.is_empty() {
                                                        set_selected_file.set(None);
                                                    } else {
                                                        set_selected_file.set(Some(val));
                                                    }
                                                    set_current_page.set(1);
                                                    refetch();
                                                }
                                            >
                                                <option value="">"最新日志"</option>
                                                {files
                                                    .into_iter()
                                                    .map(|f: LogFileInfo| {
                                                        let name = f.filename.clone();
                                                        let size_kb = f.size / 1024;
                                                        let label = format!(
                                                            "{} ({}KB)",
                                                            f.filename,
                                                            size_kb,
                                                        );
                                                        view! {
                                                            <option value=name>{label}</option>
                                                        }
                                                    })
                                                    .collect::<Vec<_>>()}
                                            </select>
                                        }
                                            .into_any()
                                    }
                                }
                            })
                    }}
                </Suspense>

                // Level filter
                <select
                    class="input"
                    on:change=move |ev| {
                        set_level_filter.set(event_target_value(&ev));
                        set_current_page.set(1);
                    }
                >
                    <option value="">"全部级别"</option>
                    <option value="ERROR">"ERROR"</option>
                    <option value="WARN">"WARN"</option>
                    <option value="INFO">"INFO"</option>
                    <option value="DEBUG">"DEBUG"</option>
                    <option value="TRACE">"TRACE"</option>
                </select>

                // Keyword search (debounced)
                <input
                    type="text"
                    class="input"
                    placeholder="搜索关键词（输入后稍候自动查询）..."
                    prop:value=move || keyword_input.get()
                    on:input=move |ev| {
                        set_keyword_input.set(event_target_value(&ev));
                    }
                />

                // Auto-scroll toggle
                <label class="log-auto-scroll">
                    <input
                        type="checkbox"
                        prop:checked=move || auto_scroll.get()
                        on:change=move |ev| {
                            set_auto_scroll.set(event_target_checked(&ev));
                        }
                    />
                    <span>"实时滚动"</span>
                </label>

                // Refresh button
                <button class="btn btn--gray btn--sm" on:click=move |_| refetch()>
                    "刷新"
                </button>
            </div>

            // Live lines (from WebSocket, newest first)
            {move || {
                let lines = live_lines.get();
                if lines.is_empty() {
                    None
                } else {
                    Some(
                        view! {
                            <div class="stats-table-section">
                                <h2>"实时日志"</h2>
                                <div class="table-wrap">
                                    <table class="stats-table log-table">
                                        <thead>
                                            <tr>
                                                <th>"时间"</th>
                                                <th>"级别"</th>
                                                <th>"来源"</th>
                                                <th>"消息"</th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {lines
                                                .into_iter()
                                                .map(|entry| {
                                                    let level_class = level_css_class(&entry.level);
                                                    let ts = entry.timestamp.clone();
                                                    view! {
                                                        <tr>
                                                            <td class="log-ts">
                                                                {move || {
                                                                    format_local_timestamp(&ts, tz_offset.get())
                                                                }}
                                                            </td>
                                                            <td>
                                                                <span class=level_class>{entry.level.clone()}</span>
                                                            </td>
                                                            <td class="log-target">{entry.target.clone()}</td>
                                                            <td class="log-msg">{entry.message.clone()}</td>
                                                        </tr>
                                                    }
                                                })
                                                .collect::<Vec<_>>()}
                                        </tbody>
                                    </table>
                                </div>
                            </div>
                        },
                    )
                }
            }}

            // Historical logs (from file)
            <AsyncView
                resource=logs
                error_label="日志"
                on_retry=refetch
                render={move |page: LogPage| {
                    if page.entries.is_empty() {
                        return view! {
                            <div class="stats-table-section">
                                <EmptyState icon="📄" message="当前筛选条件下没有日志。" />
                            </div>
                        }
                            .into_any();
                    }
                    let page_label = format!(
                        "历史日志（共 {} 条，第 {}/{} 页）",
                        page.total_lines,
                        page.page,
                        page.total_lines.div_ceil(page.page_size.max(1)).max(1),
                    );
                    view! {
                        <div class="stats-table-section">
                            <h2>{page_label}</h2>
                            <div class="table-wrap">
                                <table class="stats-table log-table">
                                    <thead>
                                        <tr>
                                            <th>"时间"</th>
                                            <th>"级别"</th>
                                            <th>"来源"</th>
                                            <th>"消息"</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {page
                                            .entries
                                            .into_iter()
                                            .map(|entry| {
                                                let level_class = level_css_class(&entry.level);
                                                let ts = entry.timestamp.clone();
                                                view! {
                                                    <tr>
                                                        <td class="log-ts">
                                                            {move || {
                                                                format_local_timestamp(&ts, tz_offset.get())
                                                            }}
                                                        </td>
                                                        <td>
                                                            <span class=level_class>
                                                                {entry.level.clone()}
                                                            </span>
                                                        </td>
                                                        <td class="log-target">
                                                            {entry.target.clone()}
                                                        </td>
                                                        <td class="log-msg">
                                                            {entry.message.clone()}
                                                        </td>
                                                    </tr>
                                                }
                                            })
                                            .collect::<Vec<_>>()}
                                    </tbody>
                                </table>
                            </div>

                            // Pagination
                            <div class="log-pagination">
                                <button
                                    class="btn btn--gray btn--sm"
                                    disabled=move || current_page.get() <= 1
                                    on:click=move |_| retreat_page(set_current_page)
                                >
                                    "上一页"
                                </button>
                                <span class="text-muted">
                                    {move || { format!("第 {} 页", current_page.get()) }}
                                </span>
                                <button
                                    class="btn btn--gray btn--sm"
                                    disabled=move || { current_page.get() >= total_pages.get() }
                                    on:click=move |_| advance_page(set_current_page, total_pages)
                                >
                                    "下一页"
                                </button>
                            </div>
                        </div>
                    }
                        .into_any()
                }}
            />
        </div>
    }
}

fn level_css_class(level: &str) -> &'static str {
    match level.to_uppercase().as_str() {
        "ERROR" => "log-level log-level--error",
        "WARN" => "log-level log-level--warn",
        "INFO" => "log-level log-level--info",
        "DEBUG" => "log-level log-level--debug",
        "TRACE" => "log-level log-level--trace",
        _ => "log-level",
    }
}

#[cfg(test)]
mod tests {
    use super::parse_task_id;

    #[test]
    fn parses_positive_task_id() {
        assert_eq!(parse_task_id(Some("42".into())), Some(42));
        assert_eq!(parse_task_id(Some("0042".into())), Some(42));
    }

    #[test]
    fn rejects_invalid_task_id() {
        assert_eq!(parse_task_id(None), None);
        assert_eq!(parse_task_id(Some("0".into())), None);
        assert_eq!(parse_task_id(Some("-1".into())), None);
        assert_eq!(parse_task_id(Some("42x".into())), None);
    }
}
