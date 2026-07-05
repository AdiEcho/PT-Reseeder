use std::sync::LazyLock;

use regex_lite::Regex;
use tracing::debug;

use crate::error::{CoreError, RepostError};
use crate::site::models::{AdaptedTorrentInfo, RawTorrentInfo};

use super::models::AdapterMapping;

// ---------------------------------------------------------------------------
// Static regex cache (compiled once via LazyLock)
// ---------------------------------------------------------------------------

// format_title regexes
static RE_MULTI_SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r" {2,}").unwrap());
static RE_SITE_WATERMARK_AT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\s*@(HDSky|MTeam|TTG|CHDBits|OurBits|PTerClub|HDHome|HDArea|Audiences|PuTao|TJUPT|NPUPT|HDTime|HDU|OpenCD|SSD|PTSBAO|PTHome|SpringSunDay|CMCT|KeepFrds|FRDS|TLFBits|HDDolby|Piggo|BeiTai|GPW|FileList|TorrentLeech|GreatPosterWall|BroadcasTheNet|Empornium|GazelleGames|AnimeBytes|Redacted|Orpheus|BHD|Blutopia|Aither|FearNoPeer|Anthelion|ReelFliX|AGSVPT|CarPT|UBits|LemonHD|DICMusic|HaresClub|PTChina|RedLeaves|BTSCHOOL|Azusa|Hhanclub)$").unwrap()
});
static RE_SITE_WATERMARK_DASH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\s+-\s*(HDSky|MTeam|TTG|CHDBits|OurBits|PTerClub|HDHome|HDArea|Audiences|PuTao|TJUPT|NPUPT|HDTime|HDU|OpenCD|SSD|PTSBAO|PTHome|SpringSunDay|CMCT|KeepFrds|FRDS|TLFBits|HDDolby|Piggo|BeiTai|GPW|FileList|TorrentLeech|GreatPosterWall|BroadcasTheNet|Empornium|GazelleGames|AnimeBytes|Redacted|Orpheus|BHD|Blutopia|Aither|FearNoPeer|Anthelion|ReelFliX|AGSVPT|CarPT|UBits|LemonHD|DICMusic|HaresClub|PTChina|RedLeaves|BTSCHOOL|Azusa|Hhanclub)$").unwrap()
});

// translate_bbcode regexes
static RE_QUOTE_ATTR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\[quote=[^\]]*\]").unwrap());
static RE_IMG_SIZED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\[img=[^\]]*\]").unwrap());
static RE_MEDIAINFO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\[mediainfo\]").unwrap());
static RE_MEDIAINFO_CLOSE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\[/mediainfo\]").unwrap());
static RE_HIDE_OPEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\[hide(?:=[^\]]*)?\]").unwrap());
static RE_HIDE_CLOSE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\[/hide\]").unwrap());
static RE_EXCESS_NEWLINES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n{4,}").unwrap());

// ---------------------------------------------------------------------------
// Sites known to NOT support [mediainfo] tags (use [code] instead)
// ---------------------------------------------------------------------------
const SITES_NO_MEDIAINFO: &[&str] = &[
    "chdbits",
    "ourbits",
    "hdarea",
    "audiences",
    "hdtime",
    "ssd",
    "pthome",
    "ptsbao",
    "springsunday",
    "cmct",
    "tlfbits",
    "piggo",
    "beitai",
    "filelist",
    "torrentleech",
    "btschool",
];

