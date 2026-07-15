use crate::components::confirm_modal::ConfirmModal;
use crate::components::empty_state::EmptyState;
use crate::components::toast::{show_toast, ToastType};
use crate::server_fns::{delete_repost, get_repost_queue, RepostEntry};
#[cfg(not(target_arch = "wasm32"))]
use crate::server_fns::{review_repost, submit_repost};
use leptos::prelude::*;
use serde::Deserialize;
#[cfg(target_arch = "wasm32")]
use serde_json::json;

#[derive(Debug, Clone, Deserialize)]
struct AutofillResponse {
    success: bool,
    filled: Vec<String>,
    skipped: Vec<String>,
    message: String,
    target_site: String,
    confirmation_required: bool,
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Deserialize)]
struct ApiErrorResponse {
    error: String,
}

#[derive(Clone)]
enum ConfirmKind {
    Submit { id: i64, label: String },
    Delete { id: i64, label: String },
    Reject {
        id: i64,
        label: String,
        notes: Option<String>,
    },
}

const STATUSES: &[(&str, Option<&str>)] = &[
    ("全部", None),
    ("待审核", Some("pending")),
    ("已批准", Some("approved")),
    ("已提交", Some("submitted")),
    ("失败", Some("failed")),
    ("已拒绝", Some("rejected")),
];

fn status_label(status: &str) -> &'static str {
    match status {
        "pending" => "待审核",
        "approved" => "已批准",
        "submitted" => "已提交",
        "failed" => "失败",
        "rejected" => "已拒绝",
        _ => "未知",
    }
}

fn status_badge_class(status: &str) -> &'static str {
    match status {
        "pending" => "badge badge--yellow",
        "approved" => "badge badge--blue",
        "submitted" => "badge badge--green",
        "failed" => "badge badge--red",
        "rejected" => "badge badge--gray",
        _ => "badge",
    }
}

fn entry_label(entry: &RepostEntry) -> String {
    format!(
        "{} → {}（种子 {}）",
        entry.source_site_name, entry.target_site_name, entry.source_torrent_id
    )
}

#[cfg(target_arch = "wasm32")]
fn is_desktop_webview() -> bool {
    use wasm_bindgen::JsValue;

    let Some(window) = web_sys::window() else {
        return false;
    };
    let window = JsValue::from(window);
    js_sys::Reflect::has(&window, &JsValue::from_str("__TAURI__")).unwrap_or(false)
        || js_sys::Reflect::has(&window, &JsValue::from_str("__TAURI_INTERNALS__")).unwrap_or(false)
}

#[cfg(target_arch = "wasm32")]
async fn review_repost_core(id: i64, action: String, notes: Option<String>) -> Result<(), String> {
    post_json(
        &format!("/api/repost/queue/{id}/review"),
        json!({ "action": action, "notes": notes, "mapping": null }).to_string(),
    )
    .await
}

#[cfg(not(target_arch = "wasm32"))]
async fn review_repost_core(id: i64, action: String, notes: Option<String>) -> Result<(), String> {
    review_repost(id, action, notes)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(target_arch = "wasm32")]
async fn submit_repost_core(id: i64) -> Result<(), String> {
    post_json(&format!("/api/repost/queue/{id}/submit"), "{}".to_string()).await
}

#[cfg(not(target_arch = "wasm32"))]
async fn submit_repost_core(id: i64) -> Result<(), String> {
    submit_repost(id).await.map_err(|e| e.to_string())
}

