// Sites: DTOs and server functions for CRUD, credential updates and probing.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteUserInfo {
    pub site_id: i64,
    pub site_name: String,
    pub uploaded: Option<i64>,
    pub downloaded: Option<i64>,
    pub ratio: Option<f64>,
    pub bonus: Option<f64>,
    pub user_class: Option<String>,
    pub seeding_count: Option<i64>,
    pub leeching_count: Option<i64>,
    pub seeding_size: Option<i64>,
    pub upload_time_seconds: Option<i64>,
    pub fetched_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteDefinitionInfo {
    pub id: String,
    pub name: String,
    pub url: String,
    pub api_url: Option<String>,
    pub adapter: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteInfo {
    pub id: i64,
    pub name: String,
    pub url: String,
    pub api_url: Option<String>,
    pub adapter_type: String,
    pub auth_type: String,
    pub probe_status: String,
    pub probe_detail_json: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteDetailData {
    pub site: SiteInfo,
    pub user_stats: Option<SiteUserInfo>,
    pub probe_detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateSiteResult {
    pub status: String,
    pub message: String,
    pub detail_json: Option<String>,
}

#[server]
pub async fn get_sites() -> Result<Vec<SiteInfo>, ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;

    let repo = Repository::new(server_pool()?);
    let sites = repo
        .list_sites()
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;
    Ok(sites
        .into_iter()
        .map(|s| SiteInfo {
            id: s.id,
            name: s.name,
            url: s.url,
            api_url: s.api_url,
            adapter_type: s.adapter_type,
            auth_type: s.auth_type,
            probe_status: s.probe_status,
            probe_detail_json: s.probe_detail_json,
            enabled: s.enabled,
        })
        .collect())
}

#[server]
pub async fn get_site_definitions() -> Result<Vec<SiteDefinitionInfo>, ServerFnError> {
    use pt_reseeder_core::site::definitions::load_all_definitions;

    let context = server_context()?;
    let definitions = load_all_definitions(Some(&context.data_dir));
    let mut results: Vec<SiteDefinitionInfo> = definitions
        .into_values()
        .map(|def| SiteDefinitionInfo {
            id: def.site.id,
            name: def.site.name,
            url: def.site.url,
            api_url: def.site.api_url,
            adapter: def.site.adapter,
        })
        .collect();
    results.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(results)
}

#[server]
pub async fn update_site_url(
    id: i64,
    url: String,
    api_url: String,
) -> Result<SiteInfo, ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;

    let url = url.trim().to_string();
    if url.is_empty() {
        return Err(ServerFnError::new("URL 不能为空"));
    }

    let repo = Repository::new(server_pool()?);
    let api_url_opt = if api_url.trim().is_empty() {
        None
    } else {
        Some(api_url.trim().to_string())
    };
    repo.update_site_url(id, &url, api_url_opt.as_deref())
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;
    refresh_site_registry_best_effort(&server_context()?).await;
    get_site_info(id).await
}

#[server]
pub async fn update_site(
    id: i64,
    url: String,
    api_url: String,
    cookie: String,
    passkey: String,
) -> Result<SiteInfo, ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;

    let url = url.trim().to_string();
    if url.is_empty() {
        return Err(ServerFnError::new("URL 不能为空"));
    }

    let context = server_context()?;
    let vault = context
        .vault
        .read()
        .await
        .clone()
        .ok_or_else(|| ServerFnError::new("凭证已锁定，请重新登录后再操作"))?;
    let repo = Repository::new(context.pool.clone());

    // Update URL
    let api_url_opt = if api_url.trim().is_empty() {
        None
    } else {
        Some(api_url.trim().to_string())
    };
    repo.update_site_url(id, &url, api_url_opt.as_deref())
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;

    // Update credentials (only if user provided new values; empty means keep existing)
    let cookie_trimmed = cookie.trim().to_string();
    let passkey_trimmed = passkey.trim().to_string();
    if !cookie_trimmed.is_empty() || !passkey_trimmed.is_empty() {
        // Load existing credentials to preserve unchanged fields
        let site_row = repo
            .get_site(id)
            .await
            .map_err(|e| ServerFnError::new(format!("{e}")))?
            .ok_or_else(|| ServerFnError::new("站点不存在"))?;

        let (encrypted_cookie, cookie_nonce) = if !cookie_trimmed.is_empty() {
            encrypt_optional(&vault, &cookie_trimmed)?
        } else {
            (
                site_row.encrypted_cookie.clone(),
                site_row.cookie_nonce.clone(),
            )
        };
        let (encrypted_passkey, passkey_nonce) = if !passkey_trimmed.is_empty() {
            encrypt_optional(&vault, &passkey_trimmed)?
        } else {
            (
                site_row.encrypted_passkey.clone(),
                site_row.passkey_nonce.clone(),
            )
        };

        repo.update_site_credentials(
            id,
            encrypted_cookie.as_deref(),
            cookie_nonce.as_deref(),
            encrypted_passkey.as_deref(),
            passkey_nonce.as_deref(),
            site_row.encrypted_token.as_deref(),
            site_row.token_nonce.as_deref(),
        )
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;
    }

    refresh_site_registry_best_effort(&context).await;
    get_site_info(id).await
}

