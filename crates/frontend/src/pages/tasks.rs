use crate::components::confirm_modal::ConfirmModal;
use crate::components::empty_state::EmptyState;
use crate::components::toast::{show_toast, ToastType};
use crate::server_fns::{
    create_task, delete_task, get_task_logs, get_tasks, trigger_task, TaskInfo, TaskLogInfo,
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

#[component]
pub fn TasksPage() -> impl IntoView {
    let (version, set_version) = signal(0u64);
    let (show_form, set_show_form) = signal(false);

    let tasks = Resource::new(move || version.get(), |_| get_tasks());
    let (confirm_delete, set_confirm_delete) = signal(None::<(i64, String)>);

    // --- Create-task form state ---
    let (name, set_name) = signal(String::new());
    let (task_type, set_task_type) = signal("reseed".to_string());
    let (trigger_type, set_trigger_type) = signal("manual".to_string());
    let (cron_expr, set_cron_expr) = signal(String::new());
    let (form_error, set_form_error) = signal(None::<String>);
    let (name_error, set_name_error) = signal(None::<String>);
    let (cron_error, set_cron_error) = signal(None::<String>);
    let (submitting, set_submitting) = signal(false);

    let on_create = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let n = name.get_untracked();
        let tt = task_type.get_untracked();
        let tg = trigger_type.get_untracked();
        set_name_error.set(None);
        set_cron_error.set(None);
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
        set_submitting.set(true);
        leptos::task::spawn_local(async move {
            match create_task(n, tt, tg, cron).await {
                Ok(_) => {
                    show_toast("任务创建成功", ToastType::Success);
                    set_name.set(String::new());
                    set_cron_expr.set(String::new());
                    set_show_form.set(false);
                    set_version.update(|v| *v += 1);
                }
                Err(e) => {
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
                    on:click=move |_| set_show_form.update(|v| *v = !*v)
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
                                    <select on:change=move |ev| {
                                        set_task_type.set(event_target_value(&ev));
                                    }>
                                        <option value="reseed" selected=true>
                                            "辅种"
                                        </option>
                                        <option value="repost">"转种"</option>
                                        <option value="sync_stats">"数据同步"</option>
                                    </select>
                                </label>
                                <label>
                                    "触发方式"
                                    <select on:change=move |ev| {
                                        set_trigger_type.set(event_target_value(&ev));
                                        set_cron_error.set(None);
                                    }>
                                        <option value="manual" selected=true>
                                            "手动"
                                        </option>
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
                <td>{task.name.clone()}</td>
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
