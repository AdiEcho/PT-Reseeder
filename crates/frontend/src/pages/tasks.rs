use crate::server_fns::{
    create_task, delete_task, get_task_logs, get_tasks, trigger_task, TaskInfo, TaskLogInfo,
};
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
                set_form_error.set(Some("Cron expression is required for cron trigger.".into()));
                return;
            }
            Some(c)
        } else {
            None
        };
        if n.trim().is_empty() {
            set_form_error.set(Some("Task name is required.".into()));
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
                <h1>"Task Management"</h1>
            </div>

            // --- Create Task Form ---
            <div class="form-section">
                <h2>"Create Task"</h2>
                <form class="inline-form" on:submit=on_create>
                    <label>
                        "Name"
                        <input
                            type="text"
                            placeholder="Task name"
                            prop:value=move || name.get()
                            on:input=move |ev| {
                                set_name.set(event_target_value(&ev));
                            }
                        />
                    </label>
                    <label>
                        "Type"
                        <select on:change=move |ev| {
                            set_task_type.set(event_target_value(&ev));
                        }>
                            <option value="reseed" selected=true>
                                "Reseed"
                            </option>
                            <option value="repost">"Repost"</option>
                        </select>
                    </label>
                    <label>
                        "Trigger"
                        <select on:change=move |ev| {
                            set_trigger_type.set(event_target_value(&ev));
                        }>
                            <option value="manual" selected=true>
                                "Manual"
                            </option>
                            <option value="cron">"Cron"</option>
                            <option value="file_watch">"File Watch"</option>
                        </select>
                    </label>
                    {move || {
                        if trigger_type.get() == "cron" {
                            Some(
                                view! {
                                    <label>
                                        "Cron Expression"
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
                        {move || if submitting.get() { "Creating..." } else { "Create" }}
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
                <h2>"Tasks"</h2>
                <Suspense fallback=move || {
                    view! { <p>"Loading tasks..."</p> }
                }>
                    {move || {
                        tasks
                            .get()
                            .map(|result| {
                                match result {
                                    Err(e) => {
                                        view! {
                                            <p class="error">{format!("Failed to load tasks: {e}")}</p>
                                        }
                                            .into_any()
                                    }
                                    Ok(list) if list.is_empty() => {
                                        view! { <p>"No tasks configured yet."</p> }.into_any()
                                    }
                                    Ok(list) => {
                                        view! {
                                            <div class="table-wrap">
                                                <table class="stats-table">
                                                    <thead>
                                                        <tr>
                                                            <th>"Name"</th>
                                                            <th>"Type"</th>
                                                            <th>"Trigger"</th>
                                                            <th>"Cron"</th>
                                                            <th>"Status"</th>
                                                            <th>"Last Run"</th>
                                                            <th>"Next Run"</th>
                                                            <th>"Runs"</th>
                                                            <th>"Actions"</th>
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

    let on_delete = move |_| {
        set_acting.set(true);
        leptos::task::spawn_local(async move {
            let _ = delete_task(task_id).await;
            set_acting.set(false);
            on_change();
        });
    };

    let sc = status_class(&task.status);
    let status_label = task.status.clone();
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
                <td>{task.task_type.clone()}</td>
                <td>{task.trigger_type.clone()}</td>
                <td class="text-muted">{cron_display}</td>
                <td class=sc>{status_label}</td>
                <td class="text-muted">{last_run}</td>
                <td class="text-muted">{next_run}</td>
                <td>{task.run_count}</td>
                <td class="action-cell">
                    <button
                        class="btn-sm btn-blue"
                        disabled=move || acting.get()
                        on:click=on_trigger
                    >
                        "Run Now"
                    </button>
                    <button
                        class="btn-sm btn-red"
                        disabled=move || acting.get()
                        on:click=on_delete
                    >
                        "Delete"
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
                                        view! { <p class="text-muted">"Loading logs..."</p> }
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
        return view! { <p class="text-muted">"No logs for this task."</p> }.into_any();
    }

    view! {
        <div class="table-wrap nested-table">
            <table class="stats-table">
                <thead>
                    <tr>
                        <th>"Status"</th>
                        <th>"Matched"</th>
                        <th>"Succeeded"</th>
                        <th>"Failed"</th>
                        <th>"Duration"</th>
                        <th>"Timestamp"</th>
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
                            let duration = log
                                .duration_ms
                                .map(|ms| format!("{:.1}s", ms as f64 / 1000.0))
                                .unwrap_or_else(|| "-".into());
                            let ts = if log.created_at.len() >= 16 {
                                log.created_at[..16].to_string()
                            } else {
                                log.created_at.clone()
                            };
                            view! {
                                <tr>
                                    <td class=lsc>{log.status.clone()}</td>
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
