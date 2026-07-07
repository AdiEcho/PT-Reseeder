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
#[cfg(feature = "ssr")]
use leptos::{config::LeptosOptions, hydration::HydrationScripts};
#[cfg(feature = "ssr")]
use leptos_meta::MetaTags;
use leptos_router::components::*;
use leptos_router::path;

#[cfg(feature = "ssr")]
pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="zh-CN">
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
                <title>"PT-Reseeder"</title>
                <link rel="stylesheet" href="/pkg/pt-reseeder.css" />
                <HydrationScripts options />
                <MetaTags />
            </head>
            <body>
                <App />
            </body>
        </html>
    }
}

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
