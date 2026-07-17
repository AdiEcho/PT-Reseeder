use crate::components::confirm_modal::ConfirmModal;
use crate::components::empty_state::EmptyState;
use crate::components::toast::{show_toast, ToastType};
use crate::server_fns::{
    create_task, delete_task, get_downloaders, get_folders, get_sites, get_task_logs, get_tasks,
    trigger_task, DownloaderInfo, FolderInfo, SiteInfo, TaskInfo, TaskLogInfo,
};
use leptos::ev;
use leptos::prelude::*;
use leptos_router::components::A;

fn status_class(status: &str) -> &'static str {
    match status {
        "running" => "text-blue",
        "paused" => "text-yellow",
        "error" => "text-red",
        _ => "text-muted", // idle
    }
}

fn truncate_utf8(value: &str, max_bytes: usize) -> &str {
    let mut end = value.len().min(max_bytes);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

fn toggle_id(list: &mut Vec<i64>, id: i64) {
    if let Some(i) = list.iter().position(|x| *x == id) {
        list.remove(i);
    } else {
        list.push(id);
    }
}

fn association_summary(task: &TaskInfo) -> String {
    let mut parts = Vec::new();
    if !task.site_ids.is_empty() {
        parts.push(format!("站点 {}", task.site_ids.len()));
    }
    if !task.source_downloader_ids.is_empty() {
        parts.push(format!("源下载器 {}", task.source_downloader_ids.len()));
    }
    if !task.folder_ids.is_empty() {
        parts.push(format!("文件夹 {}", task.folder_ids.len()));
    }
    if task.destination_downloader_id.is_some() {
        parts.push("目标下载器".to_string());
    }
    if parts.is_empty() {
        "未配置关联".to_string()
    } else {
        parts.join(" · ")
    }
}

#[component]
pub fn TasksPage() -> impl IntoView {
    let (version, set_version) = signal(0u64);
    let (show_form, set_show_form) = signal(false);

    let tasks = Resource::new(move || version.get(), |_| get_tasks());
    let sites = Resource::new(
        move || show_form.get(),
        |open| async move {
            if open {
                get_sites().await.ok()
            } else {
                None
            }
        },
    );
    let downloaders = Resource::new(
        move || show_form.get(),
        |open| async move {
            if open {
                get_downloaders().await.ok()
            } else {
                None
            }
        },
    );
    let folders = Resource::new(
        move || show_form.get(),
        |open| async move {
            if open {
                get_folders().await.ok()
            } else {
                None
            }
        },
    );
    let (confirm_delete, set_confirm_delete) = signal(None::<(i64, String)>);

    // --- Create-task form state ---
    let (name, set_name) = signal(String::new());
    let (task_type, set_task_type) = signal("reseed".to_string());
    let (trigger_type, set_trigger_type) = signal("manual".to_string());
    let (cron_expr, set_cron_expr) = signal(String::new());
    let (site_ids, set_site_ids) = signal(Vec::<i64>::new());
    let (folder_ids, set_folder_ids) = signal(Vec::<i64>::new());
    let (source_downloader_ids, set_source_downloader_ids) = signal(Vec::<i64>::new());
    let (destination_downloader_id, set_destination_downloader_id) = signal(None::<i64>);
    let (form_error, set_form_error) = signal(None::<String>);
    let (name_error, set_name_error) = signal(None::<String>);
    let (cron_error, set_cron_error) = signal(None::<String>);
    let (reseed_error, set_reseed_error) = signal(None::<String>);
    let (submitting, set_submitting) = signal(false);

    let reset_form = move || {
        set_name.set(String::new());
        set_task_type.set("reseed".to_string());
        set_trigger_type.set("manual".to_string());
        set_cron_expr.set(String::new());
        set_site_ids.set(Vec::new());
        set_folder_ids.set(Vec::new());
        set_source_downloader_ids.set(Vec::new());
        set_destination_downloader_id.set(None);
        set_name_error.set(None);
        set_cron_error.set(None);
        set_reseed_error.set(None);
        set_form_error.set(None);
    };

    let on_create = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        if submitting.get_untracked() {
            return;
        }
        let n = name.get_untracked();
        let tt = task_type.get_untracked();
        let tg = trigger_type.get_untracked();
        let mut selected_sites = site_ids.get_untracked();
        let mut selected_folders = folder_ids.get_untracked();
        let mut selected_sources = source_downloader_ids.get_untracked();
        let mut selected_destination = destination_downloader_id.get_untracked();
        set_name_error.set(None);
        set_cron_error.set(None);
        set_reseed_error.set(None);
        set_form_error.set(None);

        if n.trim().is_empty() {
            set_name_error.set(Some("任务名称不能为空。".into()));
            return;
        }
        let cron = if tg == "cron" {
            let c = cron_expr.get_untracked();
            if c.trim().is_empty() {
                set_cron_error.set(Some("Cron 触发器必须填写 Cron 表达式。".into()));
                return;
            }
            Some(c)
        } else {
            None
        };

        if tt == "reseed" {
            if selected_sites.is_empty() {
                set_reseed_error.set(Some("请至少选择一个站点。".into()));
                return;
            }
            if selected_sources.is_empty() && selected_folders.is_empty() {
                set_reseed_error.set(Some(
                    "请至少选择一种种子来源（下载器或文件夹，可同时选择）。".into(),
                ));
                return;
            }
            if selected_destination.is_none() {
                set_reseed_error.set(Some("请选择目标下载器。".into()));
                return;
            }
        } else {
            selected_sites.clear();
            selected_folders.clear();
            selected_sources.clear();
            selected_destination = None;
        }

        set_submitting.set(true);
        // Collapse immediately on submit start for responsive UX; reopen only on hard failure.
        set_show_form.set(false);
        leptos::task::spawn_local(async move {
            match create_task(
                n,
                tt,
                tg,
                cron,
                selected_sites,
                selected_folders,
                selected_sources,
                selected_destination,
            )
            .await
            {
                Ok(_) => {
                    show_toast("任务创建成功", ToastType::Success);
                    reset_form();
                    set_version.update(|v| *v += 1);
                }
                Err(e) => {
                    // Reopen so the user can correct and resubmit.
                    set_show_form.set(true);
                    show_toast(format!("创建失败：{e}"), ToastType::Error);
                    set_form_error.set(Some(format!("{e}")));
                }
            }
            set_submitting.set(false);
        });
    };

    view! {
        <div class="dashboard">
            <div class="dashboard-header">
                <h1>"任务管理"</h1>
                <button
                    class="btn btn-primary"
                    on:click=move |_| {
                        set_show_form.update(|v| {
                            *v = !*v;
                            if !*v {
                                reset_form();
                            }
                        });
                    }
                >
                    {move || if show_form.get() { "取消" } else { "创建任务" }}
                </button>
            </div>

            // --- Create Task Form (collapsible) ---
            {move || {
                if show_form.get() {
                    view! {
                        <div class="form-section">
                            <h2>"创建任务"</h2>
                            <form class="inline-form" on:submit=on_create>
                                <label>
                                    "名称" <span class="required">"*"</span>
                                    <input
                                        type="text"
                                        placeholder="任务名称"
                                        prop:value=move || name.get()
                                        on:input=move |ev| {
                                            set_name.set(event_target_value(&ev));
                                            set_name_error.set(None);
                                        }
                                    />
                                    {move || name_error.get().map(|e| view! { <p class="field-error">{e}</p> })}
                                </label>
                                <label>
                                    "类型"
                                    <select
                                        prop:value=move || task_type.get()
                                        on:change=move |ev| {
                                            set_task_type.set(event_target_value(&ev));
                                            set_reseed_error.set(None);
                                        }
                                    >
                                        <option value="reseed">"辅种"</option>
                                        <option value="repost">"转种"</option>
                                        <option value="sync_stats">"数据同步"</option>
                                    </select>
                                </label>
                                <label>
                                    "触发方式"
                                    <select
                                        prop:value=move || trigger_type.get()
                                        on:change=move |ev| {
                                            set_trigger_type.set(event_target_value(&ev));
                                            set_cron_error.set(None);
                                        }
                                    >
                                        <option value="manual">"手动"</option>
                                        <option value="cron">"定时"</option>
                                        <option value="file_watch">"文件监控"</option>
                                    </select>
                                </label>
                                {move || {
                                    if trigger_type.get() == "cron" {
                                        Some(
                                            view! {
                                                <label>
                                                    "Cron 表达式" <span class="required">"*"</span>
                                                    <input
                                                        type="text"
                                                        placeholder="0 */5 * * * *"
                                                        prop:value=move || cron_expr.get()
                                                        on:input=move |ev| {
                                                            set_cron_expr.set(event_target_value(&ev));
                                                            set_cron_error.set(None);
                                                        }
                                                    />
                                                    <p class="field-hint">
                                                        "使用 6 段表达式：秒 分 时 日 月 周。例如每 5 分钟：0 */5 * * * *"
                                                    </p>
                                                    {move || cron_error.get().map(|e| view! { <p class="field-error">{e}</p> })}
                                                </label>
                                            },
                                        )
                                    } else {
                                        None
                                    }
                                }}
                                {move || {
                                    if task_type.get() == "reseed" {
                                        Some(
                                            view! {
                                                // Use div.form-group (not nested <label>) so multi-select
                                                // checkboxes for sites/downloaders/folders can be chosen together.
                                                <div class="reseed-config form-grid">
                                                    <div class="form-group reseed-field">
                                                        <span class="field-label">
                                                            "站点" <span class="required">"*"</span>
                                                        </span>
                                                        <Suspense fallback=move || view! { <p class="text-muted">"正在加载站点..."</p> }>
                                                            {move || {
                                                                sites.get().flatten().map(|list| {
                                                                    let enabled: Vec<SiteInfo> = list.into_iter().filter(|s| s.enabled).collect();
                                                                    if enabled.is_empty() {
                                                                        return view! {
                                                                            <p class="field-hint">"暂无启用站点，请先在站点管理中添加。"</p>
                                                                        }.into_any();
                                                                    }
                                                                    view! {
                                                                        <div class="checkbox-list" role="group" aria-label="辅种站点">
                                                                            {enabled.into_iter().map(|site| {
                                                                                let id = site.id;
                                                                                let label = site.name.clone();
                                                                                view! {
                                                                                    <label class="checkbox-item">
                                                                                        <input
                                                                                            type="checkbox"
                                                                                            prop:checked=move || site_ids.get().contains(&id)
                                                                                            on:change=move |_| {
                                                                                                set_site_ids.update(|list| toggle_id(list, id));
                                                                                                set_reseed_error.set(None);
                                                                                            }
                                                                                        />
                                                                                        <span>{label}</span>
                                                                                    </label>
                                                                                }
                                                                            }).collect::<Vec<_>>()}
                                                                        </div>
                                                                    }.into_any()
                                                                })
                                                            }}
                                                        </Suspense>
                                                    </div>
                                                    <div class="form-group reseed-field">
                                                        <span class="field-label">"源下载器"</span>
                                                        <Suspense fallback=move || view! { <p class="text-muted">"正在加载下载器..."</p> }>
                                                            {move || {
                                                                downloaders.get().flatten().map(|list| {
                                                                    let enabled: Vec<DownloaderInfo> = list.into_iter().filter(|d| d.enabled).collect();
                                                                    if enabled.is_empty() {
                                                                        return view! {
                                                                            <p class="field-hint">"暂无启用下载器。"</p>
                                                                        }.into_any();
                                                                    }
                                                                    view! {
                                                                        <div class="checkbox-list" role="group" aria-label="源下载器">
                                                                            {enabled.into_iter().map(|dl| {
                                                                                let id = dl.id;
                                                                                let label = format!("{} (#{})", dl.name, dl.id);
                                                                                view! {
                                                                                    <label class="checkbox-item">
                                                                                        <input
                                                                                            type="checkbox"
                                                                                            prop:checked=move || source_downloader_ids.get().contains(&id)
                                                                                            on:change=move |_| {
                                                                                                set_source_downloader_ids.update(|list| toggle_id(list, id));
                                                                                                set_reseed_error.set(None);
                                                                                            }
                                                                                        />
                                                                                        <span>{label}</span>
                                                                                    </label>
                                                                                }
                                                                            }).collect::<Vec<_>>()}
                                                                        </div>
                                                                    }.into_any()
                                                                })
                                                            }}
                                                        </Suspense>
                                                    </div>
                                                    <div class="form-group reseed-field">
                                                        <span class="field-label">"扫描文件夹"</span>
                                                        <Suspense fallback=move || view! { <p class="text-muted">"正在加载文件夹..."</p> }>
                                                            {move || {
                                                                folders.get().flatten().map(|list| {
                                                                    let enabled: Vec<FolderInfo> = list
                                                                        .into_iter()
                                                                        .filter(|f| f.enabled && f.scan_mode == "local")
                                                                        .collect();
                                                                    if enabled.is_empty() {
                                                                        return view! {
                                                                            <p class="field-hint">"暂无启用的本机文件夹。"</p>
                                                                        }.into_any();
                                                                    }
                                                                    view! {
                                                                        <div class="checkbox-list" role="group" aria-label="扫描文件夹">
                                                                            {enabled.into_iter().map(|folder| {
                                                                                let id = folder.id;
                                                                                let label = folder.path.clone();
                                                                                view! {
                                                                                    <label class="checkbox-item">
                                                                                        <input
                                                                                            type="checkbox"
                                                                                            prop:checked=move || folder_ids.get().contains(&id)
                                                                                            on:change=move |_| {
                                                                                                set_folder_ids.update(|list| toggle_id(list, id));
                                                                                                set_reseed_error.set(None);
                                                                                            }
                                                                                        />
                                                                                        <span>{label}</span>
                                                                                    </label>
                                                                                }
                                                                            }).collect::<Vec<_>>()}
                                                                        </div>
                                                                    }.into_any()
                                                                })
                                                            }}
                                                        </Suspense>
                                                        <p class="field-hint">"下载器与文件夹可同时选择，至少选一种来源"</p>
                                                    </div>
                                                    <div class="form-group reseed-field">
                                                        <span class="field-label">
                                                            "目标下载器" <span class="required">"*"</span>
                                                        </span>
                                                        <Suspense fallback=move || view! {
                                                            <select class="input" disabled=true>
                                                                <option>"加载下载器..."</option>
                                                            </select>
                                                        }>
                                                            {move || {
                                                                downloaders.get().flatten().map(|list| {
                                                                    let enabled: Vec<DownloaderInfo> = list.into_iter().filter(|d| d.enabled).collect();
                                                                    view! {
                                                                        <select
                                                                            class="input"
                                                                            prop:value=move || destination_downloader_id.get().map(|id| id.to_string()).unwrap_or_default()
                                                                            on:change=move |ev| {
                                                                                let raw = event_target_value(&ev);
                                                                                let parsed = raw.trim().parse::<i64>().ok();
                                                                                set_destination_downloader_id.set(parsed);
                                                                                set_reseed_error.set(None);
                                                                            }
                                                                        >
                                                                            <option value="">"请选择目标下载器"</option>
                                                                            {enabled.into_iter().map(|dl| {
                                                                                let id = dl.id.to_string();
                                                                                let label = format!("{} (#{})", dl.name, dl.id);
                                                                                view! {
                                                                                    <option value=id.clone()>{label}</option>
                                                                                }
                                                                            }).collect::<Vec<_>>()}
                                                                        </select>
                                                                    }.into_any()
                                                                })
                                                            }}
                                                        </Suspense>
                                                    </div>
                                                    {move || reseed_error.get().map(|e| view! { <p class="field-error">{e}</p> })}
                                                </div>
                                            },
                                        )
                                    } else {
                                        None
                                    }
                                }}
                                <button type="submit" disabled=move || submitting.get()>
                                    {move || if submitting.get() { "创建中..." } else { "创建" }}
                                </button>
                            </form>
                            {move || {
                                form_error
                                    .get()
                                    .map(|e| {
                                        view! { <p class="field-error">{e}</p> }
                                    })
                            }}
                        </div>
                    }
                        .into_any()
                } else {
                    view! { <div></div> }.into_any()
                }
            }}

            // Page-level delete confirmation so modal mounts outside <tbody>.
            {move || {
                confirm_delete.get().map(|(task_id, name)| {
                    view! {
                        <ConfirmModal
                            title="确认删除"
                            message=format!("确定要删除任务「{name}」吗？此操作不可撤销。")
                            on_confirm=move || {
                                set_confirm_delete.set(None);
                                leptos::task::spawn_local(async move {
                                    match delete_task(task_id).await {
                                        Ok(_) => show_toast("任务已删除", ToastType::Success),
                                        Err(e) => show_toast(format!("删除失败：{e}"), ToastType::Error),
                                    }
                                    set_version.update(|v| *v += 1);
                                });
                            }
                            on_cancel=move || set_confirm_delete.set(None)
                            confirm_label="确认删除"
                            danger=true
                        />
                    }
                })
            }}

            // --- Tasks Table ---
            <div class="stats-table-section">
                <h2>"任务列表"</h2>
                <Suspense fallback=move || {
                    view! { <p>"正在加载任务..."</p> }
                }>
                    {move || {
                        tasks
                            .get()
                            .map(|result| {
                                match result {
                                    Err(e) => {
                                        view! {
                                            <div class="load-error">
                                                <span>{format!("任务加载失败：{e}")}</span>
                                                <button
                                                    class="btn btn--sm btn--outline"
                                                    on:click=move |_| set_version.update(|v| *v += 1)
                                                >
                                                    "重试"
                                                </button>
                                            </div>
                                        }
                                            .into_any()
                                    }
                                    Ok(list) if list.is_empty() => {
                                        view! { <EmptyState icon="⏱" message="尚未配置任何任务。" /> }.into_any()
                                    }
                                    Ok(list) => {
                                        view! {
                                            <div class="table-wrap">
                                                <table class="stats-table">
                                                    <thead>
                                                        <tr>
                                                            <th>"名称"</th>
                                                            <th>"类型"</th>
                                                            <th>"触发方式"</th>
                                                            <th class="col-secondary">"Cron"</th>
                                                            <th>"状态"</th>
                                                            <th class="col-secondary">"上次运行"</th>
                                                            <th class="col-secondary">"下次运行"</th>
                                                            <th>"运行次数"</th>
                                                            <th>"操作"</th>
                                                        </tr>
                                                    </thead>
                                                    <tbody>
                                                        {list
                                                            .into_iter()
                                                            .map(|task| {
                                                                view! {
                                                                    <TaskRow
                                                                        task=task
                                                                        on_change=move || set_version.update(|v| *v += 1)
                                                                        on_request_delete=move |id: i64, name: String| {
                                                                            set_confirm_delete.set(Some((id, name)));
                                                                        }
                                                                    />
                                                                }
                                                            })
                                                            .collect::<Vec<_>>()}
                                                    </tbody>
                                                </table>
                                            </div>
                                        }
                                            .into_any()
                                    }
                                }
                            })
                    }}
                </Suspense>
            </div>
        </div>
    }
}

