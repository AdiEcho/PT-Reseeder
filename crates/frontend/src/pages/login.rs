use leptos::prelude::*;
use leptos_router::hooks::use_navigate;

#[component]
pub fn LoginPage() -> impl IntoView {
    let (username, set_username) = signal(String::new());
    let (password, set_password) = signal(String::new());
    let (error_msg, set_error_msg) = signal(String::new());
    let (is_register, set_is_register) = signal(false);
    let navigate = use_navigate();

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
        <div class="login-container" style="
            display: flex;
            justify-content: center;
            align-items: center;
            min-height: 100vh;
            background: linear-gradient(135deg, #1a1a2e 0%, #16213e 50%, #0f3460 100%);
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
        ">
            <div class="login-card" style="
                background: rgba(255, 255, 255, 0.05);
                backdrop-filter: blur(10px);
                border: 1px solid rgba(255, 255, 255, 0.1);
                border-radius: 16px;
                padding: 48px 40px;
                width: 100%;
                max-width: 400px;
                box-shadow: 0 8px 32px rgba(0, 0, 0, 0.3);
            ">
                <h1 style="
                    color: #e2e8f0;
                    text-align: center;
                    font-size: 28px;
                    font-weight: 700;
                    margin: 0 0 8px 0;
                    letter-spacing: -0.5px;
                ">"PT-Reseeder"</h1>
                <h2 style="
                    color: #94a3b8;
                    text-align: center;
                    font-size: 16px;
                    font-weight: 400;
                    margin: 0 0 32px 0;
                ">{move || if is_register.get() { "Create Account" } else { "Sign In" }}</h2>

                <form on:submit=move |ev| {
                    ev.prevent_default();
                    login_action.dispatch((username.get_untracked(), password.get_untracked()));
                }>
                    <div style="margin-bottom: 20px;">
                        <label for="username" style="
                            display: block;
                            color: #94a3b8;
                            font-size: 13px;
                            font-weight: 500;
                            margin-bottom: 6px;
                            text-transform: uppercase;
                            letter-spacing: 0.5px;
                        ">"Username"</label>
                        <input
                            type="text"
                            id="username"
                            name="username"
                            required=true
                            prop:value=move || username.get()
                            on:input=move |ev| set_username.set(event_target_value(&ev))
                            style="
                                width: 100%;
                                padding: 12px 16px;
                                background: rgba(255, 255, 255, 0.07);
                                border: 1px solid rgba(255, 255, 255, 0.15);
                                border-radius: 8px;
                                color: #e2e8f0;
                                font-size: 15px;
                                outline: none;
                                transition: border-color 0.2s;
                                box-sizing: border-box;
                            "
                        />
                    </div>
                    <div style="margin-bottom: 28px;">
                        <label for="password" style="
                            display: block;
                            color: #94a3b8;
                            font-size: 13px;
                            font-weight: 500;
                            margin-bottom: 6px;
                            text-transform: uppercase;
                            letter-spacing: 0.5px;
                        ">"Password"</label>
                        <input
                            type="password"
                            id="password"
                            name="password"
                            required=true
                            prop:value=move || password.get()
                            on:input=move |ev| set_password.set(event_target_value(&ev))
                            style="
                                width: 100%;
                                padding: 12px 16px;
                                background: rgba(255, 255, 255, 0.07);
                                border: 1px solid rgba(255, 255, 255, 0.15);
                                border-radius: 8px;
                                color: #e2e8f0;
                                font-size: 15px;
                                outline: none;
                                transition: border-color 0.2s;
                                box-sizing: border-box;
                            "
                        />
                    </div>
                    <button
                        type="submit"
                        disabled=move || pending.get()
                        style="
                            width: 100%;
                            padding: 12px;
                            background: linear-gradient(135deg, #3b82f6, #2563eb);
                            border: none;
                            border-radius: 8px;
                            color: white;
                            font-size: 15px;
                            font-weight: 600;
                            cursor: pointer;
                            transition: opacity 0.2s;
                            letter-spacing: 0.3px;
                        "
                    >
                        {move || {
                            if pending.get() {
                                "Loading..."
                            } else if is_register.get() {
                                "Register"
                            } else {
                                "Login"
                            }
                        }}
                    </button>
                </form>

                <p
                    class="error"
                    style:display=move || {
                        if error_msg.get().is_empty() { "none" } else { "block" }
                    }
                    style="
                        color: #f87171;
                        background: rgba(248, 113, 113, 0.1);
                        border: 1px solid rgba(248, 113, 113, 0.2);
                        border-radius: 8px;
                        padding: 10px 14px;
                        margin-top: 16px;
                        font-size: 14px;
                        text-align: center;
                    "
                >
                    {move || error_msg.get()}
                </p>

                <p style="
                    text-align: center;
                    margin-top: 24px;
                ">
                    <a
                        href="#"
                        on:click=move |_| set_is_register.update(|v| *v = !*v)
                        style="
                            color: #60a5fa;
                            text-decoration: none;
                            font-size: 14px;
                            transition: color 0.2s;
                        "
                    >
                        {move || {
                            if is_register.get() {
                                "Already have an account? Sign in"
                            } else {
                                "No account? Register"
                            }
                        }}
                    </a>
                </p>
            </div>
        </div>
    }
}
