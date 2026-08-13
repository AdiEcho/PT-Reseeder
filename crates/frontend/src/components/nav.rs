use crate::components::icons::{Icon, NavIcon};
use crate::components::theme::ThemeToggle;
use crate::server_fns::{get_current_user, logout, UserInfo};
use leptos::ev;
use leptos::prelude::*;
use leptos_router::{
    components::{Outlet, Redirect, A},
    hooks::use_navigate,
};

/// A single entry in the sidebar navigation.
struct NavEntry {
    label: &'static str,
    href: &'static str,
    /// Identifies the inline SVG rendered by `<Icon />`. Stored as an enum
    /// rather than a component so all entries share one type in `NAV`.
    icon: NavIcon,
    /// When `true`, the link is active only on an exact path match.
    /// Otherwise it is active when the current path starts with `href`
    /// (so `/sites` stays highlighted on `/sites/:id`).
    exact: bool,
}

const NAV: &[NavEntry] = &[
    NavEntry {
        label: "仪表盘",
        href: "/dashboard",
        icon: NavIcon::Dashboard,
        exact: true,
    },
    NavEntry {
        label: "站点",
        href: "/sites",
        icon: NavIcon::Sites,
        exact: false,
    },
    NavEntry {
        label: "下载器",
        href: "/downloaders",
        icon: NavIcon::Downloaders,
        exact: true,
    },
    NavEntry {
        label: "任务",
        href: "/tasks",
        icon: NavIcon::Tasks,
        exact: true,
    },
    NavEntry {
        label: "辅种",
        href: "/reseed",
        icon: NavIcon::Reseed,
        exact: true,
    },
    NavEntry {
        label: "文件夹",
        href: "/folders",
        icon: NavIcon::Folders,
        exact: true,
    },
    NavEntry {
        label: "转种",
        href: "/repost",
        icon: NavIcon::Repost,
        exact: true,
    },
    NavEntry {
        label: "日志",
        href: "/logs",
        icon: NavIcon::Logs,
        exact: true,
    },
    NavEntry {
        label: "设置",
        href: "/settings",
        icon: NavIcon::Settings,
        exact: true,
    },
];

/// Authenticated application shell: persistent sidebar + topbar, with the
/// matched page rendered through `<Outlet />`.
///
/// Guards the whole subtree: while the session is being resolved we show a
/// loading state; if there is no logged-in user we redirect to `/login`.
#[component]
pub fn AppLayout() -> impl IntoView {
    let current_user = Resource::new(|| (), |_| get_current_user());

    view! {
        <Suspense fallback=|| view! { <div class="app-loading">"加载中…"</div> }>
            {move || {
                match current_user.get() {
                    None => view! { <div class="app-loading">"加载中…"</div> }.into_any(),
                    Some(Err(_)) => view! { <Redirect path="/login" /> }.into_any(),
                    Some(Ok(None)) => view! { <Redirect path="/login" /> }.into_any(),
                    Some(Ok(Some(user))) => view! { <Shell user=user /> }.into_any(),
                }
            }}
        </Suspense>
    }
}

#[component]
fn Shell(user: UserInfo) -> impl IntoView {
    let username = user.username.clone();
    let initial = user
        .username
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string());
    let navigate = use_navigate();

    let on_logout = move |_: ev::MouseEvent| {
        let navigate = navigate.clone();
        leptos::task::spawn_local(async move {
            let _ = logout().await;
            navigate("/login", Default::default());
        });
    };

    // Mobile sidebar visibility. Signal-driven rather than toggling classes on
    // the DOM directly, so the open state stays inside the reactive graph.
    let (sidebar_open, set_sidebar_open) = signal(false);

    view! {
        <div class="app-shell">
            <div
                class="sidebar-backdrop"
                class:visible=move || sidebar_open.get()
                on:click=move |_| set_sidebar_open.set(false)
            ></div>
            <Sidebar
                on_logout=on_logout
                is_open=sidebar_open
                on_navigate=move || set_sidebar_open.set(false)
            />
            <Topbar
                username=username.clone()
                initial=initial
                on_toggle_sidebar=move || set_sidebar_open.update(|open| *open = !*open)
            />
            <main class="app-content">
                <Outlet />
            </main>
        </div>
    }
}

#[component]
fn Sidebar<F, N>(on_logout: F, is_open: ReadSignal<bool>, on_navigate: N) -> impl IntoView
where
    F: Fn(ev::MouseEvent) + 'static + Clone,
    N: Fn() + Copy + 'static,
{
    view! {
        <aside class="app-sidebar" class:open=move || is_open.get()>
            <div class="app-sidebar__brand">
                <div class="app-sidebar__logo">"P"</div>
                <span class="app-sidebar__title">"PT-Reseeder"</span>
            </div>
            <nav class="app-sidebar__nav">
                <div class="app-sidebar__section">"管理"</div>
                {NAV
                    .iter()
                    .map(|entry| {
                        view! {
                            <A
                                href=entry.href
                                exact=entry.exact
                                {..}
                                attr:class="app-nav-link"
                                on:click=move |_| on_navigate()
                            >
                                <span class="app-nav-link__icon">
                                    <Icon icon=entry.icon />
                                </span>
                                <span>{entry.label}</span>
                            </A>
                        }
                    })
                    .collect::<Vec<_>>()}
            </nav>
            <div class="app-sidebar__footer">
                <button class="app-nav-link" on:click=move |ev| on_logout.clone()(ev)>
                    <span class="app-nav-link__icon">
                        <Icon icon=NavIcon::Logout />
                    </span>
                    <span>"退出登录"</span>
                </button>
            </div>
        </aside>
    }
}

#[component]
fn Topbar<T>(username: String, initial: String, on_toggle_sidebar: T) -> impl IntoView
where
    T: Fn() + 'static,
{
    view! {
        <header class="app-topbar">
            <div class="app-topbar__left">
                <button
                    class="mobile-menu-toggle"
                    type="button"
                    aria-label="打开菜单"
                    on:click=move |_| on_toggle_sidebar()
                >
                    "☰"
                </button>
                <div class="app-topbar__title">"PT-Reseeder"</div>
            </div>
            <div class="app-topbar__user">
                <ThemeToggle />
                <span>{username}</span>
                <div class="app-topbar__avatar">{initial}</div>
            </div>
        </header>
    }
}
