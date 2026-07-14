use leptos::prelude::*;

use crate::server_fns::{DashboardOverview, LogEntry, SiteReseedStats, UserInfoAggregate};

#[cfg(target_arch = "wasm32")]
thread_local! {
    static DASHBOARD_WS_HANDLES: std::cell::RefCell<Vec<Option<web_sys::WebSocket>>> = const { std::cell::RefCell::new(Vec::new()) };
    static LOGS_WS_HANDLES: std::cell::RefCell<Vec<Option<web_sys::WebSocket>>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// A single real-time update pushed over the WebSocket.
#[derive(Debug, Clone)]
pub struct DashboardWsUpdate {
    pub overview: Option<DashboardOverview>,
    pub site_stats: Option<Vec<SiteReseedStats>>,
    pub user_info: Option<UserInfoAggregate>,
}

/// Subscribe to `/ws/dashboard` for real-time dashboard updates.
///
/// On wasm32 (hydrated client), opens a WebSocket after hydration and
/// deserialises every incoming `dashboard_update` event into the returned
/// signal. On non-wasm (SSR) the signal is always `None`.
pub fn use_dashboard_ws() -> ReadSignal<Option<DashboardWsUpdate>> {
    let (ws_data, set_ws_data) = signal(None::<DashboardWsUpdate>);
    // Suppress unused-variable warning on non-wasm (SSR) targets where the
    // cfg(wasm32) block below is compiled out.
    #[cfg(not(target_arch = "wasm32"))]
    let _ = &set_ws_data;

    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::prelude::*;
        use wasm_bindgen::JsCast;
        use web_sys::{MessageEvent, WebSocket};

        Effect::new(move |_| {
            let window = match web_sys::window() {
                Some(w) => w,
                None => return,
            };
            let location = window.location();
            let protocol = location.protocol().unwrap_or_default();
            let host = location.host().unwrap_or_default();
            let ws_protocol = if protocol == "https:" { "wss:" } else { "ws:" };
            let url = format!("{ws_protocol}//{host}/ws/dashboard");

            let ws = match WebSocket::new(&url) {
                Ok(ws) => ws,
                Err(_) => return,
            };

            // --- onmessage ---------------------------------------------------
            let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
                if let Ok(js_str) = e.data().dyn_into::<js_sys::JsString>() {
                    let text: String = js_str.into();
                    // The server sends: {"type":"dashboard_update","overview":{...},...}
                    // Ignore explicit non-dashboard progress events.
                    #[derive(serde::Deserialize)]
                    struct RawWsEvent {
                        r#type: String,
                        overview: Option<DashboardOverview>,
                        site_stats: Option<Vec<SiteReseedStats>>,
                        user_info: Option<UserInfoAggregate>,
                    }

                    if let Ok(evt) = serde_json::from_str::<RawWsEvent>(&text) {
                        if evt.r#type == "dashboard_update" {
                            set_ws_data.set(Some(DashboardWsUpdate {
                                overview: evt.overview,
                                site_stats: evt.site_stats,
                                user_info: evt.user_info,
                            }));
                        }
                    }
                }
            });
            ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
            on_message.forget(); // prevent GC -- closure lives as long as the WS

            let handle_id = DASHBOARD_WS_HANDLES.with(|handles| {
                let mut handles = handles.borrow_mut();
                handles.push(Some(ws));
                handles.len() - 1
            });

            on_cleanup(move || {
                DASHBOARD_WS_HANDLES.with(|handles| {
                    if let Some(ws) = handles
                        .borrow_mut()
                        .get_mut(handle_id)
                        .and_then(Option::take)
                    {
                        let _ = ws.close();
                    }
                });
            });
        });
    }

    ws_data
}

/// Subscribe to `/ws/logs` for real-time log streaming.
///
/// On wasm32 (hydrated client), opens a WebSocket and deserialises every
/// incoming `log_line` event into a `LogEntry`. On non-wasm (SSR) the
/// signal is always `None`.
pub fn use_logs_ws() -> ReadSignal<Option<LogEntry>> {
    let (ws_data, set_ws_data) = signal(None::<LogEntry>);
    #[cfg(not(target_arch = "wasm32"))]
    let _ = &set_ws_data;

    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::prelude::*;
        use wasm_bindgen::JsCast;
        use web_sys::{MessageEvent, WebSocket};

        Effect::new(move |_| {
            let window = match web_sys::window() {
                Some(w) => w,
                None => return,
            };
            let location = window.location();
            let protocol = location.protocol().unwrap_or_default();
            let host = location.host().unwrap_or_default();
            let ws_protocol = if protocol == "https:" { "wss:" } else { "ws:" };
            let url = format!("{ws_protocol}//{host}/ws/logs");

            let ws = match WebSocket::new(&url) {
                Ok(ws) => ws,
                Err(_) => return,
            };

            let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
                if let Ok(js_str) = e.data().dyn_into::<js_sys::JsString>() {
                    let text: String = js_str.into();
                    #[derive(serde::Deserialize)]
                    struct RawLogEvent {
                        r#type: String,
                        line: Option<String>,
                    }

                    if let Ok(evt) = serde_json::from_str::<RawLogEvent>(&text) {
                        if evt.r#type == "log_line" {
                            if let Some(line) = evt.line {
                                let parts: Vec<&str> = line.splitn(4, ' ').collect();
                                let entry = if parts.len() >= 4 {
                                    LogEntry {
                                        timestamp: parts[0].to_string(),
                                        level: parts[1].to_string(),
                                        target: parts[2].to_string(),
                                        message: parts[3].to_string(),
                                    }
                                } else {
                                    LogEntry {
                                        timestamp: String::new(),
                                        level: String::new(),
                                        target: String::new(),
                                        message: line,
                                    }
                                };
                                set_ws_data.set(Some(entry));
                            }
                        }
                    }
                }
            });
            ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
            on_message.forget();

            let handle_id = LOGS_WS_HANDLES.with(|handles| {
                let mut handles = handles.borrow_mut();
                handles.push(Some(ws));
                handles.len() - 1
            });

            on_cleanup(move || {
                LOGS_WS_HANDLES.with(|handles| {
                    if let Some(ws) = handles
                        .borrow_mut()
                        .get_mut(handle_id)
                        .and_then(Option::take)
                    {
                        let _ = ws.close();
                    }
                });
            });
        });
    }

    ws_data
}
