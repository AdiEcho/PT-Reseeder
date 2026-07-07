use crate::pages::dashboard::DashboardPage;
use crate::pages::downloaders::DownloadersPage;
use crate::pages::folders::FoldersPage;
use crate::pages::login::LoginPage;
use crate::pages::repost::RepostPage;
use crate::pages::settings::SettingsPage;
use crate::pages::site_detail::SiteDetailPage;
use crate::pages::sites::SitesPage;
use crate::pages::tasks::TasksPage;
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
                    <Route path=path!("/sites") view=SitesPage />
                    <Route path=path!("/sites/:id") view=SiteDetailPage />
                    <Route path=path!("/downloaders") view=DownloadersPage />
                    <Route path=path!("/tasks") view=TasksPage />
                    <Route path=path!("/folders") view=FoldersPage />
                    <Route path=path!("/repost") view=RepostPage />
                    <Route path=path!("/settings") view=SettingsPage />
                    <Route path=path!("/") view=|| view! { <Redirect path="/dashboard" /> } />
                </Routes>
            </main>
        </Router>
    }
}