#[cfg(target_arch = "wasm32")]
async fn post_json(path: &str, body: String) -> Result<(), String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::JsValue;
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{Headers, Request, RequestCredentials, RequestInit, RequestMode, Response};

    let headers = Headers::new().map_err(|e| format!("headers error: {e:?}"))?;
    headers
        .set("Content-Type", "application/json")
        .map_err(|e| format!("headers error: {e:?}"))?;
    headers
        .set("X-PT-Reseeder", "1")
        .map_err(|e| format!("headers error: {e:?}"))?;

    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_mode(RequestMode::SameOrigin);
    opts.set_credentials(RequestCredentials::SameOrigin);
    opts.set_headers(&headers);
    opts.set_body(&JsValue::from_str(&body));

    let request =
        Request::new_with_str_and_init(path, &opts).map_err(|e| format!("request error: {e:?}"))?;
    let window = web_sys::window().ok_or_else(|| "window unavailable".to_string())?;
    let response_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("fetch error: {e:?}"))?;
    let response: Response = response_value
        .dyn_into()
        .map_err(|_| "fetch did not return a Response".to_string())?;

    if response.ok() {
        Ok(())
    } else {
        Err(format!("request failed with HTTP {}", response.status()))
    }
}

#[cfg(target_arch = "wasm32")]
async fn post_json_response<T>(path: &str, body: String) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    use wasm_bindgen::JsCast;
    use wasm_bindgen::JsValue;
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{Headers, Request, RequestCredentials, RequestInit, RequestMode, Response};

    let headers = Headers::new().map_err(|e| format!("headers error: {e:?}"))?;
    headers
        .set("Content-Type", "application/json")
        .map_err(|e| format!("headers error: {e:?}"))?;
    headers
        .set("X-PT-Reseeder", "1")
        .map_err(|e| format!("headers error: {e:?}"))?;

    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_mode(RequestMode::SameOrigin);
    opts.set_credentials(RequestCredentials::SameOrigin);
    opts.set_headers(&headers);
    opts.set_body(&JsValue::from_str(&body));

    let request =
        Request::new_with_str_and_init(path, &opts).map_err(|e| format!("request error: {e:?}"))?;
    let window = web_sys::window().ok_or_else(|| "window unavailable".to_string())?;
    let response_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("fetch error: {e:?}"))?;
    let response: Response = response_value
        .dyn_into()
        .map_err(|_| "fetch did not return a Response".to_string())?;
    let response_text = JsFuture::from(
        response
            .text()
            .map_err(|e| format!("response body error: {e:?}"))?,
    )
    .await
    .map_err(|e| format!("response body error: {e:?}"))?
    .as_string()
    .unwrap_or_default();

    if response.ok() {
        serde_json::from_str(&response_text).map_err(|e| format!("invalid response: {e}"))
    } else {
        let error = serde_json::from_str::<ApiErrorResponse>(&response_text)
            .map(|body| body.error)
            .unwrap_or_else(|_| format!("request failed with HTTP {}", response.status()));
        Err(error)
    }
}

#[cfg(target_arch = "wasm32")]
async fn request_headless_autofill(id: i64) -> Result<AutofillResponse, String> {
    post_json_response(
        &format!("/api/repost/queue/{id}/autofill"),
        "{}".to_string(),
    )
    .await
}

#[cfg(not(target_arch = "wasm32"))]
async fn request_headless_autofill(_id: i64) -> Result<AutofillResponse, String> {
    Err("headless autofill is available after the page hydrates".to_string())
}

#[cfg(target_arch = "wasm32")]
async fn inject_desktop_autofill(id: i64) -> Result<(), String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::JsValue;
    use wasm_bindgen_futures::JsFuture;

    let window = web_sys::window().ok_or_else(|| "window unavailable".to_string())?;
    let window = JsValue::from(window);
    let tauri = js_sys::Reflect::get(&window, &JsValue::from_str("__TAURI__"))
        .map_err(|e| format!("tauri unavailable: {e:?}"))?;
    let core = js_sys::Reflect::get(&tauri, &JsValue::from_str("core"))
        .map_err(|e| format!("tauri core unavailable: {e:?}"))?;
    let invoke = js_sys::Reflect::get(&core, &JsValue::from_str("invoke"))
        .map_err(|e| format!("tauri invoke unavailable: {e:?}"))?
        .dyn_into::<js_sys::Function>()
        .map_err(|_| "tauri invoke is not a function".to_string())?;
    let args = js_sys::Object::new();
    js_sys::Reflect::set(
        &args,
        &JsValue::from_str("entryId"),
        &JsValue::from_f64(id as f64),
    )
    .map_err(|e| format!("tauri args error: {e:?}"))?;
    let promise = invoke
        .call2(
            &core,
            &JsValue::from_str("inject_repost_autofill"),
            &JsValue::from(args),
        )
        .map_err(|e| format!("tauri invoke failed: {e:?}"))?
        .dyn_into::<js_sys::Promise>()
        .map_err(|_| "tauri invoke did not return a Promise".to_string())?;
    JsFuture::from(promise)
        .await
        .map(|_| ())
        .map_err(|e| format!("tauri command failed: {e:?}"))
}

