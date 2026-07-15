use crate::components::confirm_modal::ConfirmModal;
use crate::components::empty_state::EmptyState;
use crate::components::toast::{show_toast, ToastType};
use crate::server_fns::{
    create_folder, delete_folder, get_downloaders, get_folders, DownloaderInfo, FolderInfo,
};
use leptos::prelude::*;

#[component]
pub fn FoldersPage() -> impl IntoView {
    let (version, set_version) = signal(0u64);
    let (show_form, set_show_form) = signal(false);

    let folders = Resource::new(move || version.get(), |_| get_folders());
    let downloaders = Resource::new(|| (), |_| get_downloaders());
    let (confirm_delete, set_confirm_delete) = signal(None::<(i64, String)>);

    // --- Add-folder form state ---
    let (path, set_path) = signal(String::new());
    let (scan_mode, set_scan_mode) = signal("local".to_string());
    let (downloader_id, set_downloader_id) = signal(String::new());
    let (form_error, set_form_error) = signal(None::<String>);
    let (path_error, set_path_error) = signal(None::<String>);
    let (dl_error, set_dl_error) = signal(None::<String>);
    let (submitting, set_submitting) = signal(false);

    let on_create = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let p = path.get_untracked();
        let sm = scan_mode.get_untracked();
        set_path_error.set(None);
        set_dl_error.set(None);
        set_form_error.set(None);

        if p.trim().is_empty() {
            set_path_error.set(Some("文件夹路径不能为空。".into()));
            return;
        }
        let dl_id = if sm == "downloader" {
            let raw = downloader_id.get_untracked();
            match raw.trim().parse::<i64>() {
                Ok(id) => Some(id),
                Err(_) => {
                    set_dl_error.set(Some("请选择关联下载器。".into()));
                    return;
                }
            }
        } else {
            None
        };
        set_submitting.set(true);
        leptos::task::spawn_local(async move {
            match create_folder(p, sm, dl_id).await {
                Ok(_) => {
                    show_toast("文件夹添加成功", ToastType::Success);
                    set_path.set(String::new());
                    set_downloader_id.set(String::new());
                    set_show_form.set(false);
                    set_version.update(|v| *v += 1);
                }
                Err(e) => {
                    show_toast(format!("添加失败：{e}"), ToastType::Error);
                    set_form_error.set(Some(format!("{e}")));
                }
            }
            set_submitting.set(false);
        });
    };

    view! {
        <div class="dashboard">
            <div class="dashboard-header">
                <h1>"文件夹管理"</h1>
                <button
                    class="btn btn-primary"
                    on:click=move |_| set_show_form.update(|v| *v = !*v)
                >
                    {move || if show_form.get() { "取消" } else { "添加文件夹" }}
                </button>
            </div>

            // --- Add Folder Form (collapsible) ---
            {move || {
                if show_form.get() {
                    view! {
                        <div class="form-section">
                            <h2>"添加文件夹"</h2>
                            <form class="inline-form" on:submit=on_create>
                                <label>
                                    "路径" <span class="required">"*"</span>
                                    <input
                                        type="text"
                                        placeholder="/path/to/torrents"
                                        prop:value=move || path.get()
                                        on:input=move |ev| {
                                            set_path.set(event_target_value(&ev));
                                            set_path_error.set(None);
                                        }
                                    />
                                    {move || path_error.get().map(|e| view! { <p class="field-error">{e}</p> })}
                                </label>
                                <label>
                                    "种子来源"
                                    <select on:change=move |ev| {
                                        set_scan_mode.set(event_target_value(&ev));
                                        set_dl_error.set(None);
                                    }>
                                        <option value="local" selected=true>
                                            "本机磁盘"
                                        </option>
                                        <option value="downloader">"从下载器读取"</option>
                                    </select>
                                </label>
                                {move || {
                                    if scan_mode.get() == "downloader" {
                                        Some(
                                            view! {
                                                <label>
                                                    "关联下载器" <span class="required">"*"</span>
                                                    <Suspense fallback=move || view! {
                                                        <select class="input" disabled=true>
                                                            <option>"加载下载器..."</option>
                                                        </select>
                                                    }>
                                                        {move || {
                                                            downloaders.get().map(|result| match result {
                                                                Ok(list) => {
                                                                    view! {
                                                                        <select
                                                                            class="input"
                                                                            prop:value=move || downloader_id.get()
                                                                            on:change=move |ev| {
                                                                                set_downloader_id.set(event_target_value(&ev));
                                                                                set_dl_error.set(None);
                                                                            }
                                                                        >
                                                                            <option value="">"请选择下载器"</option>
                                                                            {list
                                                                                .into_iter()
                                                                                .map(|dl: DownloaderInfo| {
                                                                                    let id = dl.id.to_string();
                                                                                    let label = format!("{} (#{})", dl.name, dl.id);
                                                                                    view! {
                                                                                        <option value=id.clone()>{label}</option>
                                                                                    }
                                                                                })
                                                                                .collect::<Vec<_>>()}
                                                                        </select>
                                                                        <p class="field-hint">
                                                                            "从「下载器管理」中已配置的客户端里选择；列表显示名称与 ID。"
                                                                        </p>
                                                                    }
                                                                        .into_any()
                                                                }
                                                                Err(e) => {
                                                                    view! {
                                                                        <div>
                                                                            <input
                                                                                type="number"
                                                                                placeholder="下载器 ID（数字）"
                                                                                prop:value=move || downloader_id.get()
                                                                                on:input=move |ev| {
                                                                                    set_downloader_id.set(event_target_value(&ev));
                                                                                    set_dl_error.set(None);
                                                                                }
                                                                            />
                                                                            <p class="field-hint">
                                                                                {format!("下载器列表加载失败（{e}），可临时填写数字 ID。")}
                                                                            </p>
                                                                        </div>
                                                                    }
                                                                        .into_any()
                                                                }
                                                            })
                                                        }}
                                                    </Suspense>
                                                    {move || dl_error.get().map(|e| view! { <p class="field-error">{e}</p> })}
                                                </label>
                                            },
                                        )
                                    } else {
                                        None
                                    }
                                }}
                                <button type="submit" disabled=move || submitting.get()>
                                    {move || if submitting.get() { "添加中..." } else { "添加" }}
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
                confirm_delete.get().map(|(folder_id, path)| {
                    view! {
                        <ConfirmModal
                            title="确认删除"
                            message=format!("确定要删除文件夹「{path}」吗？此操作不可撤销。")
                            on_confirm=move || {
                                set_confirm_delete.set(None);
                                leptos::task::spawn_local(async move {
                                    match delete_folder(folder_id).await {
                                        Ok(_) => show_toast("文件夹已删除", ToastType::Success),
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

            // --- Folders Table ---
            <div class="stats-table-section">
                <h2>"种子文件夹"</h2>
                <Suspense fallback=move || {
                    view! { <p>"正在加载文件夹..."</p> }
                }>
                    {move || {
                        folders
                            .get()
                            .map(|result| {
                                match result {
                                    Err(e) => {
                                        view! {
                                            <div class="load-error">
                                                <span>{format!("文件夹加载失败：{e}")}</span>
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
                                        view! { <EmptyState icon="📁" message="尚未配置任何文件夹。" /> }.into_any()
                                    }
                                    Ok(list) => {
                                        let dl_map = downloaders
                                            .get()
                                            .and_then(|r| r.ok())
                                            .unwrap_or_default()
                                            .into_iter()
                                            .map(|d| (d.id, d.name))
                                            .collect::<std::collections::HashMap<_, _>>();
                                        view! {
                                            <div class="table-wrap">
                                                <table class="stats-table">
                                                    <thead>
                                                        <tr>
                                                            <th>"路径"</th>
                                                            <th>"种子来源"</th>
                                                            <th>"下载器"</th>
                                                            <th>"启用"</th>
                                                            <th class="col-secondary">"上次扫描"</th>
                                                            <th>"操作"</th>
                                                        </tr>
                                                    </thead>
                                                    <tbody>
                                                        {list
                                                            .into_iter()
                                                            .map(|folder| {
                                                                let dl_label = folder
                                                                    .downloader_id
                                                                    .map(|id| {
                                                                        dl_map
                                                                            .get(&id)
                                                                            .map(|n| format!("{n} (#{id})"))
                                                                            .unwrap_or_else(|| format!("#{id}"))
                                                                    })
                                                                    .unwrap_or_else(|| "-".into());
                                                                view! {
                                                                    <FolderRow
                                                                        folder=folder
                                                                        downloader_label=dl_label
                                                                        on_request_delete=move |id: i64, path: String| {
                                                                            set_confirm_delete.set(Some((id, path)));
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
fn FolderRow<F>(
    folder: FolderInfo,
    downloader_label: String,
    on_request_delete: F,
) -> impl IntoView
where
    F: Fn(i64, String) + Copy + 'static,
{
    let folder_id = folder.id;
    let folder_path = folder.path.clone();

    let enabled_class = if folder.enabled {
        "text-green"
    } else {
        "text-muted"
    };
    let enabled_label = if folder.enabled { "是" } else { "否" };
    let scan_mode_label = match folder.scan_mode.as_str() {
        "local" => "本机磁盘".to_string(),
        "downloader" => "从下载器读取".to_string(),
        other => other.to_string(),
    };
    let last_scanned = folder
        .last_scanned_at
        .as_deref()
        .map(|s| if s.len() >= 16 { &s[..16] } else { s })
        .unwrap_or("-")
        .to_string();

    view! {
        <tr>
            <td>{folder.path.clone()}</td>
            <td>{scan_mode_label}</td>
            <td class="text-muted">{downloader_label}</td>
            <td class=enabled_class>{enabled_label}</td>
            <td class="text-muted col-secondary">{last_scanned}</td>
            <td class="action-cell">
                <button
                    class="btn btn--sm btn--danger"
                    on:click=move |_| on_request_delete(folder_id, folder_path.clone())
                >
                    "删除"
                </button>
            </td>
        </tr>
    }
}
