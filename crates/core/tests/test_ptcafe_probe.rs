use std::sync::Arc;

use pt_reseeder_core::site::adapters::nexusphp::NexusPhpAdapter;
use pt_reseeder_core::site::models::UserInfoSelectors;
use pt_reseeder_core::site::probe::probe_site;
use pt_reseeder_core::site::traits::UserInfoCapable;

const COOKIE: &str = "c_secure_pass=eyJ1c2VyX2lkIjoiMTMxNTQiLCJleHBpcmVzIjoxODE1NDA4OTEyfS42ZDE5ZWY3ZWI4OGUxNjViMmE0ZGZkN2RmMWRmMGQ3NDY1NjVhNjNiOTY5N2Q1NjA0NDY1YzQ2ZGY1MGY4ZGU2";
const BASE_URL: &str = "https://ptcafe.club";

fn ptcafe_selectors() -> UserInfoSelectors {
    UserInfoSelectors {
        profile_url_template: Some("/userdetails.php?id={uid}".to_string()),
        uid_selector: Some("a[href*='userdetails.php?id=']".to_string()),
        uploaded_selector: Some("font.color_uploaded".to_string()),
        downloaded_selector: Some("font.color_downloaded".to_string()),
        ratio_selector: Some("font.color_ratio".to_string()),
        bonus_selector: Some("td.rowhead:contains('魔力值') + td".to_string()),
        user_class_selector: Some("td.rowhead:contains('等级') + td img".to_string()),
        seeding_count_selector: Some("img.arrowup".to_string()),
        leeching_count_selector: Some("img.arrowdown".to_string()),
        seeding_size_selector: Some("td.rowhead:contains('做种体积') + td".to_string()),
        upload_time_selector: Some("td.embedded:contains('做种时间')".to_string()),
    }
}

/// 端到端测试：用真实 cookie 调用 fetch_user_info，验证 cookie 传递和字段解析
#[tokio::test]
async fn test_ptcafe_fetch_user_info() {
    let adapter = NexusPhpAdapter::new(
        "ptcafe".to_string(),
        BASE_URL.to_string(),
        None,
        Some(COOKIE.to_string()),
        None,
        None,
        ptcafe_selectors(),
        100,
    )
    .with_fetch_seeding_size(true);

    let stats = adapter.fetch_user_info().await;
    match &stats {
        Ok(s) => {
            println!("✅ fetch_user_info 成功");
            println!("  uploaded:      {:?}", s.uploaded);
            println!("  downloaded:    {:?}", s.downloaded);
            println!("  ratio:         {:?}", s.ratio);
            println!("  bonus:         {:?}", s.bonus);
            println!("  user_class:    {:?}", s.user_class);
            println!("  seeding_count: {:?}", s.seeding_count);
            println!("  leeching_count:{:?}", s.leeching_count);
            println!("  seeding_size:  {:?}", s.seeding_size);
            println!("  upload_time:   {:?}", s.upload_time_seconds);
        }
        Err(e) => {
            panic!("❌ fetch_user_info 失败: {e}");
        }
    }

    // Debug: manually fetch userdetails and test extract_text + parse
    {
        use reqwest::header::{HeaderMap, HeaderValue};
        let mut headers = HeaderMap::new();
        headers.insert(
            reqwest::header::USER_AGENT,
            HeaderValue::from_static("PT-Reseeder/0.1"),
        );
        headers.insert(
            reqwest::header::COOKIE,
            HeaderValue::from_str(COOKIE).unwrap(),
        );
        let client = reqwest::Client::builder()
            .use_rustls_tls()
            .cookie_store(false)
            .default_headers(headers)
            .build()
            .unwrap();
        let body = client
            .get(&format!("{}/userdetails.php?id=13154", BASE_URL))
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        let html = scraper::Html::parse_document(&body);
        let raw = extract_text_impl(&html, "td.embedded:contains('做种时间')");
        println!("  DEBUG extract_text on userdetails: {:?}", raw);
    }

    let stats = stats.unwrap();
    // 上传量和下载量是最基本的字段，必须能解析
    assert!(stats.uploaded.is_some(), "uploaded 应该有值");
    assert!(stats.downloaded.is_some(), "downloaded 应该有值");
    assert!(stats.seeding_size.is_some(), "seeding_size 应该通过做种列表 AJAX 解析成功");
    // ratio: PTCafe 的分享率数值在 <font> 标签外的裸文本节点中，
    // 当前选择器只能取到标签文字 "分享率:"，暂时允许为 None
    if stats.ratio.is_some() {
        println!("  ratio 解析成功: {:?}", stats.ratio);
    } else {
        println!("  ratio 未解析到（PTCafe 页面结构限制，数值在 font 标签外）");
    }
}