#[server]
pub async fn create_site(
    name: String,
    url: String,
    api_url: String,
    adapter_type: String,
    auth_type: String,
    cookie: String,
    passkey: String,
) -> Result<SiteInfo, ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;

    let context = server_context()?;
    let vault = context
        .vault
        .read()
        .await
        .clone()
        .ok_or_else(|| ServerFnError::new("凭证已锁定，请重新登录后再创建站点"))?;
    let repo = Repository::new(context.pool.clone());
    let adapter = adapter_type.to_ascii_lowercase();
    if !matches!(
        adapter.as_str(),
        "nexusphp" | "mteam" | "unit3d" | "gazelle" | "zhuque"
    ) {
        return Err(ServerFnError::new(format!(
            "不支持的站点架构：{adapter_type}"
        )));
    }
    let id = match repo
        .create_site(
            &name,
            &url,
            (!api_url.trim().is_empty()).then_some(api_url.as_str()),
            &adapter,
            &auth_type,
        )
        .await
    {
        Ok(id) => id,
        Err(error) => return Err(ServerFnError::new(format!("{error}"))),
    };
    let credential_result = async {
        let (encrypted_cookie, cookie_nonce) = encrypt_optional(&vault, &cookie)?;
        let (encrypted_passkey, passkey_nonce) = encrypt_optional(&vault, &passkey)?;
        repo.update_site_credentials(
            id,
            encrypted_cookie.as_deref(),
            cookie_nonce.as_deref(),
            encrypted_passkey.as_deref(),
            passkey_nonce.as_deref(),
            None,
            None,
        )
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))
    }
    .await;
    if let Err(error) = credential_result {
        let _ = repo.delete_site(id).await;
        return Err(error);
    }
    refresh_site_registry_best_effort(&context).await;

    // 后台抓取一次用户数据，不阻塞站点创建流程
    {
        let pool = context.pool.clone();
        let site_registry = context.site_registry.clone();
        let site_id = id;
        tokio::spawn(async move {
            use pt_reseeder_core::db::models::UserStatRecord;
            use pt_reseeder_core::db::repo::Repository;
            use pt_reseeder_core::site::models::SiteId;

            let registry = site_registry.read().await.clone();
            let handle = registry.get(&SiteId::from(site_id));
            let user_info_cap = handle.and_then(|h| h.user_info.as_ref());
            if let Some(ui) = user_info_cap {
                match ui.fetch_user_info().await {
                    Ok(stats) => {
                        let repo = Repository::new(pool);
                        let record = UserStatRecord {
                            id: 0,
                            site_id,
                            uploaded: stats.uploaded,
                            downloaded: stats.downloaded,
                            ratio: stats.ratio,
                            bonus: stats.bonus,
                            user_class: stats.user_class,
                            seeding_count: stats.seeding_count,
                            leeching_count: stats.leeching_count,
                            seeding_size: stats.seeding_size,
                            upload_time_seconds: stats.upload_time_seconds,
                            fetched_at: String::new(),
                        };
                        if let Err(e) = repo.insert_user_stats(site_id, &record).await {
                            eprintln!("创建站点后自动抓取用户数据写入失败: {e}");
                        }
                    }
                    Err(e) => {
                        eprintln!("创建站点后自动抓取用户数据失败: {e}");
                    }
                }
            }
        });
    }

    get_site_info(id).await
}

