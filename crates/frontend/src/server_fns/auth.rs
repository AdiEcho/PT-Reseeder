// Authentication: registration, login/logout and current-user lookup.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub username: String,
}

#[server]
pub async fn has_user() -> Result<bool, ServerFnError> {
    let pool = server_pool()?;
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(&pool)
        .await?;
    Ok(count.0 > 0)
}

#[server]
pub async fn login(username: String, password: String) -> Result<(), ServerFnError> {
    auth_login(username, password).await
}

#[server]
pub async fn register(username: String, password: String) -> Result<(), ServerFnError> {
    auth_register(username, password).await
}

#[server]
pub async fn logout() -> Result<(), ServerFnError> {
    use axum::http::HeaderMap;
    use pt_reseeder_core::db::repo::Repository;
    use pt_reseeder_core::session::{resolve_session, SessionOutcome};

    let context = server_context()?;
    let headers: HeaderMap = leptos_axum::extract().await?;
    let repo = Repository::new(context.pool.clone());
    // Only the session id is needed here; every other outcome is a no-op, and the
    // removal cookie below is sent regardless of what was found.
    //
    // Note this is not reachable without a live session: `logout` is absent from
    // `PUBLIC_SERVER_FNS`, so `guard_server_fns` answers 401 before this body runs.
    // A user whose session already expired server-side therefore keeps the stale
    // cookie until the next successful login overwrites it — pre-existing
    // behaviour, not something this refactor changed. `nav.rs` ignores the result
    // and navigates to /login either way, so the UI still recovers.
    if let SessionOutcome::Valid(session) = resolve_session(&repo, cookie_headers(&headers)).await {
        let _ = repo.delete_session(session.id).await;
    }
    append_set_cookie(&build_removal_cookie(context.cookie_secure))?;
    Ok(())
}

#[server]
pub async fn get_current_user() -> Result<Option<UserInfo>, ServerFnError> {
    use axum::http::HeaderMap;
    use pt_reseeder_core::db::models::User;
    use pt_reseeder_core::db::repo::Repository;
    use pt_reseeder_core::session::{resolve_session, SessionOutcome};

    let context = server_context()?;
    if context.vault.read().await.is_none() {
        return Ok(None);
    }

    let headers: HeaderMap = leptos_axum::extract().await?;
    let pool = server_pool()?;
    let repo = Repository::new(pool.clone());
    let session = match resolve_session(&repo, cookie_headers(&headers)).await {
        SessionOutcome::Valid(session) => session,
        // Not an error: this endpoint is public and answers "who am I" with None
        // when there is no live session. Returning Err would pop an error toast
        // on every first page load.
        SessionOutcome::Unauthenticated => return Ok(None),
        SessionOutcome::Failed(e) => return Err(e.into()),
    };

    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
        .bind(session.user_id)
        .fetch_optional(&pool)
        .await?;
    Ok(user.map(|user| UserInfo {
        username: user.username,
    }))
}
