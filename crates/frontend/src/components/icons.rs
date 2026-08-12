//! 侧边导航用的内联线条图标。
//!
//! 统一规格：16×16 viewBox、`fill="none"` + `stroke="currentColor"`、
//! 1.5px 描边。用 `currentColor` 继承 `.app-nav-link` 的主题色，
//! 因此 hover / active 状态无需额外样式。图标是纯装饰，旁边的
//! 文字标签已提供语义，故一律 `aria-hidden="true"`。

use leptos::prelude::*;

/// 侧边导航图标的枚举标识。
///
/// `NavEntry` 存这个而不是组件函数指针：`#[component]` 生成的类型
/// 各不相同，无法放进同一个 `&'static [..]` 常量表。
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NavIcon {
    Dashboard,
    Sites,
    Downloaders,
    Tasks,
    Reseed,
    Folders,
    Repost,
    Logs,
    Settings,
    Logout,
}

/// 按标识渲染对应的线条图标。
#[component]
pub fn Icon(icon: NavIcon) -> impl IntoView {
    match icon {
        NavIcon::Dashboard => view! { <IconDashboard /> }.into_any(),
        NavIcon::Sites => view! { <IconSites /> }.into_any(),
        NavIcon::Downloaders => view! { <IconDownloaders /> }.into_any(),
        NavIcon::Tasks => view! { <IconTasks /> }.into_any(),
        NavIcon::Reseed => view! { <IconReseed /> }.into_any(),
        NavIcon::Folders => view! { <IconFolders /> }.into_any(),
        NavIcon::Repost => view! { <IconRepost /> }.into_any(),
        NavIcon::Logs => view! { <IconLogs /> }.into_any(),
        NavIcon::Settings => view! { <IconSettings /> }.into_any(),
        NavIcon::Logout => view! { <IconLogout /> }.into_any(),
    }
}

/// 仪表盘：四宫格面板。
#[component]
fn IconDashboard() -> impl IntoView {
    view! {
        <svg
            width="16"
            height="16"
            viewBox="0 0 16 16"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
        >
            <rect x="2" y="2" width="5" height="5" rx="1" />
            <rect x="9" y="2" width="5" height="5" rx="1" />
            <rect x="2" y="9" width="5" height="5" rx="1" />
            <rect x="9" y="9" width="5" height="5" rx="1" />
        </svg>
    }
}

/// 站点：地球（PT 站点即远端服务器）。
#[component]
fn IconSites() -> impl IntoView {
    view! {
        <svg
            width="16"
            height="16"
            viewBox="0 0 16 16"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
        >
            <circle cx="8" cy="8" r="6" />
            <path d="M2 8h12" />
            <path d="M8 2c1.6 1.8 2.4 3.8 2.4 6S9.6 12.2 8 14C6.4 12.2 5.6 10.2 5.6 8S6.4 3.8 8 2z" />
        </svg>
    }
}

/// 下载器：箭头指向托盘。
#[component]
fn IconDownloaders() -> impl IntoView {
    view! {
        <svg
            width="16"
            height="16"
            viewBox="0 0 16 16"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
        >
            <path d="M8 2v7" />
            <path d="M5 6.5 8 9.5l3-3" />
            <path d="M2.5 11v1.5A1.5 1.5 0 0 0 4 14h8a1.5 1.5 0 0 0 1.5-1.5V11" />
        </svg>
    }
}

/// 任务：带指针的时钟（任务按计划轮转）。
#[component]
fn IconTasks() -> impl IntoView {
    view! {
        <svg
            width="16"
            height="16"
            viewBox="0 0 16 16"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
        >
            <circle cx="8" cy="8" r="6" />
            <path d="M8 4.5V8l2.5 1.5" />
        </svg>
    }
}

/// 辅种结果：种子发芽（识别到的辅种明细）。
#[component]
fn IconReseed() -> impl IntoView {
    view! {
        <svg
            width="16"
            height="16"
            viewBox="0 0 16 16"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
        >
            <path d="M8 13.5V7" />
            <path d="M8 7C8 4.5 6 2.5 3.5 2.5 3.5 5 5.5 7 8 7z" />
            <path d="M8 7c0-2.5 2-4.5 4.5-4.5C12.5 5 10.5 7 8 7z" />
            <path d="M5.5 13.5h5" />
        </svg>
    }
}

/// 文件夹。
#[component]
fn IconFolders() -> impl IntoView {
    view! {
        <svg
            width="16"
            height="16"
            viewBox="0 0 16 16"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
        >
            <path d="M2 4.5A1.5 1.5 0 0 1 3.5 3h2.3l1.2 1.6h5.5A1.5 1.5 0 0 1 14 6.1v5.4a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 11.5z" />
        </svg>
    }
}

/// 转种：双向循环箭头。
#[component]
fn IconRepost() -> impl IntoView {
    view! {
        <svg
            width="16"
            height="16"
            viewBox="0 0 16 16"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
        >
            <path d="M2.5 6.5A5 5 0 0 1 12 5" />
            <path d="M13.5 9.5A5 5 0 0 1 4 11" />
            <path d="M12 2.5V5H9.5" />
            <path d="M4 13.5V11h2.5" />
        </svg>
    }
}

/// 日志：带文本行的清单。
#[component]
fn IconLogs() -> impl IntoView {
    view! {
        <svg
            width="16"
            height="16"
            viewBox="0 0 16 16"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
        >
            <rect x="3" y="2" width="10" height="12" rx="1.5" />
            <path d="M5.5 5.5h5" />
            <path d="M5.5 8h5" />
            <path d="M5.5 10.5h3" />
        </svg>
    }
}

/// 设置：齿轮（简化为圆 + 四向轴，避免复杂齿形路径）。
#[component]
fn IconSettings() -> impl IntoView {
    view! {
        <svg
            width="16"
            height="16"
            viewBox="0 0 16 16"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
        >
            <circle cx="8" cy="8" r="2.4" />
            <path d="M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2M3.4 3.4l1.4 1.4M11.2 11.2l1.4 1.4M12.6 3.4l-1.4 1.4M4.8 11.2l-1.4 1.4" />
        </svg>
    }
}

/// 退出登录：箭头移出门框。
#[component]
fn IconLogout() -> impl IntoView {
    view! {
        <svg
            width="16"
            height="16"
            viewBox="0 0 16 16"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
        >
            <path d="M6.5 2.5H4A1.5 1.5 0 0 0 2.5 4v8A1.5 1.5 0 0 0 4 13.5h2.5" />
            <path d="M10 5.5 12.5 8 10 10.5" />
            <path d="M12.5 8h-6" />
        </svg>
    }
}
