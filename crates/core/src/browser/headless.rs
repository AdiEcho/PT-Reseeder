//! Headless browser autofill implementation using chromiumoxide.
//!
//! This module is only compiled when the `headless-browser` feature is enabled.

use std::sync::Arc;

use async_trait::async_trait;
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::page::Page;
use futures::StreamExt;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::error::{CoreError, RepostError};
use crate::site::models::AdaptedTorrentInfo;

use super::{AutofillResult, RepostAutoFiller};

/// Headless Chrome/Chromium autofiller for Docker and server-only deployments.
///
/// Mirrors the desktop WebView autofill logic from `crates/desktop/src/webview.rs`
/// but runs headlessly via chromiumoxide.
pub struct HeadlessBrowser {
    browser: Arc<Browser>,
    /// Active pages keyed by repost entry ID.
    pages: RwLock<std::collections::HashMap<i64, Page>>,
}

impl HeadlessBrowser {
    /// Launch a headless Chrome/Chromium instance using environment settings.
    ///
    /// `PT_RESEEDER_CHROME_NO_SANDBOX` defaults to `false` and only disables
    /// the Chromium sandbox when set to `true` (case-insensitive).
    pub async fn from_env(chrome_path: Option<String>) -> Result<Self, CoreError> {
        let disable_sandbox = std::env::var("PT_RESEEDER_CHROME_NO_SANDBOX")
            .ok()
            .is_some_and(|value| value.eq_ignore_ascii_case("true"));

        Self::new(chrome_path, disable_sandbox).await
    }

    /// Launch a headless Chrome/Chromium instance.
    ///
    /// `chrome_path` optionally overrides the browser binary location.
    /// `disable_sandbox` must be explicitly enabled for environments where the
    /// Chromium sandbox cannot run.
    pub async fn new(
        chrome_path: Option<String>,
        disable_sandbox: bool,
    ) -> Result<Self, CoreError> {
        let mut builder = BrowserConfig::builder().window_size(1280, 900);

        if disable_sandbox {
            builder = builder.no_sandbox();
        }

        if let Some(path) = chrome_path {
            builder = builder.chrome_executable(path);
        }

        let config = builder.build().map_err(|e| {
            CoreError::Repost(RepostError::SubmissionFailed(format!(
                "failed to build browser config: {}",
                e
            )))
        })?;

        let (browser, mut handler) = Browser::launch(config).await.map_err(|e| {
            CoreError::Repost(RepostError::SubmissionFailed(format!(
                "failed to launch headless browser: {}",
                e
            )))
        })?;

        // Spawn the browser event handler in the background.
        tokio::spawn(async move {
            while let Some(event) = handler.next().await {
                if event.is_err() {
                    break;
                }
            }
        });

        info!("headless browser launched");

        Ok(Self {
            browser: Arc::new(browser),
            pages: RwLock::new(std::collections::HashMap::new()),
        })
    }

    /// Close the browser and drop all pages.
    pub async fn close(&self) {
        self.pages.write().await.clear();
        info!("headless browser closed");
    }
}

/// Build the autofill JavaScript.
///
/// This is intentionally kept in sync with the desktop WebView JS in
/// `crates/desktop/src/webview.rs::inject_repost_autofill`.
fn build_autofill_js(entry_id: i64, adapted_info_json: &str) -> String {
    let escaped_json = adapted_info_json
        .replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('\n', "\\n")
        .replace('\r', "\\r");

    format!(
        r#"
(function() {{
    try {{
        var info = JSON.parse('{escaped_json}');
        var filled = [];
        var skipped = [];

        function fillInput(selectors, value, label) {{
            if (value == null || value === '') {{
                skipped.push(label);
                return;
            }}
            for (var i = 0; i < selectors.length; i++) {{
                var el = document.querySelector(selectors[i]);
                if (el) {{
                    el.value = value;
                    el.dispatchEvent(new Event('input', {{ bubbles: true }}));
                    el.dispatchEvent(new Event('change', {{ bubbles: true }}));
                    filled.push(label);
                    return;
                }}
            }}
            skipped.push(label);
        }}

        function fillSelect(selector, value, label) {{
            if (value == null || value === '') {{
                skipped.push(label);
                return;
            }}
            var el = document.querySelector(selector);
            if (!el) {{
                skipped.push(label);
                return;
            }}
            var opts = el.options;
            var matched = false;
            for (var i = 0; i < opts.length; i++) {{
                if (opts[i].value === String(value) || opts[i].text === String(value)) {{
                    el.selectedIndex = i;
                    el.dispatchEvent(new Event('change', {{ bubbles: true }}));
                    filled.push(label);
                    matched = true;
                    break;
                }}
            }}
            if (!matched) {{
                skipped.push(label + ' (no matching option)');
            }}
        }}

        // --- Fill form fields ---

        fillInput(
            ['input[name="name"]', '#name'],
            info.name || info.torrent_name,
            'name'
        );

        fillInput(
            ['textarea[name="descr"]', '#descr'],
            info.descr,
            'descr'
        );

        fillInput(
            ['input[name="small_descr"]', '#small_descr'],
            info.small_descr,
            'small_descr'
        );

        fillSelect(
            'select[name="type"]',
            info.category_id,
            'type'
        );

        fillSelect(
            'select[name="source_sel"]',
            info.source_id,
            'source_sel'
        );

        fillSelect(
            'select[name="codec_sel"]',
            info.codec_id,
            'codec_sel'
        );

        fillSelect(
            'select[name="standard_sel"]',
            info.resolution_id,
            'standard_sel'
        );

        fillInput(
            ['input[name="url"]', '#url'],
            info.imdb_url,
            'url'
        );

        fillInput(
            ['textarea[name="mediainfo"]', '#mediainfo'],
            info.mediainfo,
            'mediainfo'
        );

        return JSON.stringify({{
            entryId: {entry_id},
            success: true,
            filled: filled,
            skipped: skipped,
            message: 'Autofill completed: ' + filled.length + ' field(s) filled, ' + skipped.length + ' skipped.'
        }});
    }} catch (err) {{
        return JSON.stringify({{
            entryId: {entry_id},
            success: false,
            filled: [],
            skipped: [],
            message: 'Autofill failed: ' + (err.message || String(err))
        }});
    }}
}})();
"#
    )
}

