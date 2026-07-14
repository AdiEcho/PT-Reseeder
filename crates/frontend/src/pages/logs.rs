use crate::server_fns::{get_log_files, get_logs, LogEntry, LogFileInfo};
use crate::ws::use_logs_ws;
use leptos::prelude::*;

#[component]
pub fn LogsPage() -> impl IntoView {
    let (version, set_version) = signal(0u32);
    let (selected_file, set_selected_file) = signal(None::<String>);
    let (level_filter, set_level_filter) = signal(String::new());
    let (keyword, set_keyword) = signal(String::new());
    let (current_page, set_current_page) = signal(1usize);
    let (auto_scroll, set_auto_scroll) = signal(true);
    let (live_lines, set_live_lines) = signal(Vec::<LogEntry>::new());

    let log_files = Resource::new(move || version.get(), |_| get_log_files());

    let logs = Resource::new(
        move || {
            (
                version.get(),
                selected_file.get(),
                current_page.get(),
                level_filter.get(),
                keyword.get(),
            )
        },
        move |(_, file, page, level, kw)| {
            let level_opt = if level.is_empty() { None } else { Some(level) };
            let kw_opt = if kw.is_empty() { None } else { Some(kw) };
            get_logs(file, Some(page), Some(100), level_opt, kw_opt)
        },
    );

    // WebSocket live log subscription
    let ws_data = use_logs_ws();

    // Append live log lines from WebSocket
    Effect::new(move |_| {
        if let Some(entry) = ws_data.get() {
            if auto_scroll.get_untracked() {
                let level_f = level_filter.get_untracked();
                let kw_f = keyword.get_untracked();

                let level_ok =
                    level_f.is_empty() || entry.level.eq_ignore_ascii_case(&level_f);
                let kw_ok = kw_f.is_empty()
                    || entry.message.contains(&kw_f)
                    || entry.target.contains(&kw_f);

                if level_ok && kw_ok {
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
        let _ = (level_filter.get(), keyword.get(), current_page.get());
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
                                    Err(_) => view! { <span></span> }.into_any(),
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

                // Keyword search
                <input
                    type="text"
                    class="input"
                    placeholder="搜索关键词..."
                    prop:value=move || keyword.get()
                    on:input=move |ev| {
                        set_keyword.set(event_target_value(&ev));
                        set_current_page.set(1);
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
                                                    view! {
                                                        <tr>
                                                            <td class="log-ts">{entry.timestamp.clone()}</td>
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
            <Suspense fallback=move || {
                view! { <p>"正在加载日志..."</p> }
            }>
                {move || {
                    logs.get()
                        .map(|result| {
                            match result {
                                Err(e) => {
                                    view! {
                                        <p class="error">{format!("日志加载失败：{e}")}</p>
                                    }
                                        .into_any()
                                }
                                Ok(page) => {
                                    let total_pages = (page.total_lines + page.page_size - 1)
                                        .max(1) / page.page_size.max(1);
                                    view! {
                                        <div class="stats-table-section">
                                            <h2>
                                                {format!(
                                                    "历史日志（共 {} 条，第 {}/{} 页）",
                                                    page.total_lines,
                                                    page.page,
                                                    total_pages.max(1),
                                                )}
                                            </h2>
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
                                                                let level_class = level_css_class(
                                                                    &entry.level,
                                                                );
                                                                view! {
                                                                    <tr>
                                                                        <td class="log-ts">
                                                                            {entry.timestamp.clone()}
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
                                                    on:click=move |_| {
                                                        set_current_page
                                                            .update(|p| {
                                                                if *p > 1 {
                                                                    *p -= 1
                                                                }
                                                            });
                                                    }
                                                >
                                                    "上一页"
                                                </button>
                                                <span class="text-muted">
                                                    {move || {
                                                        format!("第 {} 页", current_page.get())
                                                    }}
                                                </span>
                                                <button
                                                    class="btn btn--gray btn--sm"
                                                    disabled=move || {
                                                        current_page.get() >= total_pages.max(1)
                                                    }
                                                    on:click=move |_| {
                                                        set_current_page.update(|p| *p += 1);
                                                    }
                                                >
                                                    "下一页"
                                                </button>
                                            </div>
                                        </div>
                                    }
                                        .into_any()
                                }
                            }
                        })
                }}
            </Suspense>
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
