use crate::components::confirm_modal::ConfirmModal;
use crate::components::empty_state::EmptyState;
use crate::components::toast::{show_toast, ToastType};
use crate::server_fns::{
    create_downloader, create_downloader_pair, delete_downloader, delete_downloader_pair,
    get_downloader_pairs, get_downloaders, test_downloader, test_downloader_connection,
    DownloaderInfo, DownloaderPairInfo,
};
use leptos::prelude::*;

#[component]
pub fn DownloadersPage() -> impl IntoView {
    // --- Resources ---
    let downloaders = Resource::new(|| (), |_| get_downloaders());
    let pairs = Resource::new(|| (), |_| get_downloader_pairs());

    // --- Mutation actions ---
    let create_dl_action = Action::new(
        move |args: &(String, String, String, i64, String, String, String)| {
            let (name, dl_type, host, port, username, password, role) = args.clone();
            create_downloader(name, dl_type, host, port, username, password, role)
        },
    );

    let delete_dl_action = Action::new(move |id: &i64| {
        let id = *id;
        delete_downloader(id)
    });

    let test_dl_action = Action::new(move |id: &i64| {
        let id = *id;
        test_downloader(id)
    });

    let create_pair_action = Action::new(move |args: &(String, i64, i64)| {
        let (name, source_id, destination_id) = args.clone();
        create_downloader_pair(name, source_id, destination_id)
    });

    let delete_pair_action = Action::new(move |id: &i64| {
        let id = *id;
        delete_downloader_pair(id)
    });

    // Page-level delete confirmation targets so modals mount outside <tbody>.
    let (confirm_delete_dl, set_confirm_delete_dl) = signal(None::<(i64, String)>);
    let (confirm_delete_pair, set_confirm_delete_pair) = signal(None::<(i64, String)>);

    // --- Refetch after mutations ---
    Effect::new(move |_| {
        if let Some(result) = create_dl_action.value().get() {
            match result {
                Ok(_) => {
                    show_toast("下载器创建成功", ToastType::Success);
                    downloaders.refetch();
                }
                Err(e) => show_toast(format!("创建下载器失败：{e}"), ToastType::Error),
            }
        }
    });
    Effect::new(move |_| {
        if let Some(result) = delete_dl_action.value().get() {
            match result {
                Ok(_) => {
                    show_toast("下载器已删除", ToastType::Success);
                    set_confirm_delete_dl.set(None);
                    downloaders.refetch();
                    pairs.refetch();
                }
                Err(e) => show_toast(format!("删除下载器失败：{e}"), ToastType::Error),
            }
        }
    });
    Effect::new(move |_| {
        if let Some(result) = create_pair_action.value().get() {
            match result {
                Ok(_) => {
                    show_toast("迁移方向创建成功", ToastType::Success);
                    pairs.refetch();
                }
                Err(e) => show_toast(format!("创建迁移方向失败：{e}"), ToastType::Error),
            }
        }
    });
    Effect::new(move |_| {
        if let Some(result) = delete_pair_action.value().get() {
            match result {
                Ok(_) => {
                    show_toast("迁移方向已删除", ToastType::Success);
                    set_confirm_delete_pair.set(None);
                    pairs.refetch();
                }
                Err(e) => show_toast(format!("删除迁移方向失败：{e}"), ToastType::Error),
            }
        }
    });

    view! {
        <div class="dashboard">
            <div class="dashboard-header">
                <h1>"下载器管理"</h1>
            </div>

            // Section 1: Downloaders
            <DownloadersSection
                downloaders=downloaders
                create_dl_action=create_dl_action
                test_dl_action=test_dl_action
                on_request_delete=move |id: i64, name: String| set_confirm_delete_dl.set(Some((id, name)))
            />

            // Section 2: Source-Destination Pairs
            <PairsSection
                pairs=pairs
                downloaders=downloaders
                create_pair_action=create_pair_action
                on_request_delete=move |id: i64, name: String| set_confirm_delete_pair.set(Some((id, name)))
            />

            {move || {
                confirm_delete_dl.get().map(|(id, name)| {
                    view! {
                        <ConfirmModal
                            title="确认删除"
                            message=format!("确定要删除下载器「{name}」吗？此操作不可撤销。")
                            on_confirm=move || {
                                delete_dl_action.dispatch(id);
                            }
                            on_cancel=move || set_confirm_delete_dl.set(None)
                            confirm_label="确认删除"
                            danger=true
                        />
                    }
                })
            }}
            {move || {
                confirm_delete_pair.get().map(|(id, name)| {
                    view! {
                        <ConfirmModal
                            title="确认删除"
                            message=format!("确定要删除迁移方向「{name}」吗？此操作不可撤销。")
                            on_confirm=move || {
                                delete_pair_action.dispatch(id);
                            }
                            on_cancel=move || set_confirm_delete_pair.set(None)
                            confirm_label="确认删除"
                            danger=true
                        />
                    }
                })
            }}
        </div>
    }
}

