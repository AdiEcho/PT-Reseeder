use crate::server_fns::{
    delete_repost, get_repost_queue, review_repost, submit_repost, RepostEntry,
};
use leptos::prelude::*;
#[cfg(target_arch = "wasm32")]
use serde_json::json;

const STATUSES: &[(&str, Option<&str>)] = &[
    ("All", None),
    ("Pending", Some("pending")),
    ("Approved", Some("approved")),
    ("Submitted", Some("submitted")),
    ("Failed", Some("failed")),
    ("Rejected", Some("rejected")),
];

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

    // A version counter bumped after every mutation so the resource refetches.
    let (version, set_version) = signal(0u32);

    let queue = Resource::new(
        move || (status_filter.get(), version.get()),
        |(filter, _ver)| get_repost_queue(filter),
    );

    let refetch = move || set_version.update(|v| *v += 1);

    view! {
        <div class="dashboard">
            <div class="dashboard-header">
                <h1>"Repost Queue"</h1>
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

            <Suspense fallback=move || {
                view! { <p>"Loading repost queue..."</p> }
            }>
                {move || {
                    queue
                        .get()
                        .map(|result| {
                            match result {
                                Err(e) => {
                                    view! {
                                        <p class="error">
                                            {format!("Failed to load repost queue: {e}")}
                                        </p>
                                    }
                                        .into_any()
                                }
                                Ok(entries) => {
                                    if entries.is_empty() {
                                        view! { <p>"No entries in the queue."</p> }.into_any()
                                    } else {
                                        view! {
                                            <RepostTable entries=entries on_mutated=refetch />
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
fn RepostTable<F>(entries: Vec<RepostEntry>, on_mutated: F) -> impl IntoView
where
    F: Fn() + Copy + Send + Sync + 'static,
{
    view! {
        <div class="stats-table-section">
            <div class="table-wrap">
                <table class="stats-table">
                    <thead>
                        <tr>
                            <th>"Source Site"</th>
                            <th>"Torrent ID"</th>
                            <th>"Target Site"</th>
                            <th>"Status"</th>
                            <th>"Notes"</th>
                            <th>"Submitted"</th>
                            <th>"Created"</th>
                            <th>"Actions"</th>
                        </tr>
                    </thead>
                    <tbody>
                        {entries
                            .into_iter()
                            .map(|entry| {
                                view! { <RepostRow entry=entry on_mutated=on_mutated /> }
                            })
                            .collect::<Vec<_>>()}
                    </tbody>
                </table>
            </div>
        </div>
    }
}

#[component]
fn RepostRow<F>(entry: RepostEntry, on_mutated: F) -> impl IntoView
where
    F: Fn() + Copy + Send + Sync + 'static,
{
    let id = entry.id;
    let status = entry.status.clone();
    let badge_class = status_badge_class(&status);

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

    let notes_display = entry.review_notes.clone().unwrap_or_default();

    view! {
        <tr>
            <td>{entry.source_site_name.clone()}</td>
            <td>{entry.source_torrent_id.clone()}</td>
            <td>{entry.target_site_name.clone()}</td>
            <td>
                <span class=badge_class>{status.clone()}</span>
            </td>
            <td class="text-muted">{notes_display}</td>
            <td class="text-muted">{submitted_display}</td>
            <td class="text-muted">{created_display}</td>
            <td>
                <RowActions id=id status=status on_mutated=on_mutated />
            </td>
        </tr>
    }
}

#[component]
fn RowActions<F>(id: i64, status: String, on_mutated: F) -> impl IntoView
where
    F: Fn() + Copy + Send + Sync + 'static,
{
    match status.as_str() {
        "pending" => view! { <PendingActions id=id on_mutated=on_mutated /> }.into_any(),
        "approved" => view! {
            <div class="btn-group">
                <button
                    class="btn btn--green btn--sm"
                    on:click=move |_| {
                        leptos::task::spawn_local(async move {
                            let _ = submit_repost_core(id).await;
                            on_mutated();
                        });
                    }
                >
                    "Submit"
                </button>
                <DesktopAutofillAction id=id />
            </div>
        }
        .into_any(),
        "failed" => view! {
            <button
                class="btn btn--blue btn--sm"
                on:click=move |_| {
                    leptos::task::spawn_local(async move {
                        let _ = review_repost_core(id, "approved".into(), None).await;
                        on_mutated();
                    });
                }
            >
                "Retry"
            </button>
        }
        .into_any(),
        "submitted" => view! { <span class="text-green">"✓"</span> }.into_any(),
        "rejected" => view! {
            <button
                class="btn btn--red btn--sm"
                on:click=move |_| {
                    leptos::task::spawn_local(async move {
                        let _ = delete_repost(id).await;
                        on_mutated();
                    });
                }
            >
                "Delete"
            </button>
        }
        .into_any(),
        _ => view! { <span class="text-muted">"-"</span> }.into_any(),
    }
}

#[component]
fn DesktopAutofillAction(id: i64) -> impl IntoView {
    let (is_desktop, set_is_desktop) = signal(false);
    let (message, set_message) = signal(None::<String>);

    #[cfg(target_arch = "wasm32")]
    Effect::new(move |_| {
        set_is_desktop.set(is_desktop_webview());
    });

    #[cfg(not(target_arch = "wasm32"))]
    let _ = set_is_desktop;

    view! {
        {move || {
            if is_desktop.get() {
                view! {
                    <span class="desktop-autofill">
                        <button
                            class="btn btn--blue btn--sm"
                            title="Desktop WebView autofill hook"
                            on:click=move |_| {
                                leptos::task::spawn_local(async move {
                                    match inject_desktop_autofill(id).await {
                                        Ok(()) => set_message.set(Some("Autofill event sent".to_string())),
                                        Err(e) => set_message.set(Some(format!("Autofill unavailable: {e}"))),
                                    }
                                });
                            }
                        >
                            "Autofill"
                        </button>
                        <span class="badge badge--blue">"Desktop WebView"</span>
                        {move || {
                            message
                                .get()
                                .map(|msg| view! { <span class="text-muted">{msg}</span> })
                        }}
                    </span>
                }
                    .into_any()
            } else {
                view! {}.into_any()
            }
        }}
    }
}

#[component]
fn PendingActions<F>(id: i64, on_mutated: F) -> impl IntoView
where
    F: Fn() + Copy + Send + Sync + 'static,
{
    let (notes, set_notes) = signal(String::new());
    let (show_notes, set_show_notes) = signal(false);
    // Which action is pending: "approve" or "reject"
    let (pending_action, set_pending_action) = signal(None::<String>);

    view! {
        <div class="action-group">
            {move || {
                if show_notes.get() {
                    let action = pending_action.get().unwrap_or_default();
                    let action_label = if action == "approve" { "Approve" } else { "Reject" };
                    let action_clone = action.clone();
                    view! {
                        <div class="inline-notes">
                            <input
                                type="text"
                                placeholder="Notes (optional)"
                                class="input input--sm"
                                on:input=move |ev| {
                                    set_notes
                                        .set(event_target_value(&ev));
                                }
                                prop:value=move || notes.get()
                            />
                            <button
                                class="btn btn--green btn--sm"
                                on:click={
                                    let action = action_clone.clone();
                                    move |_| {
                                        let action = action.clone();
                                        let n = notes.get();
                                        let review_notes = if n.is_empty() { None } else { Some(n) };
                                        leptos::task::spawn_local(async move {
                                            let _ = review_repost_core(id, action, review_notes).await;
                                            on_mutated();
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
                                "Cancel"
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
                                "Approve"
                            </button>
                            <button
                                class="btn btn--red btn--sm"
                                on:click=move |_| {
                                    set_pending_action.set(Some("reject".into()));
                                    set_show_notes.set(true);
                                }
                            >
                                "Reject"
                            </button>
                        </div>
                    }
                        .into_any()
                }
            }}
        </div>
    }
}