#[cfg(not(target_arch = "wasm32"))]
async fn inject_desktop_autofill(_id: i64) -> Result<(), String> {
    Err("desktop autofill is only available inside the desktop WebView".to_string())
}

#[component]
pub fn RepostPage() -> impl IntoView {
    let (status_filter, set_status_filter) = signal(None::<String>);
    let (version, set_version) = signal(0u32);
    let (confirm, set_confirm) = signal(None::<ConfirmKind>);

    let queue = Resource::new(
        move || (status_filter.get(), version.get()),
        |(filter, _ver)| get_repost_queue(filter),
    );

    let refetch = move || set_version.update(|v| *v += 1);

    view! {
        <div class="dashboard">
            <div class="dashboard-header">
                <h1>"转种队列"</h1>
                <div class="trend-selector">
                    {STATUSES
                        .iter()
                        .map(|(label, value)| {
                            let value = value.map(|s| s.to_string());
                            let val_clone = value.clone();
                            view! {
                                <button
                                    class:active=move || status_filter.get() == val_clone
                                    on:click={
                                        let value = value.clone();
                                        move |_| set_status_filter.set(value.clone())
                                    }
                                >
                                    {*label}
                                </button>
                            }
                        })
                        .collect::<Vec<_>>()}
                </div>
            </div>

            {move || {
                confirm.get().map(|kind| {
                    let (title, message, confirm_label, danger) = match &kind {
                        ConfirmKind::Submit { label, .. } => (
                            "确认提交",
                            format!("确定要提交转种「{label}」吗？提交后将向目标站上传。"),
                            "确认提交",
                            false,
                        ),
                        ConfirmKind::Delete { label, .. } => (
                            "确认删除",
                            format!("确定要删除转种条目「{label}」吗？此操作不可撤销。"),
                            "确认删除",
                            true,
                        ),
                        ConfirmKind::Reject { label, .. } => (
                            "确认拒绝",
                            format!("确定要拒绝转种「{label}」吗？"),
                            "确认拒绝",
                            true,
                        ),
                    };

                    view! {
                        <ConfirmModal
                            title=title
                            message=message
                            confirm_label=confirm_label
                            danger=danger
                            on_confirm=move || {
                                let kind = kind.clone();
                                set_confirm.set(None);
                                leptos::task::spawn_local(async move {
                                    match kind {
                                        ConfirmKind::Submit { id, .. } => {
                                            match submit_repost_core(id).await {
                                                Ok(_) => {
                                                    show_toast("已提交转种", ToastType::Success);
                                                    refetch();
                                                }
                                                Err(e) => {
                                                    show_toast(format!("提交失败：{e}"), ToastType::Error)
                                                }
                                            }
                                        }
                                        ConfirmKind::Delete { id, .. } => {
                                            match delete_repost(id).await {
                                                Ok(_) => {
                                                    show_toast("转种条目已删除", ToastType::Success);
                                                    refetch();
                                                }
                                                Err(e) => {
                                                    show_toast(format!("删除失败：{e}"), ToastType::Error)
                                                }
                                            }
                                        }
                                        ConfirmKind::Reject { id, notes, .. } => {
                                            match review_repost_core(id, "rejected".into(), notes).await {
                                                Ok(_) => {
                                                    show_toast("已拒绝转种", ToastType::Success);
                                                    refetch();
                                                }
                                                Err(e) => {
                                                    show_toast(format!("拒绝失败：{e}"), ToastType::Error)
                                                }
                                            }
                                        }
                                    }
                                });
                            }
                            on_cancel=move || set_confirm.set(None)
                        />
                    }
                })
            }}

            <Suspense fallback=move || {
                view! { <p>"正在加载转种队列..."</p> }
            }>
                {move || {
                    queue
                        .get()
                        .map(|result| {
                            match result {
                                Err(e) => {
                                    view! {
                                        <div class="load-error">
                                            <span>{format!("转种队列加载失败：{e}")}</span>
                                            <button
                                                class="btn btn--sm btn--outline"
                                                on:click=move |_| refetch()
                                            >
                                                "重试"
                                            </button>
                                        </div>
                                    }
                                        .into_any()
                                }
                                Ok(entries) => {
                                    if entries.is_empty() {
                                        view! { <EmptyState icon="↻" message="队列中没有条目。" /> }.into_any()
                                    } else {
                                        view! {
                                            <RepostTable
                                                entries=entries
                                                on_mutated=refetch
                                                on_confirm=move |kind: ConfirmKind| set_confirm.set(Some(kind))
                                            />
                                        }
                                            .into_any()
                                    }
                                }
                            }
                        })
                }}
            </Suspense>
        </div>
    }
}