// ---------------------------------------------------------------------------
// Section 1: Downloaders list + add form
// ---------------------------------------------------------------------------

/// 根据下载器类型返回默认端口
fn default_port_for_type(dl_type: &str) -> &'static str {
    match dl_type {
        "qbittorrent" => "8080",
        "transmission" => "9091",
        _ => "",
    }
}

/// 前端表单校验：返回各字段错误；全 None 表示通过。
fn validate_form_fields(
    name: &str,
    host: &str,
    port_str: &str,
) -> (Option<String>, Option<String>, Option<String>) {
    let name_err = if name.trim().is_empty() {
        Some("名称不能为空".into())
    } else {
        None
    };
    let host_err = if host.trim().is_empty() {
        Some("主机地址不能为空".into())
    } else {
        None
    };
    let port_err = match port_str.parse::<i64>() {
        Ok(p) if (1..=65535).contains(&p) => None,
        Ok(_) => Some("端口必须在 1–65535 范围内".into()),
        Err(_) => Some("端口必须为数字".into()),
    };
    (name_err, host_err, port_err)
}

#[component]
fn DownloadersSection<F>(
    downloaders: Resource<Result<Vec<DownloaderInfo>, ServerFnError>>,
    create_dl_action: Action<
        (String, String, String, i64, String, String, String),
        Result<DownloaderInfo, ServerFnError>,
    >,
    test_dl_action: Action<i64, Result<String, ServerFnError>>,
    on_request_delete: F,
) -> impl IntoView
where
    F: Fn(i64, String) + Copy + Send + Sync + 'static,
{
    let (show_form, set_show_form) = signal(false);

    // Form fields
    let (name, set_name) = signal(String::new());
    let (dl_type, set_dl_type) = signal("qbittorrent".to_string());
    let (host, set_host) = signal(String::new());
    let (port, set_port) = signal("8080".to_string());
    let (username, set_username) = signal(String::new());
    let (password, set_password) = signal(String::new());
    let (role, set_role) = signal("both".to_string());

    // Validation & connection test state
    let (name_error, set_name_error) = signal(Option::<String>::None);
    let (host_error, set_host_error) = signal(Option::<String>::None);
    let (port_error, set_port_error) = signal(Option::<String>::None);
    // 连接测试状态：None=未测试, Some(Ok(msg))=成功, Some(Err(msg))=失败
    let (conn_tested, set_conn_tested) = signal(Option::<Result<String, String>>::None);
    let (conn_testing, set_conn_testing) = signal(false);

    // 切换类型时自动填入默认端口、重置连接测试状态
    let on_type_change = move |ev: web_sys::Event| {
        let new_type = event_target_value(&ev);
        set_port.set(default_port_for_type(&new_type).to_string());
        set_dl_type.set(new_type);
        set_conn_tested.set(None);
        set_name_error.set(None);
        set_host_error.set(None);
        set_port_error.set(None);
    };

    // 任何字段变化时重置连接测试状态
    let field_changed = move || {
        set_conn_tested.set(None);
        set_name_error.set(None);
        set_host_error.set(None);
        set_port_error.set(None);
    };

    let apply_validation = move || -> bool {
        let (n, h, p) = validate_form_fields(&name.get(), &host.get(), &port.get());
        set_name_error.set(n.clone());
        set_host_error.set(h.clone());
        set_port_error.set(p.clone());
        n.is_none() && h.is_none() && p.is_none()
    };

    // 连接测试
    let on_test = move |_| {
        if !apply_validation() {
            return;
        }
        set_conn_testing.set(true);
        set_conn_tested.set(None);

        let dl_type_val = dl_type.get();
        let host_val = host.get();
        let port_val: i64 = port.get().parse().unwrap_or(0);
        let username_val = username.get();
        let password_val = password.get();

        leptos::task::spawn_local(async move {
            let result = test_downloader_connection(
                dl_type_val,
                host_val,
                port_val,
                username_val,
                password_val,
            )
            .await;
            match result {
                Ok(msg) => set_conn_tested.set(Some(Ok(msg))),
                Err(e) => set_conn_tested.set(Some(Err(format!("{e}")))),
            }
            set_conn_testing.set(false);
        });
    };

    // Close/reset form only after successful create.
    Effect::new(move |_| {
        if let Some(Ok(_)) = create_dl_action.value().get() {
            set_name.set(String::new());
            set_dl_type.set("qbittorrent".to_string());
            set_host.set(String::new());
            set_port.set("8080".to_string());
            set_username.set(String::new());
            set_password.set(String::new());
            set_role.set("both".to_string());
            set_name_error.set(None);
            set_host_error.set(None);
            set_port_error.set(None);
            set_conn_tested.set(None);
            set_show_form.set(false);
        }
    });

    // 提交创建
    let on_submit = move |_| {
        if !apply_validation() {
            return;
        }

        let port_val: i64 = port.get().parse().unwrap_or(0);
        create_dl_action.dispatch((
            name.get(),
            dl_type.get(),
            host.get(),
            port_val,
            username.get(),
            password.get(),
            role.get(),
        ));
    };

    // 连接测试是否通过
    let conn_ok = move || matches!(conn_tested.get(), Some(Ok(_)));

    // Track test results per-downloader
    let test_result = test_dl_action.value();

    view! {
        <div class="stats-table-section">
            <div class="section-header">
                <h2>"下载器"</h2>
                <button
                    class="btn btn--primary"
                    on:click=move |_| set_show_form.update(|v| *v = !*v)
                >
                    {move || if show_form.get() { "取消" } else { "添加下载器" }}
                </button>
            </div>
            <p class="section-desc">"管理 qBittorrent / Transmission 等下载器实例的连接信息。"</p>

            // Add downloader form
            {move || {
                if show_form.get() {
                    Some(view! {
                        <div class="add-form">
                            <div class="form-grid">
                                <div class="form-group">
                                    <label>"名称" <span class="required">"*"</span></label>
                                    <input
                                        type="text"
                                        placeholder="我的 qBittorrent"
                                        prop:value=move || name.get()
                                        on:input=move |ev| {
                                            set_name.set(event_target_value(&ev));
                                            field_changed();
                                        }
                                    />
                                    {move || name_error.get().map(|e| view! { <p class="field-error">{e}</p> })}
                                </div>
                                <div class="form-group">
                                    <label>"类型"</label>
                                    <select
                                        prop:value=move || dl_type.get()
                                        on:change=on_type_change
                                    >
                                        <option value="qbittorrent">"qBittorrent"</option>
                                        <option value="transmission">"Transmission"</option>
                                    </select>
                                </div>
                                <div class="form-group">
                                    <label>"主机" <span class="required">"*"</span></label>
                                    <input
                                        type="text"
                                        placeholder="127.0.0.1"
                                        prop:value=move || host.get()
                                        on:input=move |ev| {
                                            set_host.set(event_target_value(&ev));
                                            field_changed();
                                        }
                                    />
                                    {move || host_error.get().map(|e| view! { <p class="field-error">{e}</p> })}
                                </div>
                                <div class="form-group">
                                    <label>"端口" <span class="required">"*"</span></label>
                                    <input
                                        type="number"
                                        placeholder="8080"
                                        prop:value=move || port.get()
                                        on:input=move |ev| {
                                            set_port.set(event_target_value(&ev));
                                            field_changed();
                                        }
                                    />
                                    {move || port_error.get().map(|e| view! { <p class="field-error">{e}</p> })}
                                </div>
                                <div class="form-group">
                                    <label>"用户名"</label>
                                    <input
                                        type="text"
                                        placeholder="admin"
                                        prop:value=move || username.get()
                                        on:input=move |ev| {
                                            set_username.set(event_target_value(&ev));
                                            field_changed();
                                        }
                                    />
                                </div>
                                <div class="form-group">
                                    <label>"密码"</label>
                                    <input
                                        type="password"
                                        placeholder="密码"
                                        prop:value=move || password.get()
                                        on:input=move |ev| {
                                            set_password.set(event_target_value(&ev));
                                            field_changed();
                                        }
                                    />
                                </div>
                                <div class="form-group">
                                    <label>"用途"</label>
                                    <select
                                        prop:value=move || role.get()
                                        on:change=move |ev| set_role.set(event_target_value(&ev))
                                    >
                                        <option value="source">"仅拉取"</option>
                                        <option value="destination">"仅推送"</option>
                                        <option value="both">"拉取和推送"</option>
                                    </select>
                                </div>
                            </div>

                            // 连接测试结果
                            {move || {
                                conn_tested.get().map(|result| match result {
                                    Ok(msg) => view! {
                                        <div class="form-alert form-alert--success">{msg}</div>
                                    }.into_any(),
                                    Err(msg) => view! {
                                        <div class="form-alert form-alert--error">{msg}</div>
                                    }.into_any(),
                                })
                            }}

                            <div class="form-actions">
                                <button
                                    class="btn btn--outline"
                                    on:click=on_test
                                    disabled=move || conn_testing.get()
                                >
                                    {move || if conn_testing.get() { "测试中…" } else { "测试连接" }}
                                </button>
                                <button
                                    class="btn btn--primary"
                                    on:click=on_submit
                                    disabled=move || !conn_ok() || create_dl_action.pending().get()
                                    title=move || {
                                        if create_dl_action.pending().get() {
                                            "创建中...".to_string()
                                        } else if conn_ok() {
                                            "创建下载器".to_string()
                                        } else {
                                            "请先测试连接".to_string()
                                        }
                                    }
                                >
                                    {move || if create_dl_action.pending().get() { "创建中..." } else { "创建" }}
                                </button>
                            </div>
                        </div>
                    })
                } else {
                    None
                }
            }}

            // Downloaders table
            <Suspense fallback=move || view! { <p>"正在加载下载器..."</p> }>
                {move || {
                    downloaders.get().map(|result| match result {
                        Err(e) => view! {
                            <div class="load-error">
                                <span>{format!("下载器加载失败：{e}")}</span>
                                <button
                                    class="btn btn--sm btn--outline"
                                    on:click=move |_| downloaders.refetch()
                                >
                                    "重试"
                                </button>
                            </div>
                        }.into_any(),
                        Ok(list) if list.is_empty() => view! {
                            <EmptyState icon="⬇" message="尚未配置任何下载器。" />
                        }.into_any(),
                        Ok(list) => view! {
                            <div class="table-wrap">
                                <table class="stats-table">
                                    <thead>
                                        <tr>
                                            <th>"名称"</th>
                                            <th>"类型"</th>
                                            <th>"主机"</th>
                                            <th>"用途"</th>
                                            <th>"启用"</th>
                                            <th>"操作"</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {list
                                            .into_iter()
                                            .map(|dl| {
                                                view! {
                                                    <DownloaderRow
                                                        dl=dl
                                                        test_dl_action=test_dl_action
                                                        on_request_delete=on_request_delete
                                                    />
                                                }
                                            })
                                            .collect::<Vec<_>>()}
                                    </tbody>
                                </table>
                            </div>

                            // Show latest test result
                            {move || {
                                test_result.get().map(|res| match res {
                                    Ok(msg) => view! {
                                        <p class="test-result text-green">{msg}</p>
                                    }.into_any(),
                                    Err(e) => view! {
                                        <p class="test-result text-red">
                                            {format!("测试失败：{e}")}
                                        </p>
                                    }.into_any(),
                                })
                            }}
                        }.into_any(),
                    })
                }}
            </Suspense>
        </div>
    }
}

