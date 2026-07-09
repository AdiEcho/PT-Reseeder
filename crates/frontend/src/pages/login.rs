use leptos::prelude::*;
use leptos_router::hooks::use_navigate;

#[component]
pub fn LoginPage() -> impl IntoView {
    let (username, set_username) = signal(String::new());
    let (password, set_password) = signal(String::new());
    let (error_msg, set_error_msg) = signal(String::new());
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
            let result = if is_register.get_untracked() {
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
                                    <label for="username" class="login-label">"用户名"</label>
                                    <input
                                        type="text"
                                        id="username"
                                        name="username"
                                        class="login-input"
                                        required=true
                                        prop:value=move || username.get()
                                        on:input=move |ev| set_username.set(event_target_value(&ev))
                                    />
                                </div>
                                <div style="margin-bottom: 28px;">
                                    <label for="password" class="login-label">"密码"</label>
                                    <input
                                        type="password"
                                        id="password"
                                        name="password"
                                        class="login-input"
                                        required=true
                                        prop:value=move || password.get()
                                        on:input=move |ev| set_password.set(event_target_value(&ev))
                                    />
                                </div>
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
