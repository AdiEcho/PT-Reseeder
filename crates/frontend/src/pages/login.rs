use leptos::prelude::*;
use leptos_router::hooks::use_navigate;

#[component]
pub fn LoginPage() -> impl IntoView {
    let (username, set_username) = signal(String::new());
    let (password, set_password) = signal(String::new());
    let (error_msg, set_error_msg) = signal(String::new());
    let (password_confirm, set_password_confirm) = signal(String::new());
    let (confirm_error, set_confirm_error) = signal(String::new());
    let (is_register, set_is_register) = signal(false);
    let navigate = use_navigate();

    // Check whether any user exists; if not, default to register mode.
    let user_exists = Resource::new(|| (), |_| crate::server_fns::has_user());

    // Once the resource resolves, set the initial mode.
    Effect::new(move || {
        if let Some(Ok(exists)) = user_exists.get() {
            if !exists {
                set_is_register.set(true);
            }
        }
    });

    let login_action = Action::new(move |(username, password): &(String, String)| {
        let username = username.clone();
        let password = password.clone();
        let navigate = navigate.clone();
        async move {
            set_confirm_error.set(String::new());
            let result = if is_register.get_untracked() {
                if password != password_confirm.get_untracked() {
                    set_confirm_error.set("两次密码输入不一致".to_string());
                    return;
                }
                crate::server_fns::register(username, password).await
            } else {
                crate::server_fns::login(username, password).await
            };
            match result {
                Ok(_) => {
                    navigate("/dashboard", Default::default());
                }
                Err(e) => {
                    set_error_msg.set(e.to_string());
                }
            }
        }
    });

    let pending = login_action.pending();

    view! {
        <Suspense fallback=|| view! {
            <div class="login-container">"加载中…"</div>
        }>
            {move || {
                // While the resource hasn't resolved, show nothing (Suspense covers it).
                let has_existing_user = match user_exists.get() {
                    Some(Ok(v)) => v,
                    _ => return view! { <div /> }.into_any(),
                };

                view! {
                    <div class="login-container">
                        <div class="login-card">
                            <h1 class="login-title">"PT-Reseeder"</h1>
                            <h2 class="login-subtitle">{move || if is_register.get() { "创建账号" } else { "登录" }}</h2>

                            <form on:submit=move |ev| {
                                ev.prevent_default();
                                login_action.dispatch((username.get_untracked(), password.get_untracked()));
                            }>
                                <div style="margin-bottom: 20px;">
                                    <label for="username" class="login-label">
                                        "用户名" <span class="required">"*"</span>
                                    </label>
                                    <input
                                        type="text"
                                        id="username"
                                        name="username"
                                        class="login-input"
                                        autocomplete="username"
                                        required=true
                                        prop:value=move || username.get()
                                        on:input=move |ev| {
                                            set_username.set(event_target_value(&ev));
                                            set_error_msg.set(String::new());
                                        }
                                    />
                                </div>
                                <div style="margin-bottom: 28px;">
                                    <label for="password" class="login-label">
                                        "密码" <span class="required">"*"</span>
                                    </label>
                                    <input
                                        type="password"
                                        id="password"
                                        name="password"
                                        class="login-input"
                                        autocomplete=move || {
                                            if is_register.get() {
                                                "new-password"
                                            } else {
                                                "current-password"
                                            }
                                        }
                                        required=true
                                        prop:value=move || password.get()
                                        on:input=move |ev| {
                                            set_password.set(event_target_value(&ev));
                                            set_error_msg.set(String::new());
                                        }
                                    />
                                </div>
                                {move || {
                                    if is_register.get() {
                                        Some(view! {
                                            <div style="margin-bottom: 28px;">
                                                <label for="password_confirm" class="login-label">
                                                    "确认密码" <span class="required">"*"</span>
                                                </label>
                                                <input
                                                    type="password"
                                                    id="password_confirm"
                                                    name="password_confirm"
                                                    class="login-input"
                                                    autocomplete="new-password"
                                                    required=true
                                                    prop:value=move || password_confirm.get()
                                                    on:input=move |ev| {
                                                        set_password_confirm.set(event_target_value(&ev));
                                                        set_confirm_error.set(String::new());
                                                        set_error_msg.set(String::new());
                                                    }
                                                />
                                                <p
                                                    class="field-error"
                                                    style:display=move || {
                                                        if confirm_error.get().is_empty() { "none" } else { "block" }
                                                    }
                                                >
                                                    {move || confirm_error.get()}
                                                </p>
                                            </div>
                                        })
                                    } else {
                                        None
                                    }
                                }}
                                <button
                                    type="submit"
                                    class="login-submit"
                                    disabled=move || pending.get()
                                >
                                    {move || {
                                        if pending.get() {
                                            "加载中..."
                                        } else if is_register.get() {
                                            "注册"
                                        } else {
                                            "登录"
                                        }
                                    }}
                                </button>
                            </form>

                            <p
                                class="login-error"
                                style:display=move || {
                                    if error_msg.get().is_empty() { "none" } else { "block" }
                                }
                            >
                                {move || error_msg.get()}
                            </p>

                            // Only show the toggle link when no user exists yet
                            // (allow switching between register/login modes).
                            {move || {
                                if has_existing_user {
                                    view! { <div /> }.into_any()
                                } else {
                                    view! {
                                        <p style="text-align: center; margin-top: 24px;">
                                            <a
                                                href="#"
                                                class="login-toggle"
                                                on:click=move |_| set_is_register.update(|v| *v = !*v)
                                            >
                                                {move || {
                                                    if is_register.get() {
                                                        "已有账号？去登录"
                                                    } else {
                                                        "还没有账号？去注册"
                                                    }
                                                }}
                                            </a>
                                        </p>
                                    }.into_any()
                                }
                            }}
                        </div>
                    </div>
                }.into_any()
            }}
        </Suspense>
    }
}