#[async_trait]
impl RepostAutoFiller for HeadlessBrowser {
    async fn open_upload_page(&self, site_url: &str, entry_id: i64) -> Result<(), CoreError> {
        let upload_url = format!("{}/upload.php", site_url.trim_end_matches('/'));

        let page = self.browser.new_page(&upload_url).await.map_err(|e| {
            CoreError::Repost(RepostError::SubmissionFailed(format!(
                "failed to open upload page: {}",
                e
            )))
        })?;

        // Wait for the page to be reasonably loaded.
        page.wait_for_navigation().await.map_err(|e| {
            CoreError::Repost(RepostError::SubmissionFailed(format!(
                "page navigation failed: {}",
                e
            )))
        })?;

        info!(entry_id, url = %upload_url, "opened upload page in headless browser");

        self.pages.write().await.insert(entry_id, page);
        Ok(())
    }

    async fn inject_autofill(
        &self,
        entry_id: i64,
        adapted_info: &AdaptedTorrentInfo,
    ) -> Result<AutofillResult, CoreError> {
        let pages = self.pages.read().await;
        let page = pages.get(&entry_id).ok_or_else(|| {
            CoreError::Repost(RepostError::SubmissionFailed(format!(
                "no open page for entry {}; call open_upload_page first",
                entry_id
            )))
        })?;

        let info_json = serde_json::to_string(adapted_info).map_err(|e| {
            CoreError::Repost(RepostError::SubmissionFailed(format!(
                "failed to serialize adapted info: {}",
                e
            )))
        })?;

        let script = build_autofill_js(entry_id, &info_json);

        let result_value = page.evaluate(script).await.map_err(|e| {
            CoreError::Repost(RepostError::SubmissionFailed(format!(
                "JS evaluation failed: {}",
                e
            )))
        })?;

        let result_str = result_value.into_value::<String>().map_err(|e| {
            CoreError::Repost(RepostError::SubmissionFailed(format!(
                "failed to extract JS result: {}",
                e
            )))
        })?;

        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct JsResult {
            entry_id: i64,
            success: bool,
            filled: Vec<String>,
            skipped: Vec<String>,
            message: String,
        }

        let js_result: JsResult = serde_json::from_str(&result_str).map_err(|e| {
            CoreError::Repost(RepostError::SubmissionFailed(format!(
                "failed to parse autofill result: {}",
                e
            )))
        })?;

        if !js_result.success {
            warn!(entry_id, msg = %js_result.message, "headless autofill reported failure");
        } else {
            info!(
                entry_id,
                filled = js_result.filled.len(),
                skipped = js_result.skipped.len(),
                "headless autofill completed"
            );
        }

        Ok(AutofillResult {
            entry_id: js_result.entry_id,
            success: js_result.success,
            filled: js_result.filled,
            skipped: js_result.skipped,
            message: js_result.message,
        })
    }

    fn is_available(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use boa_engine::{Context, Source};
    use serde_json::Value;

    use super::*;

    fn execute_autofill_js(entry_id: i64, info_json: &str) -> Value {
        let script = format!(
            "var Event = function(type, options) {{ this.type = type; this.options = options; }};\n\
             var document = {{ querySelector: function() {{ return null; }} }};\n{}",
            build_autofill_js(entry_id, info_json)
        );
        let mut context = Context::default();
        let result = context
            .eval(Source::from_bytes(&script))
            .expect("autofill JavaScript should execute");
        let result = result
            .as_string()
            .expect("autofill IIFE should return a JSON string")
            .to_std_string_escaped();

        serde_json::from_str(&result).expect("autofill result should deserialize")
    }

    #[test]
    fn autofill_js_returns_deserializable_success_result_with_special_chars() {
        let info_json = serde_json::json!({
            "name": "it's a \"test\"\n第二行\\path",
            "descr": "描述 ' \" \n \\"
        })
        .to_string();

        let result = execute_autofill_js(42, &info_json);

        assert_eq!(result["entryId"], 42);
        assert_eq!(result["success"], true);
        assert_eq!(result["filled"], serde_json::json!([]));
        assert_eq!(result["skipped"].as_array().map(Vec::len), Some(9));
        assert!(result["message"]
            .as_str()
            .is_some_and(|message| message.contains("Autofill completed")));
    }

    #[test]
    fn autofill_js_returns_deserializable_error_result() {
        let result = execute_autofill_js(7, "{invalid json");

        assert_eq!(result["entryId"], 7);
        assert_eq!(result["success"], false);
        assert_eq!(result["filled"], serde_json::json!([]));
        assert_eq!(result["skipped"], serde_json::json!([]));
        assert!(result["message"]
            .as_str()
            .is_some_and(|message| message.starts_with("Autofill failed:")));
    }
}
