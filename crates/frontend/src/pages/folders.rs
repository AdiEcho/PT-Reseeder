use crate::server_fns::{create_folder, delete_folder, get_folders, FolderInfo};
use leptos::ev;
use leptos::prelude::*;

#[component]
pub fn FoldersPage() -> impl IntoView {
    let (version, set_version) = signal(0u64);

    let folders = Resource::new(move || version.get(), |_| get_folders());

    // --- Add-folder form state ---
    let (path, set_path) = signal(String::new());
    let (scan_mode, set_scan_mode) = signal("local".to_string());
    let (downloader_id, set_downloader_id) = signal(String::new());
    let (form_error, set_form_error) = signal(None::<String>);
    let (submitting, set_submitting) = signal(false);

    let on_create = move |ev: leptos::ev::SubmitEvent| {
        ev.prevent_default();
        let p = path.get_untracked();
        let sm = scan_mode.get_untracked();
        if p.trim().is_empty() {
            set_form_error.set(Some("文件夹路径不能为空。".into()));
            return;
        }
        let dl_id = if sm == "downloader" {
            let raw = downloader_id.get_untracked();
            match raw.trim().parse::<i64>() {
                Ok(id) => Some(id),
                Err(_) => {
                    set_form_error.set(Some("下载器 ID 必须是有效数字。".into()));
                    return;
                }
            }
        } else {
            None
        };
        set_submitting.set(true);
        set_form_error.set(None);
        leptos::task::spawn_local(async move {
            match create_folder(p, sm, dl_id).await {
                Ok(_) => {
                    set_path.set(String::new());
                    set_downloader_id.set(String::new());
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
                <h1>"文件夹管理"</h1>
            </div>

            // --- Add Folder Form ---
            <div class="form-section">
                <h2>"添加文件夹"</h2>
                <form class="inline-form" on:submit=on_create>
                    <label>
                        "路径"
                        <input
                            type="text"
                            placeholder="/path/to/torrents"
                            prop:value=move || path.get()
                            on:input=move |ev| {
                                set_path.set(event_target_value(&ev));
                            }
                        />
                    </label>
                    <label>
                        "种子来源"
                        <select on:change=move |ev| {
                            set_scan_mode.set(event_target_value(&ev));
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
                                        "关联下载器 ID"
                                        <input
                                            type="text"
                                            placeholder="关联下载器 ID"
                                            prop:value=move || downloader_id.get()
                                            on:input=move |ev| {
                                                set_downloader_id.set(event_target_value(&ev));
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
                        {move || if submitting.get() { "添加中..." } else { "添加" }}
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
                                            <p class="error">
                                                {format!("文件夹加载失败：{e}")}
                                            </p>
                                        }
                                            .into_any()
                                    }
                                    Ok(list) if list.is_empty() => {
                                        view! { <p>"尚未配置任何文件夹。"</p> }.into_any()
                                    }
                                    Ok(list) => {
                                        view! {
                                            <div class="table-wrap">
                                                <table class="stats-table">
                                                    <thead>
                                                        <tr>
                                                            <th>"路径"</th>
                                                            <th>"种子来源"</th>
                                                            <th>"下载器"</th>
                                                            <th>"启用"</th>
                                                            <th>"上次扫描"</th>
                                                            <th>"操作"</th>
                                                        </tr>
                                                    </thead>
                                                    <tbody>
                                                        {list
                                                            .into_iter()
                                                            .map(|folder| {
                                                                view! {
                                                                    <FolderRow
                                                                        folder=folder
                                                                        on_change=move || {
                                                                            set_version.update(|v| *v += 1)
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
fn FolderRow(
    folder: FolderInfo,
    on_change: impl Fn() + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let folder_id = folder.id;
    let (acting, set_acting) = signal(false);
    let (confirm_delete, set_confirm_delete) = signal(false);

    let on_delete = move |_: ev::MouseEvent| {
        set_confirm_delete.set(true);
    };

    let do_delete = move |_: ev::MouseEvent| {
        set_acting.set(true);
        set_confirm_delete.set(false);
        leptos::task::spawn_local(async move {
            let _ = delete_folder(folder_id).await;
            set_acting.set(false);
            on_change();
        });
    };

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
    let downloader_display = folder
        .downloader_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| "-".into());
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
            <td class="text-muted">{downloader_display}</td>
            <td class=enabled_class>{enabled_label}</td>
            <td class="text-muted">{last_scanned}</td>
            <td class="action-cell">
                <button
                    class="btn btn--sm btn--danger"
                    disabled=move || acting.get()
                    on:click=on_delete
                >
                    "删除"
                </button>
                {move || {
                    if confirm_delete.get() {
                        Some(view! {
                            <div class="inline-form" style="margin-top: 6px;">
                                <span class="text-red">"确认？"</span>
                                <button class="btn btn--sm btn--danger" on:click=do_delete>"是"</button>
                                <button class="btn btn--sm btn--outline" on:click=move |_| set_confirm_delete.set(false)>"否"</button>
                            </div>
                        })
                    } else {
                        None
                    }
                }}
            </td>
        </tr>
    }
}
