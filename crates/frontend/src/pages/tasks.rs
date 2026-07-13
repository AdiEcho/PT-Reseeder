use crate::server_fns::{
    create_task, delete_task, get_task_logs, get_tasks, trigger_task, TaskInfo, TaskLogInfo,
};
use leptos::ev;
use leptos::prelude::*;

fn status_class(status: &str) -> &'static str {
    match status {
        "running" => "text-blue",
        "paused" => "text-yellow",
        "error" => "text-red",
        _ => "text-muted", // idle
    }
}

#[component]
pub fn TasksPage() -> impl IntoView {
    let (version, set_version) = signal(0u64);

    let tasks = Resource::new(move || version.get(), |_| get_tasks());

    // --- Create-task form state ---
    let (name, set_name) = signal(String::new());
    let (task_type, set_task_type) = signal("reseed".to_string());
    let (trigger_type, set_trigger_type) = signal("manual".to_string());
    let (cron_expr, set_cron_expr) = signal(String::new());
    let (form_error, set_form_error) = signal(None::<String>);
    let (submitting, set_submitting) = signal(false);

    let on_create = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let n = name.get_untracked();
        let tt = task_type.get_untracked();
        let tg = trigger_type.get_untracked();
        let cron = if tg == "cron" {
            let c = cron_expr.get_untracked();
            if c.trim().is_empty() {
                set_form_error.set(Some("Cron 触发器必须填写 Cron 表达式。".into()));
                return;
            }
            Some(c)
        } else {
            None
        };
        if n.trim().is_empty() {
            set_form_error.set(Some("任务名称不能为空。".into()));
            return;
        }
        set_submitting.set(true);
        set_form_error.set(None);
        leptos::task::spawn_local(async move {
            match create_task(n, tt, tg, cron).await {
                Ok(_) => {
                    set_name.set(String::new());
                    set_cron_expr.set(String::new());
                    set_version.update(|v| *v += 1);
                }
                Err(e) => {
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
            </div>

            // --- Create Task Form ---
            <div class="form-section">
                <h2>"创建任务"</h2>
                <form class="inline-form" on:submit=on_create>
                    <label>
                        "名称"
                        <input
                            type="text"
                            placeholder="任务名称"
                            prop:value=move || name.get()
                            on:input=move |ev| {
                                set_name.set(event_target_value(&ev));
                            }
                        />
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
                                        "Cron 表达式"
                                        <input
                                            type="text"
                                            placeholder="0 */5 * * * *"
                                            prop:value=move || cron_expr.get()
                                            on:input=move |ev| {
                                                set_cron_expr.set(event_target_value(&ev));
                                            }
                                        />
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
                            view! { <p class="error">{e}</p> }
                        })
                }}
            </div>

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
                                            <p class="error">{format!("任务加载失败：{e}")}</p>
                                        }
                                            .into_any()
                                    }
                                    Ok(list) if list.is_empty() => {
                                        view! { <p>"尚未配置任何任务。"</p> }.into_any()
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
                                                            <th>"Cron"</th>
                                                            <th>"状态"</th>
                                                            <th>"上次运行"</th>
                                                            <th>"下次运行"</th>
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
fn TaskRow(task: TaskInfo, on_change: impl Fn() + Copy + Send + Sync + 'static) -> impl IntoView {
    let task_id = task.id;
    let (expanded, set_expanded) = signal(false);
    let (acting, set_acting) = signal(false);
    let (confirm_delete, set_confirm_delete) = signal(false);

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

    let on_trigger = move |_| {
        set_acting.set(true);
        leptos::task::spawn_local(async move {
            let _ = trigger_task(task_id).await;
            set_acting.set(false);
            on_change();
        });
    };

    let on_delete = move |_: ev::MouseEvent| {
        set_confirm_delete.set(true);
    };

    let do_delete = move |_: ev::MouseEvent| {
        set_acting.set(true);
        set_confirm_delete.set(false);
        leptos::task::spawn_local(async move {
            let _ = delete_task(task_id).await;
            set_acting.set(false);
            on_change();
        });
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
        .map(|s| if s.len() >= 16 { &s[..16] } else { s })
        .unwrap_or("-")
        .to_string();
    let next_run = task
        .next_run_at
        .as_deref()
        .map(|s| if s.len() >= 16 { &s[..16] } else { s })
        .unwrap_or("-")
        .to_string();
    let cron_display = task.cron_expression.clone().unwrap_or_else(|| "-".into());

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
                <td class="text-muted">{cron_display}</td>
                <td class=sc>{status_label}</td>
                <td class="text-muted">{last_run}</td>
                <td class="text-muted">{next_run}</td>
                <td>{task.run_count}</td>
                <td class="action-cell">
                    <button
                        class="btn btn--sm btn--primary"
                        disabled=move || acting.get()
                        on:click=on_trigger
                    >
                        "立即运行"
                    </button>
                    <button
                        class="btn btn--sm btn--danger"
                        disabled=move || acting.get()
                        on:click=on_delete
                    >
                        "删除"
                    </button>
                </td>
            </tr>
            // Delete confirmation dialog
            {move || {
                if confirm_delete.get() {
                    Some(view! {
                        <tr class="expand-row">
                            <td colspan="9">
                                <div class="inline-form">
                                    <span class="text-red">"确定要删除该任务吗？"</span>
                                    <button class="btn btn--sm btn--danger" on:click=do_delete>"确认删除"</button>
                                    <button class="btn btn--sm btn--outline" on:click=move |_| set_confirm_delete.set(false)>"取消"</button>
                                </div>
                            </td>
                        </tr>
                    })
                } else {
                    None
                }
            }}
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
                            let ts = if log.created_at.len() >= 16 {
                                log.created_at[..16].to_string()
                            } else {
                                log.created_at.clone()
                            };
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
