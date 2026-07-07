#[tauri::command]
pub async fn inject_repost_autofill(
    webview_window: tauri::WebviewWindow,
    entry_id: i64,
) -> Result<(), String> {
    let script = format!(
        r#"window.dispatchEvent(new CustomEvent("pt-reseeder:repost-autofill", {{ detail: {{ entryId: {entry_id}, available: false, message: "Desktop WebView autofill hook ready; no autofill payload supplied." }} }}));"#
    );

    webview_window.eval(&script).map_err(|e| e.to_string())
}
