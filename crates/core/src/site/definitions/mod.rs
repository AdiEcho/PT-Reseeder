use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, info, warn};

use crate::site::models::SiteDefinition;

const BUILTIN_HDSKY: &str = include_str!("hdsky.toml");
const BUILTIN_MTEAM: &str = include_str!("mteam.toml");
const BUILTIN_AUDIENCES: &str = include_str!("audiences.toml");
const BUILTIN_PTERCLUB: &str = include_str!("pterclub.toml");
const BUILTIN_OURBITS: &str = include_str!("ourbits.toml");
const BUILTIN_CHDBITS: &str = include_str!("chdbits.toml");
const BUILTIN_TTG: &str = include_str!("ttg.toml");
const BUILTIN_DMHY: &str = include_str!("dmhy.toml");
const BUILTIN_HDFANS: &str = include_str!("hdfans.toml");
const BUILTIN_HDAREA: &str = include_str!("hdarea.toml");
const BUILTIN_PTHOME: &str = include_str!("pthome.toml");
const BUILTIN_LEMONHD: &str = include_str!("lemonhd.toml");
const BUILTIN_HDHOME: &str = include_str!("hdhome.toml");
const BUILTIN_SPRINGSUNDAY: &str = include_str!("springsunday.toml");
const BUILTIN_KEEPFRDS: &str = include_str!("keepfrds.toml");
const BUILTIN_HDCITY: &str = include_str!("hdcity.toml");
const BUILTIN_BEITAI: &str = include_str!("beitai.toml");
const BUILTIN_HAIDAN: &str = include_str!("haidan.toml");
const BUILTIN_PIGGO: &str = include_str!("piggo.toml");
const BUILTIN_HARES: &str = include_str!("hares.toml");
const BUILTIN_ZHUQUE: &str = include_str!("zhuque.toml");
const BUILTIN_GREATPOSTERWALL: &str = include_str!("greatposterwall.toml");
const BUILTIN_DICMUSIC: &str = include_str!("dicmusic.toml");
const BUILTIN_AITHER: &str = include_str!("aither.toml");
const BUILTIN_BLUTOPIA: &str = include_str!("blutopia.toml");
const BUILTIN_LEAGUHD: &str = include_str!("leaguhd.toml");
const BUILTIN_BTSCHOOL: &str = include_str!("btschool.toml");
const BUILTIN_HDATMOS: &str = include_str!("hdatmos.toml");
const BUILTIN_REDACTED: &str = include_str!("redacted.toml");
const BUILTIN_ORPHEUS: &str = include_str!("orpheus.toml");

/// Load all built-in site definitions embedded at compile time.
pub fn load_builtin_definitions() -> HashMap<String, SiteDefinition> {
    let mut definitions = HashMap::new();

    let builtins = [
        ("hdsky", BUILTIN_HDSKY),
        ("mteam", BUILTIN_MTEAM),
        ("audiences", BUILTIN_AUDIENCES),
        ("pterclub", BUILTIN_PTERCLUB),
        ("ourbits", BUILTIN_OURBITS),
        ("chdbits", BUILTIN_CHDBITS),
        ("ttg", BUILTIN_TTG),
        ("dmhy", BUILTIN_DMHY),
        ("hdfans", BUILTIN_HDFANS),
        ("hdarea", BUILTIN_HDAREA),
        ("pthome", BUILTIN_PTHOME),
        ("lemonhd", BUILTIN_LEMONHD),
        ("hdhome", BUILTIN_HDHOME),
        ("springsunday", BUILTIN_SPRINGSUNDAY),
        ("keepfrds", BUILTIN_KEEPFRDS),
        ("hdcity", BUILTIN_HDCITY),
        ("beitai", BUILTIN_BEITAI),
        ("haidan", BUILTIN_HAIDAN),
        ("piggo", BUILTIN_PIGGO),
        ("hares", BUILTIN_HARES),
        ("zhuque", BUILTIN_ZHUQUE),
        ("greatposterwall", BUILTIN_GREATPOSTERWALL),
        ("dicmusic", BUILTIN_DICMUSIC),
        ("aither", BUILTIN_AITHER),
        ("blutopia", BUILTIN_BLUTOPIA),
        ("leaguhd", BUILTIN_LEAGUHD),
        ("btschool", BUILTIN_BTSCHOOL),
        ("hdatmos", BUILTIN_HDATMOS),
        ("redacted", BUILTIN_REDACTED),
        ("orpheus", BUILTIN_ORPHEUS),
    ];

    for (name, toml_str) in &builtins {
        match toml_lib::from_str::<SiteDefinition>(toml_str) {
            Ok(def) => {
                debug!("Loaded builtin site definition: {}", def.site.id);
                definitions.insert(def.site.id.clone(), def);
            }
            Err(e) => {
                warn!("Failed to parse builtin site definition '{}': {}", name, e);
            }
        }
    }

    definitions
}

