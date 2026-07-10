use tauri::Manager;

/// Inject JavaScript into the WebView that parses adapted torrent info JSON
/// and auto-fills common NexusPHP upload form fields.
///
/// The form is NOT auto-submitted -- the user reviews and clicks submit manually.
#[tauri::command]
pub async fn inject_repost_autofill(
    webview_window: tauri::WebviewWindow,
    entry_id: i64,
    adapted_info_json: String,
) -> Result<(), String> {
    // We embed the JSON payload directly into the injected script as a string
    // literal.  The JS side parses it, fills form fields, and dispatches a
    // CustomEvent back to the page so the UI can show success/failure feedback.
    let escaped_json = adapted_info_json
        .replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('\n', "\\n")
        .replace('\r', "\\r");

    let script = format!(
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

        // Torrent name
        fillInput(
            ['input[name="name"]', '#name'],
            info.name || info.torrent_name,
            'name'
        );

        // Description (BBCode)
        fillInput(
            ['textarea[name="descr"]', '#descr'],
            info.description || info.descr,
            'descr'
        );

        // Subtitle / small description
        fillInput(
            ['input[name="small_descr"]', '#small_descr'],
            info.small_descr || info.subtitle,
            'small_descr'
        );

        // Category select
        fillSelect(
            'select[name="type"]',
            info.category || info.type_id,
            'type'
        );

        // Source select
        fillSelect(
            'select[name="source_sel"]',
            info.source || info.source_sel,
            'source_sel'
        );

        // Codec select
        fillSelect(
            'select[name="codec_sel"]',
            info.codec || info.codec_sel,
            'codec_sel'
        );

        // Resolution / standard select
        fillSelect(
            'select[name="standard_sel"]',
            info.resolution || info.standard_sel || info.standard,
            'standard_sel'
        );

        // IMDb URL
        fillInput(
            ['input[name="url"]', '#url'],
            info.imdb_url || info.url || info.imdb,
            'url'
        );

        // MediaInfo
        fillInput(
            ['textarea[name="mediainfo"]', '#mediainfo'],
            info.mediainfo || info.media_info,
            'mediainfo'
        );

        window.dispatchEvent(new CustomEvent('pt-reseeder:repost-autofill', {{
            detail: {{
                entryId: {entry_id},
                available: true,
                success: true,
                filled: filled,
                skipped: skipped,
                message: 'Autofill completed: ' + filled.length + ' field(s) filled, ' + skipped.length + ' skipped.'
            }}
        }}));
    }} catch (err) {{
        window.dispatchEvent(new CustomEvent('pt-reseeder:repost-autofill', {{
            detail: {{
                entryId: {entry_id},
                available: true,
                success: false,
                error: err.message || String(err),
                message: 'Autofill failed: ' + (err.message || String(err))
            }}
        }}));
    }}
}})();
"#
    );

    webview_window.eval(&script).map_err(|e| e.to_string())
}

/// Open a PT site's upload page in a dedicated WebView window.
///
/// The window is labelled `repost-{entry_id}` so each repost queue entry gets
/// its own tab-like window, and calling this twice with the same entry_id will
/// focus the existing window rather than spawning a duplicate.
#[tauri::command]
pub async fn open_upload_page(
    app: tauri::AppHandle,
    site_url: String,
    entry_id: i64,
) -> Result<(), String> {
    let label = format!("repost-{entry_id}");

    // If a window with this label already exists, just bring it to the front.
    if let Some(existing) = app.get_webview_window(&label) {
        existing.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }

    let upload_url = format!("{}/upload.php", site_url.trim_end_matches('/'));
    let url = url::Url::parse(&upload_url).map_err(|e| format!("invalid site URL: {e}"))?;

    tauri::WebviewWindowBuilder::new(&app, &label, tauri::WebviewUrl::External(url))
        .title(format!("Upload - repost #{entry_id}"))
        .inner_size(1100.0, 800.0)
        .build()
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Check whether WebView autofill is available.
///
/// On the desktop (Tauri) build this always returns `true`.  The frontend can
/// call this to decide whether to show the "auto-fill" button or fall back to a
/// manual workflow (e.g. on a pure web deployment).
#[tauri::command]
pub async fn check_autofill_available() -> bool {
    true
}