#[component]
fn RepostTable<F, G>(entries: Vec<RepostEntry>, on_mutated: F, on_confirm: G) -> impl IntoView
where
    F: Fn() + Copy + Send + Sync + 'static,
    G: Fn(ConfirmKind) + Copy + Send + Sync + 'static,
{
    view! {
        <div class="stats-table-section">
            <div class="table-wrap">
                <table class="stats-table">
                    <thead>
                        <tr>
                            <th>"源站点"</th>
                            <th>"种子 ID"</th>
                            <th>"目标站点"</th>
                            <th>"状态"</th>
                            <th class="col-secondary">"备注"</th>
                            <th class="col-secondary">"时间"</th>
                            <th>"操作"</th>
                        </tr>
                    </thead>
                    <tbody>
                        {entries
                            .into_iter()
                            .map(|entry| {
                                view! {
                                    <RepostRow
                                        entry=entry
                                        on_mutated=on_mutated
                                        on_confirm=on_confirm
                                    />
                                }
                            })
                            .collect::<Vec<_>>()}
                    </tbody>
                </table>
            </div>
        </div>
    }
}

#[component]
fn RepostRow<F, G>(entry: RepostEntry, on_mutated: F, on_confirm: G) -> impl IntoView
where
    F: Fn() + Copy + Send + Sync + 'static,
    G: Fn(ConfirmKind) + Copy + Send + Sync + 'static,
{
    let id = entry.id;
    let status = entry.status.clone();
    let badge_class = status_badge_class(&status);
    let status_text = status_label(&status);
    let label = entry_label(&entry);

    let submitted_display = entry
        .submitted_at
        .as_deref()
        .map(|s| {
            if s.len() >= 16 {
                s[..16].to_string()
            } else {
                s.to_string()
            }
        })
        .unwrap_or_else(|| "-".into());

    let created_display = if entry.created_at.len() >= 16 {
        entry.created_at[..16].to_string()
    } else {
        entry.created_at.clone()
    };

    let time_display = if submitted_display != "-" {
        submitted_display.clone()
    } else {
        created_display.clone()
    };
    let time_title = format!("提交：{submitted_display} / 创建：{created_display}");

    let notes_display = entry.review_notes.clone().unwrap_or_default();
    let notes_title = notes_display.clone();
    let notes_cell = if notes_display.is_empty() {
        "-".to_string()
    } else {
        notes_display
    };

    view! {
        <tr>
            <td>{entry.source_site_name.clone()}</td>
            <td>{entry.source_torrent_id.clone()}</td>
            <td>{entry.target_site_name.clone()}</td>
            <td>
                <span class=badge_class>{status_text}</span>
            </td>
            <td class="text-muted table-col--secondary" title=notes_title>
                {notes_cell}
            </td>
            <td class="text-muted col-secondary" title=time_title>
                {time_display}
            </td>
            <td class="action-cell">
                <RowActions
                    id=id
                    status=status
                    label=label
                    on_mutated=on_mutated
                    on_confirm=on_confirm
                />
            </td>
        </tr>
    }
}

