// Application config: DTO and server functions backed by the `app_config` table.

pub const FETCH_SEEDING_SIZE_CONFIG_KEY: &str = "fetch_seeding_size";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigEntry {
    pub key: String,
    pub value: String,
    pub updated_at: String,
}

#[server]
pub async fn get_app_config() -> Result<Vec<ConfigEntry>, ServerFnError> {
    let pool = server_pool()?;
    let rows = sqlx::query_as::<_, (String, String, String)>(
        "SELECT key, value, updated_at FROM app_config ORDER BY key",
    )
    .fetch_all(&pool)
    .await
    .map_err(|e| ServerFnError::new(format!("{e}")))?;
    Ok(rows
        .into_iter()
        .map(|(key, value, updated_at)| ConfigEntry {
            key,
            value,
            updated_at,
        })
        .collect())
}

#[server]
pub async fn update_app_config(key: String, value: String) -> Result<(), ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;

    let context = server_context()?;
    let normalized_value = if key == FETCH_SEEDING_SIZE_CONFIG_KEY {
        match value.trim().to_ascii_lowercase().as_str() {
            "true" | "1" => "true".to_string(),
            "false" | "0" => "false".to_string(),
            _ => return Err(ServerFnError::new("做种大小开关的值必须为 true 或 false")),
        }
    } else {
        value
    };

    Repository::new(context.pool.clone())
        .set_config(&key, &normalized_value)
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;

    if key == FETCH_SEEDING_SIZE_CONFIG_KEY {
        context.fetch_seeding_size.store(
            normalized_value == "true",
            std::sync::atomic::Ordering::Relaxed,
        );
    }
    Ok(())
}