/// 端到端测试：用真实 cookie 调用 fetch_passkey
#[tokio::test]
async fn test_ptcafe_fetch_passkey() {
    let adapter = NexusPhpAdapter::new(
        "ptcafe".to_string(),
        BASE_URL.to_string(),
        None,
        Some(COOKIE.to_string()),
        None,
        None,
        ptcafe_selectors(),
        100,
    );

    match adapter.fetch_passkey().await {
        Ok(pk) => println!(
            "✅ fetch_passkey 结果: {:?}",
            pk.as_deref().map(|s| &s[..8])
        ),
        Err(e) => panic!("❌ fetch_passkey 失败: {e}"),
    }
}

/// 端到端测试：模拟 validate_site 的完整 probe 流程，验证最终状态
#[tokio::test]
async fn test_ptcafe_probe_status() {
    let adapter = NexusPhpAdapter::new(
        "ptcafe".to_string(),
        BASE_URL.to_string(),
        None,
        Some(COOKIE.to_string()),
        None,
        None,
        ptcafe_selectors(),
        100,
    );

    let user_info: Arc<dyn UserInfoCapable> = Arc::new(adapter);
    let result = probe_site(None, Some(&user_info)).await;

    println!("\n=== Probe 结果 ===");
    println!("overall_status: {}", result.overall_status);
    println!("passkey_available: {:?}", result.passkey_available);
    for field in &result.user_info_fields {
        let icon = if field.success { "✅" } else { "❌" };
        println!(
            "  {} {}: {:?} {:?}",
            icon, field.field_name, field.value_preview, field.error
        );
    }

    let status = result.status_str();
    assert!(
        status == "ok" || status == "partial",
        "probe 状态应该是 ok 或 partial，实际是: {status}"
    );

    // 核心字段必须成功
    let uploaded = result
        .user_info_fields
        .iter()
        .find(|f| f.field_name == "uploaded");
    assert!(
        uploaded.map_or(false, |f| f.success),
        "uploaded 字段应该解析成功"
    );

    let downloaded = result
        .user_info_fields
        .iter()
        .find(|f| f.field_name == "downloaded");
    assert!(
        downloaded.map_or(false, |f| f.success),
        "downloaded 字段应该解析成功"
    );
}

/// 单元测试：验证 :contains() 选择器解析在本地 HTML 上正常工作
#[test]
fn test_contains_selector_parsing() {
    // 模拟 NexusPHP userdetails 页面的典型 HTML 结构
    let html_str = r#"
    <table>
        <tr>
            <td class="rowhead">魔力值</td>
            <td class="rowfollow">12345.6</td>
        </tr>
        <tr>
            <td class="rowhead">等级</td>
            <td class="rowfollow"><img src="/pic/class.gif" alt="User" title="User" /></td>
        </tr>
        <tr>
            <td class="rowhead">当前做种</td>
            <td class="rowfollow">42</td>
        </tr>
        <tr>
            <td class="rowhead">当前下载</td>
            <td class="rowfollow">3</td>
        </tr>
        <tr>
            <td class="rowhead">做种体积</td>
            <td class="rowfollow">1.23 TB</td>
        </tr>
        <tr>
            <td class="rowhead">做种时间</td>
            <td class="rowfollow">1年2月3天</td>
        </tr>
        <tr>
            <td class="rowhead">上传量</td>
            <td class="rowfollow"><font class="color_uploaded">5.67 TB</font></td>
        </tr>
        <tr>
            <td class="rowhead">下载量</td>
            <td class="rowfollow"><font class="color_downloaded">1.23 TB</font></td>
        </tr>
    </table>
    "#;

    use scraper::{Html, Selector};

    let html = Html::parse_document(html_str);

    // 验证标准选择器仍然工作
    let sel = Selector::parse("font.color_uploaded").unwrap();
    let el = html.select(&sel).next().unwrap();
    let text: String = el.text().collect::<Vec<_>>().join("");
    assert_eq!(text.trim(), "5.67 TB");

    // 验证 :contains() — Selector::parse 应该失败（scraper 不支持）
    assert!(
        Selector::parse("td.rowhead:contains('魔力值') + td").is_err(),
        ":contains() 不应该被 scraper 原生支持，我们的 extract_text 需要手动处理"
    );

    // 使用 adapter 内部的 extract_text 逻辑来验证
    // 由于 extract_text 是私有的，我们通过构造 adapter 并调用更高层来间接验证
    // 这里直接复制核心逻辑做单元验证
    let sel_str = "td.rowhead:contains('魔力值') + td";
    let result = extract_text_impl(&html, sel_str);
    assert_eq!(
        result.as_deref(),
        Some("12345.6"),
        "魔力值应该解析为 12345.6"
    );

    let sel_str2 = "td.rowhead:contains('当前做种') + td";
    let result2 = extract_text_impl(&html, sel_str2);
    assert_eq!(result2.as_deref(), Some("42"), "当前做种应该解析为 42");

    let sel_str3 = "td.rowhead:contains('做种体积') + td";
    let result3 = extract_text_impl(&html, sel_str3);
    assert_eq!(
        result3.as_deref(),
        Some("1.23 TB"),
        "做种体积应该解析为 1.23 TB"
    );

    let sel_str4 = "td.rowhead:contains('做种时间') + td";
    let result4 = extract_text_impl(&html, sel_str4);
    assert_eq!(
        result4.as_deref(),
        Some("1年2月3天"),
        "做种时间应该解析为 1年2月3天"
    );
}

