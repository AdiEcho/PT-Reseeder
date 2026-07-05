use crate::server_fns::{login, register};
use leptos::prelude::*;

#[component]
pub fn LoginPage() -> impl IntoView {
    let (error_msg, set_error_msg) = signal(String::new());
    let (is_register, set_is_register) = signal(false);

    let login_action = Action::new(move |(username, password): &(String, String)| {
        let username = username.clone();
        let password = password.clone();
        async move {
            let result = if is_register.get_untracked() {
                register(username, password).await
            } else {
                login(username, password).await
            };
            match result {
                Ok(_) => {
                    // Redirect to dashboard on success
                }
                Err(e) => {
                    set_error_msg.set(e.to_string());
                }
            }
        }
    });

    view! {
        <div class="login-container">
            <h1>"PT-Reseeder"</h1>
            <h2>{move || if is_register.get() { "Register" } else { "Login" }}</h2>

            <form on:submit=move |ev| {
                ev.prevent_default();
                // Extract form values and dispatch action
                let _ = &login_action;
            }>
                <div>
                    <label for="username">"Username"</label>
                    <input type="text" id="username" name="username" required=true />
                </div>
                <div>
                    <label for="password">"Password"</label>
                    <input type="password" id="password" name="password" required=true />
                </div>
                <button type="submit">
                    {move || if is_register.get() { "Register" } else { "Login" }}
                </button>
            </form>

            <p
                class="error"
                style:display=move || {
                    if error_msg.get().is_empty() { "none" } else { "block" }
                }
            >
                {move || error_msg.get()}
            </p>

            <p>
                <a href="#" on:click=move |_| set_is_register.update(|v| *v = !*v)>
                    {move || {
                        if is_register.get() {
                            "Already have account? Login"
                        } else {
                            "No account? Register"
                        }
                    }}
                </a>
            </p>
        </div>
    }
}
