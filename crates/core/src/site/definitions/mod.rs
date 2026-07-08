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