/// Load user-provided site definitions from a directory.
///
/// Looks for `*.toml` files in `{data_dir}/sites/`.
/// Parse errors are logged as warnings but do not cause failure.
pub fn load_user_definitions(data_dir: &Path) -> HashMap<String, SiteDefinition> {
    let mut definitions = HashMap::new();
    let sites_dir = data_dir.join("sites");

    if !sites_dir.exists() {
        debug!(
            "User sites directory does not exist: {}",
            sites_dir.display()
        );
        return definitions;
    }

    let entries = match std::fs::read_dir(&sites_dir) {
        Ok(entries) => entries,
        Err(e) => {
            warn!(
                "Failed to read user sites directory {}: {}",
                sites_dir.display(),
                e
            );
            return definitions;
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                warn!("Failed to read directory entry: {}", e);
                continue;
            }
        };

        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
            continue;
        }

        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                warn!(
                    "Failed to read site definition file {}: {}",
                    path.display(),
                    e
                );
                continue;
            }
        };

        match toml_lib::from_str::<SiteDefinition>(&content) {
            Ok(def) => {
                debug!(
                    "Loaded user site definition: {} from {}",
                    def.site.id,
                    path.display()
                );
                definitions.insert(def.site.id.clone(), def);
            }
            Err(e) => {
                warn!(
                    "Failed to parse user site definition {}: {}",
                    path.display(),
                    e
                );
            }
        }
    }

    definitions
}

