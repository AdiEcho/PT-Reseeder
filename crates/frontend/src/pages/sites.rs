use leptos::prelude::*;

#[component]
pub fn SitesPage() -> impl IntoView {
    let (show_form, set_show_form) = signal(false);
    let (confirm_delete_id, set_confirm_delete_id) = signal(None::<i64>);
    let (edit_url_site, set_edit_url_site) = signal(None::<(i64, String, String)>);

    // Form field signals
    let (selected_preset, set_selected_preset) = signal(String::new());
    let (name, set_name) = signal(String::new());
    let (url, set_url) = signal(String::new());
    let (api_url, set_api_url) = signal(String::new());
    let (adapter_type, set_adapter_type) = signal("NexusPHP".to_string());
    let (auth_type, set_auth_type) = signal("cookie".to_string());
    let (cookie, set_cookie) = signal(String::new());
    let (passkey, set_passkey) = signal(String::new());
    let (is_custom, set_is_custom) = signal(true);

    // Edit URL form signals
    let (edit_url, set_edit_url) = signal(String::new());
    let (edit_api_url, set_edit_api_url) = signal(String::new());

    // Load sites list
    let sites = Resource::new(|| (), |_| crate::server_fns::get_sites());

    // Load site definitions for preset selector
    let definitions = Resource::new(|| (), |_| crate::server_fns::get_site_definitions());

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

    // Update site URL action
    let update_url_action = Action::new(move |args: &(i64, String, String)| {
        let (id, u, au) = args.clone();
        async move { crate::server_fns::update_site_url(id, u, au).await }
    });

    // Refetch sites after create/delete/probe/update
    Effect::new(move |_| {
        if create_action.value().get().is_some() {
            sites.refetch();
            // Reset form
            set_selected_preset.set(String::new());
            set_name.set(String::new());
            set_url.set(String::new());
            set_api_url.set(String::new());
            set_adapter_type.set("NexusPHP".to_string());
            set_auth_type.set("cookie".to_string());
            set_cookie.set(String::new());
            set_passkey.set(String::new());
            set_is_custom.set(true);
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

    Effect::new(move |_| {
        if update_url_action.value().get().is_some() {
            sites.refetch();
            set_edit_url_site.set(None);
        }
    });

    view! {
        <div class="dashboard">
            <div class="dashboard-header">
                <h1>"站点管理"</h1>
                <button
                    class="btn btn-primary"
                    on:click=move |_| set_show_form.update(|v| *v = !*v)
                >
                    {move || if show_form.get() { "取消" } else { "添加站点" }}
                </button>
            </div>

            // Add Site form
            {move || {
                if show_form.get() {
                    view! {
                        <div class="form-section">
                            <h2>"添加新站点"</h2>
                            <div class="form-grid">
                                // Preset selector
                                <div class="form-group" style="grid-column: 1 / -1;">
                                    <label>"选择站点预设"</label>
                                    <Suspense fallback=move || view! { <select disabled=true><option>"加载中..."</option></select> }>
                                        {move || {
                                            definitions.get().map(|result| match result {
                                                Ok(defs) => {
                                                    let defs_for_change = defs.clone();
                                                    view! {
                                                        <select
                                                            prop:value=move || selected_preset.get()
                                                            on:change=move |ev| {
                                                                let val = event_target_value(&ev);
                                                                set_selected_preset.set(val.clone());
                                                                if val == "__custom__" || val.is_empty() {
                                                                    set_is_custom.set(true);
                                                                    if val == "__custom__" {
                                                                        set_name.set(String::new());
                                                                        set_url.set(String::new());
                                                                        set_api_url.set(String::new());
                                                                        set_adapter_type.set("NexusPHP".to_string());
                                                                    }
                                                                } else if let Some(def) = defs_for_change.iter().find(|d| d.id == val) {
                                                                    set_is_custom.set(false);
                                                                    set_name.set(def.name.clone());
                                                                    set_url.set(def.url.clone());
                                                                    set_api_url.set(def.api_url.clone().unwrap_or_default());
                                                                    // Capitalize adapter for display
                                                                    let adapter_display = match def.adapter.as_str() {
                                                                        "nexusphp" => "NexusPHP",
                                                                        "unit3d" => "Unit3D",
                                                                        "gazelle" => "Gazelle",
                                                                        "zhuque" => "Zhuque",
                                                                        "mteam" => "MTeam",
                                                                        other => other,
                                                                    };
                                                                    set_adapter_type.set(adapter_display.to_string());
                                                                }
                                                            }
                                                        >
                                                            <option value="">"-- 请选择站点 --"</option>
                                                            {defs.into_iter().map(|def| {
                                                                let id = def.id.clone();
                                                                let label = format!("{} ({})", def.name, def.adapter);
                                                                view! {
                                                                    <option value=id>{label}</option>
                                                                }
                                                            }).collect::<Vec<_>>()}
                                                            <option value="__custom__">"🔧 自定义站点"</option>
                                                        </select>
                                                    }
                                                    .into_any()
                                                }
                                                Err(e) => {
                                                    view! {
                                                        <p class="error">{format!("预设加载失败：{e}")}</p>
                                                    }
                                                    .into_any()
                                                }
                                            })
                                        }}
                                    </Suspense>
                                </div>
                                <div class="form-group">
                                    <label>"名称"</label>
                                    <input
                                        type="text"
                                        placeholder="站点名称"
                                        prop:value=move || name.get()
                                        disabled=move || !is_custom.get()
                                        on:input=move |ev| {
                                            set_name.set(event_target_value(&ev))
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
                                            set_url.set(event_target_value(&ev))
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
                                            set_api_url.set(event_target_value(&ev))
                                        }
                                    />
                                </div>
                                <div class="form-group">
                                    <label>"站点架构"</label>
                                    <select
                                        prop:value=move || adapter_type.get()
                                        disabled=move || !is_custom.get()
                                        on:change=move |ev| {
                                            set_adapter_type.set(event_target_value(&ev))
                                        }
                                    >
                                        <option value="NexusPHP">"NexusPHP"</option>
                                        <option value="Unit3D">"Unit3D"</option>
                                        <option value="Gazelle">"Gazelle"</option>
                                        <option value="MTeam">"MTeam"</option>
                                        <option value="Zhuque">"Zhuque"</option>
                                        <option value="Luminance">"Luminance"</option>
                                        <option value="Generic">"Generic"</option>
                                    </select>
                                </div>
                                <div class="form-group">
                                    <label>"认证方式"</label>
                                    <select
                                        prop:value=move || auth_type.get()
                                        on:change=move |ev| {
                                            set_auth_type.set(event_target_value(&ev))
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
                                        placeholder="会话 Cookie"
                                        prop:value=move || cookie.get()
                                        on:input=move |ev| {
                                            set_cookie.set(event_target_value(&ev))
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
                                            set_passkey.set(event_target_value(&ev))
                                        }
                                    />
                                </div>
                            </div>
                            <div class="form-actions">
                                <button
                                    class="btn btn-primary"
                                    on:click=move |_| { create_action.dispatch(()); }
                                >
                                    "创建站点"
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
                        view! { <p class="error">{format!("创建失败：{e}")}</p> }
                    })
            }}
            {move || {
                delete_action
                    .value()
                    .get()
                    .and_then(|r| r.err())
                    .map(|e| {
                        view! { <p class="error">{format!("删除失败：{e}")}</p> }
                    })
            }}
            {move || {
                probe_action
                    .value()
                    .get()
                    .and_then(|r| r.err())
                    .map(|e| {
                        view! { <p class="error">{format!("连通测试失败：{e}")}</p> }
                    })
            }}
            {move || {
                update_url_action
                    .value()
                    .get()
                    .and_then(|r| r.err())
                    .map(|e| {
                        view! { <p class="error">{format!("URL 更新失败：{e}")}</p> }
                    })
            }}
            {move || {
                update_url_action
                    .value()
                    .get()
                    .and_then(|r| r.ok())
                    .map(|_| {
                        view! { <div class="form-alert form-alert--success">"站点 URL 已更新"</div> }
                    })
            }}

            // Sites table
            <Suspense fallback=move || view! { <p>"正在加载站点..."</p> }>
                {move || {
                    sites
                        .get()
                        .map(|result| match result {
                            Ok(sites_list) => {
                                if sites_list.is_empty() {
                                    view! {
                                        <div class="stats-table-section">
                                            <p>"尚未配置任何站点，请在上方添加。"</p>
                                        </div>
                                    }
                                        .into_any()
                                } else {
                                    view! {
                                        <div class="stats-table-section">
                                            <h2>"站点列表"</h2>
                                            <div class="table-wrap">
                                                <table class="stats-table">
                                                    <thead>
                                                        <tr>
                                                            <th>"名称"</th>
                                                            <th>"URL"</th>
                                                            <th>"架构"</th>
                                                            <th>"连通状态"</th>
                                                            <th>"启用"</th>
                                                            <th>"操作"</th>
                                                        </tr>
                                                    </thead>
                                                    <tbody>
                                                        {sites_list
                                                            .into_iter()
                                                            .map(|site| {
                                                                let site_id = site.id;
                                                                let site_url = site.url.clone();
                                                                let site_api_url = site.api_url.clone().unwrap_or_default();
                                                                let detail_href = format!(
                                                                    "/sites/{}",
                                                                    site.id,
                                                                );
                                                                let (probe_class, probe_label) = match site
                                                                    .probe_status
                                                                    .as_str()
                                                                {
                                                                    "ok" => ("text-green", "正常"),
                                                                    "failed" => ("text-red", "失败"),
                                                                    "pending" => ("text-muted", "检测中"),
                                                                    _ => ("text-muted", "未知"),
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
                                                                                view! { <span class="text-green">"是"</span> }
                                                                                    .into_any()
                                                                            } else {
                                                                                view! { <span class="text-red">"否"</span> }
                                                                                    .into_any()
                                                                            }}
                                                                        </td>
                                                                        <td class="actions-cell">
                                                                            <button
                                                                                class="btn btn--sm btn--outline"
                                                                                on:click=move |_| {
                                                                                    set_edit_url.set(site_url.clone());
                                                                                    set_edit_api_url.set(site_api_url.clone());
                                                                                    set_edit_url_site.set(Some((site_id, site_url.clone(), site_api_url.clone())));
                                                                                }
                                                                            >
                                                                                "编辑URL"
                                                                            </button>
                                                                            <button
                                                                                class="btn btn--sm btn--outline"
                                                                                on:click=move |_| { probe_action.dispatch(site_id); }
                                                                            >
                                                                                "连通测试"
                                                                            </button>
                                                                            <button
                                                                                class="btn btn--sm btn--danger"
                                                                                on:click=move |_| { set_confirm_delete_id.set(Some(site_id)); }
                                                                            >
                                                                                "删除"
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
                                    <p class="error">{format!("站点加载失败：{e}")}</p>
                                }
                                    .into_any()
                            }
                        })
                }}
            </Suspense>

            // Edit URL dialog
            {move || {
                edit_url_site.get().map(|(edit_id, _, _)| {
                    view! {
                        <div class="confirm-overlay">
                            <div class="confirm-dialog">
                                <h3>"编辑站点 URL"</h3>
                                <div class="form-grid">
                                    <div class="form-group">
                                        <label>"URL"</label>
                                        <input
                                            type="text"
                                            prop:value=move || edit_url.get()
                                            on:input=move |ev| set_edit_url.set(event_target_value(&ev))
                                        />
                                    </div>
                                    <div class="form-group">
                                        <label>"API URL"</label>
                                        <input
                                            type="text"
                                            prop:value=move || edit_api_url.get()
                                            on:input=move |ev| set_edit_api_url.set(event_target_value(&ev))
                                        />
                                    </div>
                                </div>
                                <div class="form-actions">
                                    <button
                                        class="btn btn--outline"
                                        on:click=move |_| set_edit_url_site.set(None)
                                    >
                                        "取消"
                                    </button>
                                    <button
                                        class="btn btn-primary"
                                        on:click=move |_| {
                                            update_url_action.dispatch((edit_id, edit_url.get_untracked(), edit_api_url.get_untracked()));
                                        }
                                    >
                                        "保存"
                                    </button>
                                </div>
                            </div>
                        </div>
                    }
                })
            }}

            // Delete confirmation dialog
            {move || {
                confirm_delete_id.get().map(|del_id| {
                    view! {
                        <div class="confirm-overlay">
                            <div class="confirm-dialog">
                                <h3>"确认删除"</h3>
                                <p>"确定要删除该站点吗？此操作不可撤销。"</p>
                                <div class="form-actions">
                                    <button
                                        class="btn btn--outline"
                                        on:click=move |_| set_confirm_delete_id.set(None)
                                    >
                                        "取消"
                                    </button>
                                    <button
                                        class="btn btn--danger"
                                        on:click=move |_| {
                                            delete_action.dispatch(del_id);
                                            set_confirm_delete_id.set(None);
                                        }
                                    >
                                        "确认删除"
                                    </button>
                                </div>
                            </div>
                        </div>
                    }
                })
            }}
        </div>
    }
}
