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
    use axum_extra::extract::cookie::CookieJar;
    use pt_reseeder_core::db::repo::Repository;

    let context = server_context()?;
    let jar: CookieJar = leptos_axum::extract()
        .await?;
    if let Some(cookie) = jar.get(SESSION_COOKIE_NAME) {
        if let Some(token_hash) = hash_token(cookie.value()) {
            let repo = Repository::new(context.pool.clone());
            if let Some(session) = repo
                .find_session_by_hash(&token_hash)
                .await?
            {
                let _ = repo.delete_session(session.id).await;
            }
        }
    }
    append_set_cookie(&build_removal_cookie(context.cookie_secure))?;
    Ok(())
}

#[server]
pub async fn get_current_user() -> Result<Option<UserInfo>, ServerFnError> {
    use axum_extra::extract::cookie::CookieJar;
    use pt_reseeder_core::db::models::User;
    use pt_reseeder_core::db::repo::Repository;

    let context = server_context()?;
    if context.vault.read().await.is_none() {
        return Ok(None);
    }

    let jar: CookieJar = leptos_axum::extract()
        .await?;
    let Some(cookie) = jar.get(SESSION_COOKIE_NAME) else {
        return Ok(None);
    };
    let Some(token_hash) = hash_token(cookie.value()) else {
        return Ok(None);
    };

    let pool = server_pool()?;
    let repo = Repository::new(pool.clone());
    let Some(session) = repo
        .find_session_by_hash(&token_hash)
        .await?
    else {
        return Ok(None);
    };
    if pt_reseeder_core::session::is_session_expired(&session.expires_at) {
        let _ = repo.delete_session(session.id).await;
        return Ok(None);
    }

    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
        .bind(session.user_id)
        .fetch_optional(&pool)
        .await?;
    Ok(user.map(|user| UserInfo {
        username: user.username,
    }))
}