/// Load all site definitions: built-in first, then user overrides.
///
/// User definitions with the same `site.id` override built-in ones.
pub fn load_all_definitions(data_dir: Option<&Path>) -> HashMap<String, SiteDefinition> {
    let mut definitions = load_builtin_definitions();
    let builtin_count = definitions.len();

    let mut user_override_count = 0;
    if let Some(dir) = data_dir {
        let user_defs = load_user_definitions(dir);
        for (id, def) in user_defs {
            if definitions.contains_key(&id) {
                debug!("User definition overrides builtin for site: {}", id);
                user_override_count += 1;
            }
            definitions.insert(id, def);
        }
    }

    let total = definitions.len();
    info!(
        "Loaded {} site definitions ({} builtin, {} user override)",
        total, builtin_count, user_override_count
    );

    definitions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_builtin_definitions_parse() {
        let definitions = load_builtin_definitions();
        // We have 30 builtin definitions
        assert!(
            definitions.len() >= 30,
            "Expected at least 30 builtin definitions, got {}",
            definitions.len()
        );

        // Verify key sites are present
        let expected_sites = [
            "hdsky",
            "mteam",
            "audiences",
            "pterclub",
            "ourbits",
            "hdhome",
            "springsunday",
            "hdfans",
            "leaguhd",
            "btschool",
            "hdatmos",
            "zhuque",
            "aither",
            "blutopia",
            "redacted",
            "orpheus",
            "greatposterwall",
            "dicmusic",
        ];
        for site_id in &expected_sites {
            assert!(
                definitions.contains_key(*site_id),
                "Missing expected site definition: {}",
                site_id
            );
        }
    }

    #[test]
    fn test_nexusphp_sites_have_user_info() {
        let definitions = load_builtin_definitions();
        let nexusphp_sites = [
            "hdsky",
            "audiences",
            "pterclub",
            "ourbits",
            "hdhome",
            "springsunday",
            "hdfans",
            "leaguhd",
            "btschool",
            "hdatmos",
        ];
        for site_id in &nexusphp_sites {
            let def = definitions
                .get(*site_id)
                .unwrap_or_else(|| panic!("Missing site: {}", site_id));
            assert_eq!(def.site.adapter, "nexusphp", "{} should use nexusphp adapter", site_id);
            assert!(
                def.user_info.is_some(),
                "{} should have user_info selectors",
                site_id
            );
        }
    }

    #[test]
    fn test_site_definition_fields() {
        let definitions = load_builtin_definitions();

        // Verify HDSky has correct fields
        let hdsky = definitions.get("hdsky").expect("hdsky should exist");
        assert_eq!(hdsky.site.name, "HDSky");
        assert_eq!(hdsky.site.url, "https://hdsky.me");
        assert_eq!(hdsky.site.adapter, "nexusphp");
        assert!(hdsky.site.api_url.is_some());

        // Verify M-Team uses mteam adapter
        let mteam = definitions.get("mteam").expect("mteam should exist");
        assert_eq!(mteam.site.adapter, "mteam");
        assert!(mteam.user_info.is_none());

        // Verify Gazelle sites
        let redacted = definitions.get("redacted").expect("redacted should exist");
        assert_eq!(redacted.site.adapter, "gazelle");
        assert_eq!(redacted.site.url, "https://redacted.sh");

        // Verify Unit3D sites
        let aither = definitions.get("aither").expect("aither should exist");
        assert_eq!(aither.site.adapter, "unit3d");

        // Verify Zhuque
        let zhuque = definitions.get("zhuque").expect("zhuque should exist");
        assert_eq!(zhuque.site.adapter, "zhuque");
    }

    #[test]
    fn test_load_all_definitions_without_user_dir() {
        let definitions = load_all_definitions(None);
        assert!(
            definitions.len() >= 30,
            "Expected at least 30 definitions, got {}",
            definitions.len()
        );
    }

    #[test]
    fn all_builtin_definitions_have_required_fields() {
        let defs = load_builtin_definitions();
        for (key, def) in &defs {
            assert!(!def.site.id.is_empty(), "site {} has empty id", key);
            assert!(!def.site.name.is_empty(), "site {} has empty name", key);
            assert!(!def.site.url.is_empty(), "site {} has empty url", key);
            assert!(!def.site.adapter.is_empty(), "site {} has empty adapter", key);
        }
    }

    #[test]
    fn load_user_definitions_returns_empty_for_nonexistent_dir() {
        let defs = load_user_definitions(std::path::Path::new("/tmp/nonexistent-pt-reseeder-test-dir-xyz"));
        assert!(defs.is_empty());
    }

    #[test]
    fn load_user_definitions_reads_toml_files_from_sites_subdir() {
        let tmp = tempfile::tempdir().unwrap();
        let sites_dir = tmp.path().join("sites");
        std::fs::create_dir_all(&sites_dir).unwrap();

        let toml_content = r#"
[site]
id = "custom"
name = "CustomSite"
url = "https://custom.example.com"
adapter = "nexusphp"
"#;
        std::fs::write(sites_dir.join("custom.toml"), toml_content).unwrap();

        let defs = load_user_definitions(tmp.path());
        assert_eq!(defs.len(), 1);
        assert!(defs.contains_key("custom"));
        assert_eq!(defs["custom"].site.name, "CustomSite");
    }

    #[test]
    fn load_user_definitions_ignores_non_toml_files() {
        let tmp = tempfile::tempdir().unwrap();
        let sites_dir = tmp.path().join("sites");
        std::fs::create_dir_all(&sites_dir).unwrap();
        std::fs::write(sites_dir.join("readme.txt"), "not a toml file").unwrap();

        let defs = load_user_definitions(tmp.path());
        assert!(defs.is_empty());
    }

    #[test]
    fn load_user_definitions_skips_invalid_toml() {
        let tmp = tempfile::tempdir().unwrap();
        let sites_dir = tmp.path().join("sites");
        std::fs::create_dir_all(&sites_dir).unwrap();
        std::fs::write(sites_dir.join("bad.toml"), "this is { not valid toml").unwrap();

        let defs = load_user_definitions(tmp.path());
        assert!(defs.is_empty());
    }

    #[test]
    fn load_all_definitions_merges_builtin_and_user() {
        let tmp = tempfile::tempdir().unwrap();
        let sites_dir = tmp.path().join("sites");
        std::fs::create_dir_all(&sites_dir).unwrap();

        let toml_content = r#"
[site]
id = "usersite"
name = "UserOnly"
url = "https://useronly.example.com"
adapter = "unit3d"
"#;
        std::fs::write(sites_dir.join("usersite.toml"), toml_content).unwrap();

        let defs = load_all_definitions(Some(tmp.path()));
        assert!(defs.contains_key("hdsky"));
        assert!(defs.contains_key("usersite"));
        assert_eq!(defs["usersite"].site.name, "UserOnly");
    }

    #[test]
    fn load_all_definitions_user_overrides_builtin() {
        let tmp = tempfile::tempdir().unwrap();
        let sites_dir = tmp.path().join("sites");
        std::fs::create_dir_all(&sites_dir).unwrap();

        let toml_content = r#"
[site]
id = "hdsky"
name = "HDSky-Custom"
url = "https://custom-hdsky.example.com"
adapter = "nexusphp"
"#;
        std::fs::write(sites_dir.join("hdsky.toml"), toml_content).unwrap();

        let defs = load_all_definitions(Some(tmp.path()));
        assert_eq!(defs["hdsky"].site.name, "HDSky-Custom");
    }
}
