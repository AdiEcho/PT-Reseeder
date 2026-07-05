use crate::pages::dashboard::DashboardPage;
use crate::pages::login::LoginPage;
use leptos::prelude::*;
use leptos_router::components::*;
use leptos_router::path;

#[component]
pub fn App() -> impl IntoView {
    view! {
        <Router>
            <main>
                <Routes fallback=|| "Page not found">
                    <Route path=path!("/login") view=LoginPage />
                    <Route path=path!("/dashboard") view=DashboardPage />
                    <Route path=path!("/") view=|| view! { <Redirect path="/dashboard" /> } />
                </Routes>
            </main>
        </Router>
    }
}
