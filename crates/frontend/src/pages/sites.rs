use leptos::prelude::*;

#[component]
pub fn SitesPage() -> impl IntoView {
    let (show_form, set_show_form) = signal(false);

    // Form field signals
    let (name, set_name) = signal(String::new());
    let (url, set_url) = signal(String::new());
    let (api_url, set_api_url) = signal(String::new());
    let (adapter_type, set_adapter_type) = signal("NexusPHP".to_string());
    let (auth_type, set_auth_type) = signal("cookie".to_string());
    let (cookie, set_cookie) = signal(String::new());
    let (passkey, set_passkey) = signal(String::new());

    // Load sites list
    let sites = Resource::new(|| (), |_| crate::server_fns::get_sites());

    // Create site action
    let create_action = Action::new(move |_: &()| {
        let n = name.get_untracked();
        let u = url.get_untracked();
        let au = api_url.get_untracked();
        let at = adapter_type.get_untracked();
        let aht = auth_type.get_untracked();
        let c = cookie.get_untracked();
        let p = passkey.get_untracked();
        async move { crate::server_fns::create_site(n, u, au, at, aht, c, p).await }
    });

    // Delete site action
    let delete_action = Action::new(move |id: &i64| {
        let id = *id;
        async move { crate::server_fns::delete_site(id).await }
    });

    // Probe site action
    let probe_action = Action::new(move |id: &i64| {
        let id = *id;
        async move { crate::server_fns::probe_site(id).await }
    });

    // Refetch sites after create/delete/probe
    Effect::new(move |_| {
        if create_action.value().get().is_some() {
            sites.refetch();
            // Reset form
            set_name.set(String::new());
            set_url.set(String::new());
            set_api_url.set(String::new());
            set_adapter_type.set("NexusPHP".to_string());
            set_auth_type.set("cookie".to_string());
            set_cookie.set(String::new());
            set_passkey.set(String::new());
            set_show_form.set(false);
        }
    });

    Effect::new(move |_| {
        if delete_action.value().get().is_some() {
            sites.refetch();
        }
    });

    Effect::new(move |_| {
        if probe_action.value().get().is_some() {
            sites.refetch();
        }
    });

    view! {
        <div class="dashboard">
            <div class="dashboard-header">
                <h1>"Site Management"</h1>
                <button
                    class="btn btn-primary"
                    on:click=move |_| set_show_form.update(|v| *v = !*v)
                >
                    {move || if show_form.get() { "Cancel" } else { "Add Site" }}
                </button>
            </div>

            // Add Site form
            {move || {
                if show_form.get() {
                    view! {
                        <div class="form-section">
                            <h2>"Add New Site"</h2>
                            <div class="form-grid">
                                <div class="form-group">
                                    <label>"Name"</label>
                                    <input
                                        type="text"
                                        placeholder="Site name"
                                        prop:value=move || name.get()
                                        on:input=move |ev| {
                                            set_name
                                                .set(
                                                    event_target_value(&ev),
                                                )
                                        }
                                    />
                                </div>
                                <div class="form-group">
                                    <label>"URL"</label>
                                    <input
                                        type="text"
                                        placeholder="https://example.com"
                                        prop:value=move || url.get()
                                        on:input=move |ev| {
                                            set_url
                                                .set(
                                                    event_target_value(&ev),
                                                )
                                        }
                                    />
                                </div>
                                <div class="form-group">
                                    <label>"API URL"</label>
                                    <input
                                        type="text"
                                        placeholder="https://example.com/api"
                                        prop:value=move || api_url.get()
                                        on:input=move |ev| {
                                            set_api_url
                                                .set(
                                                    event_target_value(&ev),
                                                )
                                        }
                                    />
                                </div>
                                <div class="form-group">
                                    <label>"Adapter Type"</label>
                                    <select
                                        prop:value=move || adapter_type.get()
                                        on:change=move |ev| {
                                            set_adapter_type
                                                .set(
                                                    event_target_value(&ev),
                                                )
                                        }
                                    >
                                        <option value="NexusPHP">"NexusPHP"</option>
                                        <option value="Unit3D">"Unit3D"</option>
                                        <option value="Gazelle">"Gazelle"</option>
                                        <option value="Luminance">"Luminance"</option>
                                        <option value="Generic">"Generic"</option>
                                    </select>
                                </div>
                                <div class="form-group">
                                    <label>"Auth Type"</label>
                                    <select
                                        prop:value=move || auth_type.get()
                                        on:change=move |ev| {
                                            set_auth_type
                                                .set(
                                                    event_target_value(&ev),
                                                )
                                        }
                                    >
                                        <option value="cookie">"Cookie"</option>
                                        <option value="passkey">"Passkey"</option>
                                        <option value="apikey">"API Key"</option>
                                    </select>
                                </div>
                                <div class="form-group">
                                    <label>"Cookie"</label>
                                    <input
                                        type="text"
                                        placeholder="Session cookie"
                                        prop:value=move || cookie.get()
                                        on:input=move |ev| {
                                            set_cookie
                                                .set(
                                                    event_target_value(&ev),
                                                )
                                        }
                                    />
                                </div>
                                <div class="form-group">
                                    <label>"Passkey"</label>
                                    <input
                                        type="text"
                                        placeholder="Passkey"
                                        prop:value=move || passkey.get()
                                        on:input=move |ev| {
                                            set_passkey
                                                .set(
                                                    event_target_value(&ev),
                                                )
                                        }
                                    />
                                </div>
                            </div>
                            <div class="form-actions">
                                <button
                                    class="btn btn-primary"
                                    on:click=move |_| { create_action.dispatch(()); }
                                >
                                    "Create Site"
                                </button>
                            </div>
                        </div>
                    }
                        .into_any()
                } else {
                    view! { <div></div> }.into_any()
                }
            }}

            // Error display for actions
            {move || {
                create_action
                    .value()
                    .get()
                    .and_then(|r| r.err())
                    .map(|e| {
                        view! { <p class="error">{format!("Create failed: {e}")}</p> }
                    })
            }}
            {move || {
                delete_action
                    .value()
                    .get()
                    .and_then(|r| r.err())
                    .map(|e| {
                        view! { <p class="error">{format!("Delete failed: {e}")}</p> }
                    })
            }}
            {move || {
                probe_action
                    .value()
                    .get()
                    .and_then(|r| r.err())
                    .map(|e| {
                        view! { <p class="error">{format!("Probe failed: {e}")}</p> }
                    })
            }}

            // Sites table
            <Suspense fallback=move || view! { <p>"Loading sites..."</p> }>
                {move || {
                    sites
                        .get()
                        .map(|result| match result {
                            Ok(sites_list) => {
                                if sites_list.is_empty() {
                                    view! {
                                        <div class="stats-table-section">
                                            <p>"No sites configured yet. Add one above."</p>
                                        </div>
                                    }
                                        .into_any()
                                } else {
                                    view! {
                                        <div class="stats-table-section">
                                            <h2>"Sites"</h2>
                                            <div class="table-wrap">
                                                <table class="stats-table">
                                                    <thead>
                                                        <tr>
                                                            <th>"Name"</th>
                                                            <th>"URL"</th>
                                                            <th>"Adapter"</th>
                                                            <th>"Probe Status"</th>
                                                            <th>"Enabled"</th>
                                                            <th>"Actions"</th>
                                                        </tr>
                                                    </thead>
                                                    <tbody>
                                                        {sites_list
                                                            .into_iter()
                                                            .map(|site| {
                                                                let site_id = site.id;
                                                                let detail_href = format!(
                                                                    "/sites/{}",
                                                                    site.id,
                                                                );
                                                                let (probe_class, probe_label) = match site
                                                                    .probe_status
                                                                    .as_str()
                                                                {
                                                                    "ok" => ("text-green", "OK"),
                                                                    "failed" => ("text-red", "Failed"),
                                                                    "pending" => ("text-muted", "Pending"),
                                                                    _ => ("text-muted", "Unknown"),
                                                                };
                                                                view! {
                                                                    <tr>
                                                                        <td>
                                                                            <a href=detail_href>{site.name}</a>
                                                                        </td>
                                                                        <td class="text-muted">{site.url}</td>
                                                                        <td>{site.adapter_type}</td>
                                                                        <td class=probe_class>{probe_label}</td>
                                                                        <td>
                                                                            {if site.enabled {
                                                                                view! { <span class="text-green">"Yes"</span> }
                                                                                    .into_any()
                                                                            } else {
                                                                                view! { <span class="text-red">"No"</span> }
                                                                                    .into_any()
                                                                            }}
                                                                        </td>
                                                                        <td class="actions-cell">
                                                                            <button
                                                                                class="btn btn-sm btn-outline"
                                                                                on:click=move |_| { probe_action.dispatch(site_id); }
                                                                            >
                                                                                "Probe"
                                                                            </button>
                                                                            <button
                                                                                class="btn btn-sm btn-danger"
                                                                                on:click=move |_| { delete_action.dispatch(site_id); }
                                                                            >
                                                                                "Delete"
                                                                            </button>
                                                                        </td>
                                                                    </tr>
                                                                }
                                                            })
                                                            .collect::<Vec<_>>()}
                                                    </tbody>
                                                </table>
                                            </div>
                                        </div>
                                    }
                                        .into_any()
                                }
                            }
                            Err(e) => {
                                view! {
                                    <p class="error">{format!("Failed to load sites: {e}")}</p>
                                }
                                    .into_any()
                            }
                        })
                }}
            </Suspense>
        </div>
    }
}