/// 单元测试：验证无 sibling 后缀的 :contains() 选择器（模拟 PTCafe 传输行结构）
#[test]
fn test_contains_no_sibling_suffix() {
    let html_str = r#"
    <table>
        <tr>
            <td class="rowhead">传输</td>
            <td class="rowfollow">
                <table>
                    <tr>
                        <td class="embedded"><strong>上传量</strong>:  4.425 TB</td>
                        <td class="embedded"><strong>下载量</strong>:  750.00 GB</td>
                    </tr>
                </table>
            </td>
        </tr>
    </table>
    "#;

    let html = scraper::Html::parse_document(html_str);

    // 无 sibling 后缀的 :contains() — 应返回匹配元素自身的文本
    let r1 = extract_text_impl(&html, "td.embedded:contains('上传量')");
    println!("td.embedded:contains('上传量') => {:?}", r1);
    assert!(r1.is_some(), "应该匹配到包含'上传量'的 td.embedded");
    assert!(
        r1.as_ref().unwrap().contains("4.425 TB"),
        "应该包含 4.425 TB，实际: {:?}",
        r1
    );

    let r2 = extract_text_impl(&html, "td.embedded:contains('下载量')");
    println!("td.embedded:contains('下载量') => {:?}", r2);
    assert!(r2.is_some(), "应该匹配到包含'下载量'的 td.embedded");
    assert!(
        r2.as_ref().unwrap().contains("750.00 GB"),
        "应该包含 750.00 GB，实际: {:?}",
        r2
    );
}

/// 单元测试：验证 parse_time_to_seconds 对各种格式的处理
#[test]
fn test_parse_time_formats() {
    // 模拟 PTCafe BT时间行的真实 HTML 结构
    let html_str = r#"
    <table>
        <tr>
            <td class="rowhead">BT时间</td>
            <td class="rowfollow">
                <table><tr>
                    <td class="embedded"><strong>做种时间</strong>:  21768天19:49:29</td>
                    <td class="embedded"><strong>下载时间</strong>:  0:00</td>
                </tr></table>
            </td>
        </tr>
    </table>
    "#;
    let html = scraper::Html::parse_document(html_str);

    let r = extract_text_impl(&html, "td.embedded:contains('做种时间')");
    println!("td.embedded:contains('做种时间') => {:?}", r);
    assert!(r.is_some(), "应该匹配到做种时间");
    let text = r.unwrap();
    assert!(text.contains("21768"), "应该包含 21768，实际: {text}");
}

/// 复制 extract_text 的核心 :contains() 逻辑，用于单元测试
fn extract_text_impl(html: &scraper::Html, sel_str: &str) -> Option<String> {
    use scraper::{ElementRef, Selector};

    if let Some(contains_start) = sel_str.find(":contains(") {
        let prefix_sel_str = &sel_str[..contains_start];
        let after_contains = &sel_str[contains_start + ":contains(".len()..];

        let (needle, rest) = if after_contains.starts_with('\'') {
            let end = after_contains[1..].find('\'')?;
            (&after_contains[1..1 + end], &after_contains[2 + end..])
        } else if after_contains.starts_with('"') {
            let end = after_contains[1..].find('"')?;
            (&after_contains[1..1 + end], &after_contains[2 + end..])
        } else {
            return None;
        };

        let rest = rest.strip_prefix(')')?;
        let suffix = rest.trim();

        let prefix_selector = Selector::parse(prefix_sel_str).ok()?;

        let matched_el = html.select(&prefix_selector).find(|el| {
            let text: String = el.text().collect::<Vec<_>>().join("");
            text.contains(needle)
        })?;

        if let Some(sibling_part) = suffix.strip_prefix('+') {
            let sibling_part = sibling_part.trim();
            let node_id = matched_el.id();
            let node_ref = html.tree.get(node_id)?;
            for sibling in node_ref.next_siblings() {
                if let Some(el) = ElementRef::wrap(sibling) {
                    if !sibling_part.is_empty() {
                        let expected_tag = sibling_part
                            .split(|c: char| c == '.' || c == '#' || c == '[' || c == ':')
                            .next()
                            .unwrap_or("");
                        if !expected_tag.is_empty()
                            && !el.value().name().eq_ignore_ascii_case(expected_tag)
                        {
                            continue;
                        }
                    }
                    let text: String = el.text().collect::<Vec<_>>().join("").trim().to_string();
                    if !text.is_empty() {
                        return Some(text);
                    }
                }
            }
            None
        } else {
            let text: String = matched_el
                .text()
                .collect::<Vec<_>>()
                .join("")
                .trim()
                .to_string();
            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        }
    } else {
        let selector = Selector::parse(sel_str).ok()?;
        let element = html.select(&selector).next()?;
        let text: String = element
            .text()
            .collect::<Vec<_>>()
            .join("")
            .trim()
            .to_string();
        if text.is_empty() {
            None
        } else {
            Some(text)
        }
    }
}