#[component]
fn TaskRow<F, G>(task: TaskInfo, on_change: F, on_request_delete: G) -> impl IntoView
where
    F: Fn() + Copy + Send + Sync + 'static,
    G: Fn(i64, String) + Copy + 'static,
{
    let task_id = task.id;
    #[cfg(target_arch = "wasm32")]
    let initial_run_count = task.run_count;
    let task_name = task.name.clone();
    let assoc_summary = association_summary(&task);
    let (expanded, set_expanded) = signal(false);
    let (acting, set_acting) = signal(false);

    let logs = Resource::new(
        move || expanded.get(),
        move |open| async move {
            if open {
                get_task_logs(task_id).await.ok()
            } else {
                None
            }
        },
    );

    let on_trigger = move |ev: ev::MouseEvent| {
        ev.stop_propagation();
        set_acting.set(true);
        leptos::task::spawn_local(async move {
            match trigger_task(task_id).await {
                Ok(_) => {
                    show_toast("任务已触发", ToastType::Success);
                    on_change();
                    #[cfg(not(target_arch = "wasm32"))]
                    set_acting.set(false);
                    #[cfg(target_arch = "wasm32")]
                    {
                        let mut saw_running = false;
                        for _ in 0..300 {
                            gloo_timers::future::TimeoutFuture::new(1_000).await;
                            let Some(task) = (match get_tasks().await {
                                Ok(tasks) => tasks.into_iter().find(|task| task.id == task_id),
                                Err(_) => continue,
                            }) else {
                                on_change();
                                break;
                            };
                            if task.status == "running" && !saw_running {
                                saw_running = true;
                                on_change();
                            }
                            if task.status != "running" && task.run_count > initial_run_count {
                                on_change();
                                break;
                            }
                        }
                        set_acting.set(false);
                    }
                }
                Err(e) => {
                    show_toast(format!("触发失败：{e}"), ToastType::Error);
                    set_acting.set(false);
                }
            }
        });
    };

    let on_delete = move |ev: ev::MouseEvent| {
        ev.stop_propagation();
        on_request_delete(task_id, task_name.clone());
    };

    let sc = status_class(&task.status);
    let status_label = match task.status.as_str() {
        "running" => "运行中",
        "paused" => "已暂停",
        "error" => "错误",
        _ => "空闲",
    };
    let last_run = task
        .last_run_at
        .as_deref()
        .map(|s| truncate_utf8(s, 16))
        .unwrap_or("-")
        .to_string();
    let next_run = task
        .next_run_at
        .as_deref()
        .map(|s| truncate_utf8(s, 16))
        .unwrap_or("-")
        .to_string();
    let cron_display = task.cron_expression.clone().unwrap_or_else(|| "-".into());
    let cron_title = cron_display.clone();

    view! {
        <>
            <tr
                class="clickable-row"
                on:click=move |_| set_expanded.update(|v| *v = !*v)
            >
                <td>
                    <div>{task.name.clone()}</div>
                    <div class="text-muted table-subtext">{assoc_summary}</div>
                </td>
                <td>{match task.task_type.as_str() {
                    "reseed" => "辅种".to_string(),
                    "repost" => "转种".to_string(),
                    "sync_stats" => "数据同步".to_string(),
                    other => other.to_string(),
                }}</td>
                <td>{match task.trigger_type.as_str() {
                    "manual" => "手动".to_string(),
                    "cron" => "定时".to_string(),
                    "file_watch" => "文件监控".to_string(),
                    other => other.to_string(),
                }}</td>
                <td class="text-muted col-secondary table-col--secondary" title=cron_title>{cron_display}</td>
                <td class=sc>{status_label}</td>
                <td class="text-muted col-secondary">{last_run}</td>
                <td class="text-muted col-secondary">{next_run}</td>
                <td>{task.run_count}</td>
                <td class="action-cell">
                    <button
                        class="btn btn--sm btn--primary"
                        disabled=move || acting.get()
                        on:click=on_trigger
                    >
                        "立即运行"
                    </button>
                    <A
                        href=format!("/logs?task_id={task_id}")
                        attr:class="btn btn--sm btn--outline"
                        on:click=move |ev| ev.stop_propagation()
                    >
                        "日志"
                    </A>
                    <button
                        class="btn btn--sm btn--danger"
                        disabled=move || acting.get()
                        on:click=on_delete
                    >
                        "删除"
                    </button>
                </td>
            </tr>
            {move || {
                if expanded.get() {
                    Some(
                        view! {
                            <tr class="expand-row">
                                <td colspan="9">
                                    <Suspense fallback=move || {
                                        view! { <p class="text-muted">"正在加载日志..."</p> }
                                    }>
                                        {move || {
                                            logs.get()
                                                .flatten()
                                                .map(|log_list| {
                                                    view! { <TaskLogTable logs=log_list /> }
                                                })
                                        }}
                                    </Suspense>
                                </td>
                            </tr>
                        },
                    )
                } else {
                    None
                }
            }}
        </>
    }
}