/// Adapt a RawTorrentInfo from the source site into an AdaptedTorrentInfo for the target site.
pub fn adapt_torrent_info(
    raw: &RawTorrentInfo,
    target_site: &str,
    mapping: Option<&AdapterMapping>,
) -> Result<AdaptedTorrentInfo, CoreError> {
    let name = format_title(&raw.name, target_site);
    let translated_descr = translate_bbcode(&raw.descr, target_site);
    let descr = mapping
        .and_then(|m| m.description_template.as_deref())
        .map(|template| apply_description_template(template, raw, &translated_descr))
        .unwrap_or(translated_descr);

    let (category_id, source_id, codec_id, resolution_id) = if let Some(m) = mapping {
        let cat = find_mapping(&raw.torrent_type, &m.categories, |c| {
            (&c.torrent_type, &c.aliases)
        })
        .map(|c| c.category_id)
        .or_else(|| default_category_id(&raw.torrent_type));
        let src = find_mapping(&raw.medium, &m.sources, |s| (&s.medium, &s.aliases))
            .map(|s| s.source_id)
            .or_else(|| default_source_id(&raw.medium));
        let codec = find_mapping(&raw.video_codec, &m.codecs, |c| (&c.codec, &c.aliases))
            .map(|c| c.codec_id)
            .or_else(|| default_codec_id(&raw.video_codec));
        let res = find_mapping(&raw.resolution, &m.resolutions, |r| {
            (&r.resolution, &r.aliases)
        })
        .map(|r| r.resolution_id)
        .or_else(|| default_resolution_id(&raw.resolution));

        require_mapping("category", &raw.torrent_type, cat)?;
        (cat, src, codec, res)
    } else {
        let cat = default_category_id(&raw.torrent_type);
        require_mapping("category", &raw.torrent_type, cat)?;
        (
            cat,
            default_source_id(&raw.medium),
            default_codec_id(&raw.video_codec),
            default_resolution_id(&raw.resolution),
        )
    };

    debug!(
        target_site = target_site,
        category_id = ?category_id,
        codec_id = ?codec_id,
        resolution_id = ?resolution_id,
        source_id = ?source_id,
        "adapted torrent info"
    );

    Ok(AdaptedTorrentInfo {
        name,
        small_descr: raw.small_descr.clone(),
        descr,
        imdb_url: raw.imdb_url.clone(),
        douban_url: raw.douban_url.clone(),
        mediainfo: raw.mediainfo.clone(),
        images: raw.images.clone(),
        category_id,
        source_id,
        codec_id,
        resolution_id,
        torrent_file_data: raw.torrent_file_data.clone(),
        target_site: target_site.to_string(),
    })
}

fn find_mapping<'a, T, F>(value: &str, entries: &'a [T], fields: F) -> Option<&'a T>
where
    F: Fn(&'a T) -> (&'a String, &'a Vec<String>),
{
    entries.iter().find(|entry| {
        let (primary, aliases) = fields(entry);
        primary.eq_ignore_ascii_case(value)
            || aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(value))
    })
}

fn require_mapping(kind: &str, value: &str, id: Option<i64>) -> Result<(), CoreError> {
    if id.is_none() && !value.trim().is_empty() {
        return Err(CoreError::Repost(RepostError::AdaptationFailed(format!(
            "missing {kind} mapping for '{value}'"
        ))));
    }
    Ok(())
}

fn default_category_id(value: &str) -> Option<i64> {
    let lower = value.to_ascii_lowercase();
    match lower.as_str() {
        "401" | "movie" | "movies" | "电影" | "影片" => Some(401),
        "402" | "tv" | "series" | "剧集" | "电视剧" => Some(402),
        "403" | "music" | "音乐" => Some(403),
        "404" | "documentary" | "纪录" | "纪录片" => Some(404),
        "405" | "anime" | "animation" | "动画" | "动漫" => Some(405),
        "406" | "variety" | "show" | "综艺" => Some(406),
        "407" | "sports" | "体育" => Some(407),
        "408" | "software" | "软件" => Some(408),
        "409" | "game" | "games" | "游戏" => Some(409),
        "410" | "ebook" | "book" | "电子书" | "书籍" => Some(410),
        _ => value.parse::<i64>().ok(),
    }
}

fn default_source_id(value: &str) -> Option<i64> {
    let lower = value.to_ascii_lowercase().replace(['_', ' '], "-");
    match lower.as_str() {
        "1" | "blu-ray" | "bluray" | "bdrip" | "bdiso" => Some(1),
        "3" | "remux" => Some(3),
        "4" | "hdtv" => Some(4),
        "5" | "web-dl" | "webdl" | "web" => Some(5),
        "7" | "encode" => Some(7),
        _ => value.parse::<i64>().ok(),
    }
}

fn default_codec_id(value: &str) -> Option<i64> {
    let lower = value.to_ascii_lowercase();
    match lower.as_str() {
        "1" | "h.264" | "h264" | "x264" | "avc" => Some(1),
        "2" | "h.265" | "h265" | "x265" | "hevc" => Some(2),
        "3" | "mpeg-2" | "mpeg2" => Some(3),
        "4" | "vc-1" | "vc1" => Some(4),
        _ => value.parse::<i64>().ok(),
    }
}

fn default_resolution_id(value: &str) -> Option<i64> {
    let lower = value.to_ascii_lowercase();
    match lower.as_str() {
        "1" | "4k" | "2160p" | "uhd" => Some(1),
        "2" | "1080p" => Some(2),
        "3" | "1080i" => Some(3),
        "4" | "720p" => Some(4),
        "5" | "sd" | "480p" => Some(5),
        _ => value.parse::<i64>().ok(),
    }
}