#[component]
fn RowActions<F, G>(id: i64, status: String, label: String, on_mutated: F, on_confirm: G) -> impl IntoView
where
    F: Fn() + Copy + Send + Sync + 'static,
    G: Fn(ConfirmKind) + Copy + Send + Sync + 'static,
{
    match status.as_str() {
        "pending" => view! {
            <PendingActions id=id label=label on_mutated=on_mutated on_confirm=on_confirm />
        }
        .into_any(),
        "approved" => {
            let label_submit = label.clone();
            view! {
                <div class="btn-group">
                    <button
                        class="btn btn--green btn--sm"
                        on:click=move |_| {
                            on_confirm(ConfirmKind::Submit {
                                id,
                                label: label_submit.clone(),
                            });
                        }
                    >
                        "提交"
                    </button>
                    <AutofillAction id=id />
                </div>
            }
            .into_any()
        }
        "failed" => view! {
            <button
                class="btn btn--blue btn--sm"
                on:click=move |_| {
                    leptos::task::spawn_local(async move {
                        match review_repost_core(id, "approved".into(), None).await {
                            Ok(_) => {
                                show_toast("已重新批准，可再次提交", ToastType::Success);
                                on_mutated();
                            }
                            Err(e) => show_toast(format!("重试失败：{e}"), ToastType::Error),
                        }
                    });
                }
            >
                "重试"
            </button>
        }
        .into_any(),
        "submitted" => view! { <span class="text-green">"✓"</span> }.into_any(),
        "rejected" => {
            let label_delete = label.clone();
            view! {
                <button
                    class="btn btn--red btn--sm"
                    on:click=move |_| {
                        on_confirm(ConfirmKind::Delete {
                            id,
                            label: label_delete.clone(),
                        });
                    }
                >
                    "删除"
                </button>
            }
            .into_any()
        }
        _ => view! { <span class="text-muted">"-"</span> }.into_any(),
    }
}

#[component]
fn AutofillAction(id: i64) -> impl IntoView {
    let (is_desktop, set_is_desktop) = signal(false);
    let (busy, set_busy) = signal(false);

    #[cfg(target_arch = "wasm32")]
    Effect::new(move |_| {
        set_is_desktop.set(is_desktop_webview());
    });

    #[cfg(not(target_arch = "wasm32"))]
    let _ = set_is_desktop;

    view! {
        <span class="desktop-autofill">
            <button
                class="btn btn--blue btn--sm"
                disabled=move || busy.get()
                title=move || {
                    if is_desktop.get() {
                        "桌面端 WebView 自动填表"
                    } else {
                        "由服务器无头浏览器打开上传页并填表，不会自动提交"
                    }
                }
                on:click=move |_| {
                    set_busy.set(true);
                    leptos::task::spawn_local(async move {
                        let result = if is_desktop.get_untracked() {
                            inject_desktop_autofill(id)
                                .await
                                .map(|_| "自动填表事件已发送，请在上传页确认后手动提交。".to_string())
                        } else {
                            request_headless_autofill(id).await.map(|result| {
                                let status = if result.success { "自动填表完成" } else { "自动填表未完成" };
                                let confirmation = if result.confirmation_required {
                                    "请在目标站上传页确认后手动提交。"
                                } else {
                                    ""
                                };
                                format!(
                                    "{status}（{}）：已填 {} 项，跳过 {} 项。{} {}",
                                    result.target_site,
                                    result.filled.len(),
                                    result.skipped.len(),
                                    result.message,
                                    confirmation
                                )
                            })
                        };
                        match result {
                            Ok(msg) => show_toast(msg, ToastType::Info),
                            Err(e) => show_toast(format!("自动填表不可用：{e}"), ToastType::Error),
                        }
                        set_busy.set(false);
                    });
                }
            >
                {move || if busy.get() { "填表中..." } else { "自动填表" }}
            </button>
            {move || {
                if is_desktop.get() {
                    view! { <span class="badge badge--blue">"桌面端"</span> }.into_any()
                } else {
                    view! { <span class="badge badge--blue">"无头浏览器"</span> }.into_any()
                }
            }}
        </span>
    }
}