#[component]
fn TaskLogTable(logs: Vec<TaskLogInfo>) -> impl IntoView {
    if logs.is_empty() {
        return view! { <p class="text-muted">"该任务暂无日志。"</p> }.into_any();
    }

    view! {
        <div class="table-wrap nested-table">
            <table class="stats-table">
                <thead>
                    <tr>
                        <th>"状态"</th>
                        <th>"匹配数"</th>
                        <th>"成功"</th>
                        <th>"失败"</th>
                        <th>"耗时"</th>
                        <th>"时间"</th>
                    </tr>
                </thead>
                <tbody>
                    {logs
                        .into_iter()
                        .map(|log| {
                            let lsc = match log.status.as_str() {
                                "success" => "text-green",
                                "failed" | "error" => "text-red",
                                "running" => "text-blue",
                                _ => "text-muted",
                            };
                            let status_label = match log.status.as_str() {
                                "success" => "成功",
                                "failed" | "error" => "失败",
                                "running" => "运行中",
                                _ => "未知",
                            };
                            let duration = log
                                .duration_ms
                                .map(|ms| format!("{:.1}秒", ms as f64 / 1000.0))
                                .unwrap_or_else(|| "-".into());
                            let ts = truncate_utf8(&log.created_at, 16).to_string();
                            view! {
                                <tr>
                                    <td class=lsc>{status_label}</td>
                                    <td>{log.matched_count}</td>
                                    <td class="text-green">{log.succeeded_count}</td>
                                    <td class="text-red">{log.failed_count}</td>
                                    <td class="text-muted">{duration}</td>
                                    <td class="text-muted">{ts}</td>
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

#[cfg(test)]
mod tests {
    use super::truncate_utf8;

    #[test]
    fn truncate_utf8_respects_byte_limit_and_char_boundaries() {
        assert_eq!(truncate_utf8("2026-07-16 12:34", 16), "2026-07-16 12:34");
        assert_eq!(truncate_utf8("短文本", 16), "短文本");
        assert_eq!(truncate_utf8("123456789012345中", 16), "123456789012345");
        assert_eq!(truncate_utf8("中文文本", 4), "中");
    }
}
