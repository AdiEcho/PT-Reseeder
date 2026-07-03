use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub username: String,
}

#[server]
pub async fn login(username: String, password: String) -> Result<(), ServerFnError> {
    // This runs on server -- extract AppState from context
    // Call auth logic
    // For now, stub:
    let _ = (&username, &password);
    Ok(())
}

#[server]
pub async fn register(username: String, password: String) -> Result<(), ServerFnError> {
    let _ = (&username, &password);
    Ok(())
}

#[server]
pub async fn logout() -> Result<(), ServerFnError> {
    Ok(())
}

#[server]
pub async fn get_current_user() -> Result<Option<UserInfo>, ServerFnError> {
    Ok(None)
}