#[component]
fn DownloaderRow<F>(
    dl: DownloaderInfo,
    test_dl_action: Action<i64, Result<String, ServerFnError>>,
    on_request_delete: F,
) -> impl IntoView
where
    F: Fn(i64, String) + Copy + 'static,
{
    let dl_id = dl.id;
    let dl_name = dl.name.clone();
    let enabled_class = if dl.enabled { "text-green" } else { "text-red" };
    let enabled_label = if dl.enabled { "是" } else { "否" };
    let host_port = format!("{}:{}", dl.host, dl.port);
    let role_label = match dl.role.as_str() {
        "source" => "仅拉取".to_string(),
        "destination" => "仅推送".to_string(),
        "both" => "拉取和推送".to_string(),
        other => other.to_string(),
    };

    view! {
        <tr>
            <td>{dl.name.clone()}</td>
            <td>{dl.dl_type}</td>
            <td>{host_port}</td>
            <td>{role_label}</td>
            <td class=enabled_class>{enabled_label}</td>
            <td class="actions-cell">
                <button
                    class="btn btn--small btn--outline"
                    on:click=move |_| {
                        test_dl_action.dispatch(dl_id);
                    }
                >
                    "测试"
                </button>
                <button
                    class="btn btn--small btn--danger"
                    on:click=move |_| on_request_delete(dl_id, dl_name.clone())
                >
                    "删除"
                </button>
            </td>
        </tr>
    }
}