#[server]
pub async fn validate_site(
    name: String,
    url: String,
    api_url: String,
    adapter_type: String,
    cookie: String,
    passkey: String,
) -> Result<ValidateSiteResult, ServerFnError> {
    use pt_reseeder_core::site::adapters::gazelle::GazelleAdapter;
    use pt_reseeder_core::site::adapters::mteam::MTeamAdapter;
    use pt_reseeder_core::site::adapters::nexusphp::NexusPhpAdapter;
    use pt_reseeder_core::site::adapters::unit3d::Unit3dAdapter;
    use pt_reseeder_core::site::adapters::zhuque::ZhuqueAdapter;
    use pt_reseeder_core::site::definitions::load_all_definitions;
    use pt_reseeder_core::site::models::UserInfoSelectors;
    use pt_reseeder_core::site::probe::probe_site as run_site_probe;
    use pt_reseeder_core::site::traits::{ReseedCapable, UserInfoCapable};
    use std::sync::Arc;

    /// Reseed + user-info adapters built for one probe; either may be absent
    /// depending on the site type and which credentials were supplied.
    type ProbeCapabilities = (
        Option<Arc<dyn ReseedCapable>>,
        Option<Arc<dyn UserInfoCapable>>,
    );

    let context = server_context()?;
    let adapter = adapter_type.to_ascii_lowercase();
    let api_url_opt = (!api_url.trim().is_empty()).then_some(api_url);
    let cookie_opt = (!cookie.trim().is_empty()).then_some(cookie);
    let passkey_opt = (!passkey.trim().is_empty()).then_some(passkey);

    let definitions = load_all_definitions(Some(&context.data_dir));
    let selectors = definitions
        .get(&name)
        .and_then(|def| def.user_info.clone())
        .unwrap_or_else(|| UserInfoSelectors {
            profile_url_template: None,
            uid_selector: None,
            uploaded_selector: None,
            downloaded_selector: None,
            ratio_selector: None,
            bonus_selector: None,
            user_class_selector: None,
            seeding_count_selector: None,
            leeching_count_selector: None,
            seeding_size_selector: None,
            upload_time_selector: None,
        });

    let fetch_seeding_size = context
        .fetch_seeding_size
        .load(std::sync::atomic::Ordering::Relaxed);

    // 同时持有 reseed + user_info，有 api_url/passkey 时连通测试会真正打辅种 API。
    let (reseed, user_info): ProbeCapabilities = match adapter.as_str() {
        "nexusphp" => {
            let adapter = Arc::new(
                NexusPhpAdapter::new(
                    name,
                    url,
                    api_url_opt,
                    cookie_opt,
                    passkey_opt,
                    None,
                    selectors,
                    100,
                )
                .with_fetch_seeding_size(fetch_seeding_size),
            );
            (Some(adapter.clone()), Some(adapter))
        }
        "mteam" => {
            let adapter = Arc::new(MTeamAdapter::new(name, url, None, passkey_opt, 100));
            (Some(adapter.clone()), Some(adapter))
        }
        "unit3d" => {
            let adapter = Arc::new(Unit3dAdapter::new(name, url, None, passkey_opt, 100));
            (Some(adapter.clone()), Some(adapter))
        }
        "gazelle" => {
            let adapter = Arc::new(GazelleAdapter::new(name, url, cookie_opt, passkey_opt, 100));
            (Some(adapter.clone()), Some(adapter))
        }
        "zhuque" => {
            let adapter = Arc::new(ZhuqueAdapter::new(
                name,
                url,
                None,
                passkey_opt,
                cookie_opt,
                100,
            ));
            (Some(adapter.clone()), Some(adapter))
        }
        other => {
            return Ok(ValidateSiteResult {
                status: "failed".to_string(),
                message: format!("不支持的站点架构：{other}"),
                detail_json: None,
            });
        }
    };

    let probe = run_site_probe(reseed.as_ref(), user_info.as_ref()).await;
    let status = probe.status_str().to_string();
    let detail = probe.to_json();
    let message = match status.as_str() {
        "ok" => "校验通过，站点连通正常".to_string(),
        "partial" => "站点可访问，但部分指标未获取或不受支持，请查看具体项目".to_string(),
        "failed" => "校验失败，无法连接站点或凭证无效".to_string(),
        _ => "校验结果未知".to_string(),
    };

    Ok(ValidateSiteResult {
        status,
        message,
        detail_json: Some(detail),
    })
}