#[component]
fn PendingActions<F, G>(id: i64, label: String, on_mutated: F, on_confirm: G) -> impl IntoView
where
    F: Fn() + Copy + Send + Sync + 'static,
    G: Fn(ConfirmKind) + Copy + Send + Sync + 'static,
{
    let (notes, set_notes) = signal(String::new());
    let (show_notes, set_show_notes) = signal(false);
    let (pending_action, set_pending_action) = signal(None::<String>);
    let (acting, set_acting) = signal(false);
    let label_for_reject = label.clone();

    view! {
        <div class="action-group">
            {move || {
                if show_notes.get() {
                    let action = pending_action.get().unwrap_or_default();
                    let action_label = if action == "approve" { "批准" } else { "拒绝" };
                    let action_clone = action.clone();
                    let label_for_reject = label_for_reject.clone();
                    view! {
                        <div class="inline-notes">
                            <input
                                type="text"
                                placeholder="备注（可选）"
                                class="input input--sm"
                                on:input=move |ev| {
                                    set_notes.set(event_target_value(&ev));
                                }
                                prop:value=move || notes.get()
                            />
                            <button
                                class="btn btn--green btn--sm"
                                disabled=move || acting.get()
                                on:click={
                                    let action = action_clone.clone();
                                    move |_| {
                                        let action = action.clone();
                                        let n = notes.get();
                                        let review_notes = if n.is_empty() { None } else { Some(n) };
                                        if action == "reject" {
                                            on_confirm(ConfirmKind::Reject {
                                                id,
                                                label: label_for_reject.clone(),
                                                notes: review_notes,
                                            });
                                            set_show_notes.set(false);
                                            set_pending_action.set(None);
                                            return;
                                        }
                                        set_acting.set(true);
                                        leptos::task::spawn_local(async move {
                                            match review_repost_core(id, action, review_notes).await {
                                                Ok(_) => {
                                                    show_toast("已批准转种", ToastType::Success);
                                                    on_mutated();
                                                }
                                                Err(e) => {
                                                    show_toast(format!("批准失败：{e}"), ToastType::Error)
                                                }
                                            }
                                            set_acting.set(false);
                                        });
                                    }
                                }
                            >
                                {action_label}
                            </button>
                            <button
                                class="btn btn--gray btn--sm"
                                on:click=move |_| {
                                    set_show_notes.set(false);
                                    set_pending_action.set(None);
                                }
                            >
                                "取消"
                            </button>
                        </div>
                    }
                        .into_any()
                } else {
                    view! {
                        <div class="btn-group">
                            <button
                                class="btn btn--green btn--sm"
                                on:click=move |_| {
                                    set_pending_action.set(Some("approve".into()));
                                    set_show_notes.set(true);
                                }
                            >
                                "批准"
                            </button>
                            <button
                                class="btn btn--red btn--sm"
                                on:click=move |_| {
                                    set_pending_action.set(Some("reject".into()));
                                    set_show_notes.set(true);
                                }
                            >
                                "拒绝"
                            </button>
                        </div>
                    }
                        .into_any()
                }
            }}
        </div>
    }
}