// ---------------------------------------------------------------------------
// Section 2: Source-Destination Pairs
// ---------------------------------------------------------------------------

#[component]
fn PairsSection<F>(
    pairs: Resource<Result<Vec<DownloaderPairInfo>, ServerFnError>>,
    downloaders: Resource<Result<Vec<DownloaderInfo>, ServerFnError>>,
    create_pair_action: Action<(String, i64, i64), Result<DownloaderPairInfo, ServerFnError>>,
    on_request_delete: F,
) -> impl IntoView
where
    F: Fn(i64, String) + Copy + Send + Sync + 'static,
{
    let (show_form, set_show_form) = signal(false);

    // Form fields
    let (pair_name, set_pair_name) = signal(String::new());
    let (source_id, set_source_id) = signal(String::new());
    let (dest_id, set_dest_id) = signal(String::new());

    let (name_error, set_name_error) = signal(None::<String>);
    let (source_error, set_source_error) = signal(None::<String>);
    let (dest_error, set_dest_error) = signal(None::<String>);

    // Close/reset form only after successful create.
    Effect::new(move |_| {
        if let Some(Ok(_)) = create_pair_action.value().get() {
            set_pair_name.set(String::new());
            set_source_id.set(String::new());
            set_dest_id.set(String::new());
            set_name_error.set(None);
            set_source_error.set(None);
            set_dest_error.set(None);
            set_show_form.set(false);
        }
    });

    let on_submit = move |_| {
        let name_val = pair_name.get();
        let mut ok = true;
        if name_val.trim().is_empty() {
            set_name_error.set(Some("迁移方向名称不能为空".into()));
            ok = false;
        } else {
            set_name_error.set(None);
        }
        let src: i64 = source_id.get().parse().unwrap_or(0);
        let dst: i64 = dest_id.get().parse().unwrap_or(0);
        if src == 0 {
            set_source_error.set(Some("请选择拉取端下载器".into()));
            ok = false;
        } else {
            set_source_error.set(None);
        }
        if dst == 0 {
            set_dest_error.set(Some("请选择推送端下载器".into()));
            ok = false;
        } else if src != 0 && src == dst {
            set_dest_error.set(Some("拉取端和推送端不能相同".into()));
            ok = false;
        } else {
            set_dest_error.set(None);
        }
        if !ok {
            return;
        }
        create_pair_action.dispatch((name_val, src, dst));
    };

    view! {
        <div class="stats-table-section">
            <div class="section-header">
                <h2>"迁移方向"</h2>
                <button
                    class="btn btn--primary"
                    on:click=move |_| set_show_form.update(|v| *v = !*v)
                >
                    {move || if show_form.get() { "取消" } else { "添加迁移方向" }}
                </button>
            </div>
            <p class="section-desc">"定义种子迁移的方向：从哪个下载器拉取、推送到哪个下载器。创建辅种任务时可选择一个迁移方向。"</p>

            // Add pair form
            {move || {
                if show_form.get() {
                    let dl_list = downloaders
                        .get()
                        .and_then(|r| r.ok())
                        .unwrap_or_default();
                    let dl_list2 = dl_list.clone();

                    Some(view! {
                        <div class="add-form">
                            <div class="form-row">
                                <label>"名称" <span class="required">"*"</span></label>
                                <input
                                    type="text"
                                    placeholder="本机 → 盒子"
                                    prop:value=move || pair_name.get()
                                    on:input=move |ev| {
                                        set_pair_name.set(event_target_value(&ev));
                                        set_name_error.set(None);
                                    }
                                />
                                {move || name_error.get().map(|e| view! { <p class="field-error">{e}</p> })}
                            </div>
                            <div class="form-row">
                                <label>"从哪拉取" <span class="required">"*"</span></label>
                                <select
                                    prop:value=move || source_id.get()
                                    on:change=move |ev| {
                                        set_source_id.set(event_target_value(&ev));
                                        set_source_error.set(None);
                                    }
                                >
                                    <option value="">"-- 选择拉取端 --"</option>
                                    {dl_list
                                        .into_iter()
                                        .map(|dl| {
                                            let val = dl.id.to_string();
                                            view! {
                                                <option value=val>{dl.name}</option>
                                            }
                                        })
                                        .collect::<Vec<_>>()}
                                </select>
                                {move || source_error.get().map(|e| view! { <p class="field-error">{e}</p> })}
                            </div>
                            <div class="form-row">
                                <label>"推送到哪" <span class="required">"*"</span></label>
                                <select
                                    prop:value=move || dest_id.get()
                                    on:change=move |ev| {
                                        set_dest_id.set(event_target_value(&ev));
                                        set_dest_error.set(None);
                                    }
                                >
                                    <option value="">"-- 选择推送端 --"</option>
                                    {dl_list2
                                        .into_iter()
                                        .map(|dl| {
                                            let val = dl.id.to_string();
                                            view! {
                                                <option value=val>{dl.name}</option>
                                            }
                                        })
                                        .collect::<Vec<_>>()}
                                </select>
                                {move || dest_error.get().map(|e| view! { <p class="field-error">{e}</p> })}
                            </div>
                            <div class="form-actions">
                                <button
                                    class="btn btn--primary"
                                    disabled=move || create_pair_action.pending().get()
                                    on:click=on_submit
                                >
                                    {move || if create_pair_action.pending().get() { "创建中..." } else { "创建" }}
                                </button>
                            </div>
                        </div>
                    })
                } else {
                    None
                }
            }}

            // Pairs table
            <Suspense fallback=move || view! { <p>"正在加载迁移方向..."</p> }>
                {move || {
                    pairs.get().map(|result| match result {
                        Err(e) => view! {
                            <div class="load-error">
                                <span>{format!("迁移方向加载失败：{e}")}</span>
                                <button
                                    class="btn btn--sm btn--outline"
                                    on:click=move |_| pairs.refetch()
                                >
                                    "重试"
                                </button>
                            </div>
                        }.into_any(),
                        Ok(list) if list.is_empty() => view! {
                            <EmptyState icon="↻" message="尚未配置任何迁移方向。" />
                        }.into_any(),
                        Ok(list) => view! {
                            <div class="table-wrap">
                                <table class="stats-table">
                                    <thead>
                                        <tr>
                                            <th>"名称"</th>
                                            <th>"拉取端"</th>
                                            <th>"推送端"</th>
                                            <th>"操作"</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {list
                                            .into_iter()
                                            .map(|pair| {
                                                view! {
                                                    <PairRow
                                                        pair=pair
                                                        on_request_delete=on_request_delete
                                                    />
                                                }
                                            })
                                            .collect::<Vec<_>>()}
                                    </tbody>
                                </table>
                            </div>
                        }.into_any(),
                    })
                }}
            </Suspense>
        </div>
    }
}

#[component]
fn PairRow<F>(
    pair: DownloaderPairInfo,
    on_request_delete: F,
) -> impl IntoView
where
    F: Fn(i64, String) + Copy + 'static,
{
    let pair_id = pair.id;
    let pair_name = pair.name.clone();

    view! {
        <tr>
            <td>{pair.name.clone()}</td>
            <td>{pair.source_name}</td>
            <td>{pair.destination_name}</td>
            <td class="actions-cell">
                <button
                    class="btn btn--small btn--danger"
                    on:click=move |_| on_request_delete(pair_id, pair_name.clone())
                >
                    "删除"
                </button>
            </td>
        </tr>
    }
}