#[server]
pub async fn delete_site(id: i64) -> Result<(), ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;

    let context = server_context()?;
    Repository::new(context.pool.clone())
        .delete_site(id)
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;
    refresh_site_registry_best_effort(&context).await;
    Ok(())
}

#[server]
pub async fn probe_site(id: i64) -> Result<ValidateSiteResult, ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;
    use pt_reseeder_core::site::probe::probe_site as run_site_probe;

    let context = server_context()?;
    let repo = Repository::new(context.pool.clone());
    let site = repo
        .get_site(id)
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?
        .ok_or_else(|| ServerFnError::new("site not found"))?;

    let registry = context.site_registry.read().await.clone();
    let handle = registry
        .get(&pt_reseeder_core::site::models::SiteId::from(site.id))
        .cloned()
        .ok_or_else(|| ServerFnError::new("站点适配器未注册，请确认凭证已解锁且站点架构受支持"))?;
    let probe = run_site_probe(handle.reseed.as_ref(), handle.user_info.as_ref()).await;
    let status = probe.status_str().to_string();
    let detail = probe.to_json();
    repo.update_probe_status(id, &status, Some(&detail))
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?;

    let message = match status.as_str() {
        "ok" => "校验通过，站点连通正常".to_string(),
        "partial" => "站点可访问，但部分指标未获取或不受支持，请查看具体项目".to_string(),
        "failed" => "校验失败，无法连接站点或凭证无效".to_string(),
        _ => "校验结果未知".to_string(),
    };

    Ok(ValidateSiteResult {
        status,
        message,
        detail_json: Some(detail),
    })
}

#[server]
pub async fn get_site_detail(id: i64) -> Result<SiteDetailData, ServerFnError> {
    use pt_reseeder_core::db::repo::Repository;

    let repo = Repository::new(server_pool()?);
    let site = repo
        .get_site(id)
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?
        .ok_or_else(|| ServerFnError::new("site not found"))?;
    let user_stats = repo
        .get_latest_stats_by_site(id)
        .await
        .map_err(|e| ServerFnError::new(format!("{e}")))?
        .map(|s| SiteUserInfo {
            site_id: s.site_id,
            site_name: site.name.clone(),
            uploaded: s.uploaded,
            downloaded: s.downloaded,
            ratio: s.ratio,
            bonus: s.bonus,
            user_class: s.user_class,
            seeding_count: s.seeding_count,
            leeching_count: s.leeching_count,
            seeding_size: s.seeding_size,
            upload_time_seconds: s.upload_time_seconds,
            fetched_at: s.fetched_at,
        });
    Ok(SiteDetailData {
        probe_detail: site.probe_detail_json.clone(),
        site: SiteInfo {
            id: site.id,
            name: site.name,
            url: site.url,
            api_url: site.api_url,
            adapter_type: site.adapter_type,
            auth_type: site.auth_type,
            probe_status: site.probe_status,
            probe_detail_json: site.probe_detail_json,
            enabled: site.enabled,
        },
        user_stats,
    })
}

#[server]
pub async fn refresh_site_stats(id: i64) -> Result<(), ServerFnError> {
    let _ = get_site_detail(id).await?;
    Ok(())
}

#[server]
async fn get_site_info(id: i64) -> Result<SiteInfo, ServerFnError> {
    Ok(get_site_detail(id).await?.site)
}