fn apply_description_template(template: &str, raw: &RawTorrentInfo, descr: &str) -> String {
    template
        .replace("{descr}", descr)
        .replace("{description}", descr)
        .replace("{source_site}", &raw.source_site)
        .replace("{source_url}", &raw.source_url)
        .replace("{name}", &raw.name)
        .replace("{small_descr}", &raw.small_descr)
        .replace("{imdb_url}", raw.imdb_url.as_deref().unwrap_or(""))
        .replace("{douban_url}", raw.douban_url.as_deref().unwrap_or(""))
        .replace("{mediainfo}", raw.mediainfo.as_deref().unwrap_or(""))
}

/// Format/clean the torrent title for the target site conventions.
fn format_title(title: &str, _target_site: &str) -> String {
    let mut result = title.trim().to_string();
    result = RE_MULTI_SPACES.replace_all(&result, " ").into_owned();
    result = RE_SITE_WATERMARK_AT.replace(&result, "").into_owned();
    result = RE_SITE_WATERMARK_DASH.replace(&result, "").into_owned();
    result.trim().to_string()
}

/// Translate BBCode dialect differences between sites.
fn translate_bbcode(content: &str, target_site: &str) -> String {
    let mut result = content.to_string();
    result = RE_QUOTE_ATTR.replace_all(&result, "[quote]").into_owned();
    result = RE_IMG_SIZED.replace_all(&result, "[img]").into_owned();

    let target_lower = target_site.to_ascii_lowercase();
    if SITES_NO_MEDIAINFO.iter().any(|&s| target_lower.contains(s)) {
        result = RE_MEDIAINFO.replace_all(&result, "[code]").into_owned();
        result = RE_MEDIAINFO_CLOSE
            .replace_all(&result, "[/code]")
            .into_owned();
    }

    result = RE_HIDE_OPEN.replace_all(&result, "").into_owned();
    result = RE_HIDE_CLOSE.replace_all(&result, "").into_owned();
    result = RE_EXCESS_NEWLINES
        .replace_all(&result, "\n\n\n")
        .into_owned();

    result
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_trims_whitespace() {
        assert_eq!(format_title("  hello  ", "mteam"), "hello");
    }

    #[test]
    fn title_collapses_multiple_spaces() {
        assert_eq!(
            format_title("Movie  Name   2024  1080p", "mteam"),
            "Movie Name 2024 1080p"
        );
    }

    #[test]
    fn title_removes_at_watermark() {
        assert_eq!(
            format_title("Movie.2024.1080p.BluRay @HDSky", "ourbits"),
            "Movie.2024.1080p.BluRay"
        );
    }

    #[test]
    fn title_removes_dash_watermark() {
        assert_eq!(
            format_title("Movie.2024.1080p.BluRay - TTG", "ourbits"),
            "Movie.2024.1080p.BluRay"
        );
    }

    #[test]
    fn title_preserves_legitimate_tags() {
        let title = "Movie Name [1080p] [HEVC] [DTS-HD]";
        assert_eq!(format_title(title, "mteam"), title);
    }

    #[test]
    fn title_no_false_positive_on_unknown_site() {
        let title = "Movie.2024.1080p @RandomGroup";
        assert_eq!(format_title(title, "mteam"), title);
    }

    #[test]
    fn bbcode_quote_attr_stripped() {
        let input = "[quote=someuser]Hello[/quote]";
        let output = translate_bbcode(input, "mteam");
        assert_eq!(output, "[quote]Hello[/quote]");
    }

    #[test]
    fn bbcode_img_size_stripped() {
        let input = "[img=800,600]https://example.com/pic.jpg[/img]";
        let output = translate_bbcode(input, "mteam");
        assert_eq!(output, "[img]https://example.com/pic.jpg[/img]");
    }

    #[test]
    fn bbcode_url_unchanged() {
        let input = "[url=https://example.com]Click here[/url]";
        let output = translate_bbcode(input, "mteam");
        assert_eq!(output, input);
    }

    #[test]
    fn bbcode_mediainfo_kept_on_supporting_site() {
        let input = "[mediainfo]codec info here[/mediainfo]";
        let output = translate_bbcode(input, "mteam");
        assert_eq!(output, "[mediainfo]codec info here[/mediainfo]");
    }

    #[test]
    fn bbcode_mediainfo_converted_on_unsupported_site() {
        let input = "[mediainfo]codec info here[/mediainfo]";
        let output = translate_bbcode(input, "chdbits");
        assert_eq!(output, "[code]codec info here[/code]");
    }

    #[test]
    fn bbcode_hide_tags_removed_content_kept() {
        let input = "Before [hide]secret content[/hide] After";
        let output = translate_bbcode(input, "mteam");
        assert_eq!(output, "Before secret content After");
    }

    #[test]
    fn bbcode_excess_newlines_collapsed() {
        let input = "Line1\n\n\n\n\n\nLine2";
        let output = translate_bbcode(input, "mteam");
        assert_eq!(output, "Line1\n\n\nLine2");
    }
}
