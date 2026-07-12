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

// format_subtitle regexes (ref: auto_feed.js deal_with_subtitle)
static RE_CHECKED_BY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\[checked by.*?\]").unwrap());
static RE_AUTOUP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\bautoup\b").unwrap());

// translate_bbcode regexes
static RE_QUOTE_ATTR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\[quote=[^\]]*\]").unwrap());
static RE_IMG_SIZED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\[img=[^\]]*\]").unwrap());
static RE_MEDIAINFO: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\[mediainfo\]").unwrap());
static RE_MEDIAINFO_CLOSE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\[/mediainfo\]").unwrap());
static RE_HIDE_OPEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\[hide(?:=[^\]]*)?]").unwrap());
static RE_HIDE_CLOSE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\[/hide\]").unwrap());
static RE_EXCESS_NEWLINES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\n{4,}").unwrap());

// clean_description regexes (ref: auto_feed.js fill_raw_info)
static RE_EMPTY_QUOTE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\[quote\]\s*\[/quote\]").unwrap());
static RE_EMPTY_BOLD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\[b\]\s*\[/b\]").unwrap());
static RE_CONSECUTIVE_NEWLINES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\n{3,}").unwrap());

// extract_url regexes (ref: auto_feed.js fill_raw_info)
static RE_IMDB_URL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"https?://(?:www\.)?imdb\.com/title/(tt\d+)").unwrap());
static RE_DOUBAN_URL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"https?://(?:movie\.)?douban\.com/subject/(\d+)").unwrap());

// infer_from_title regexes (ref: auto_feed.js String.prototype.medium_sel etc.)
static RE_MEDIUM_WEBDL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(Web-?dl|WEB[. ])").unwrap());
static RE_MEDIUM_WEBRIP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)webrip").unwrap());
static RE_MEDIUM_REMUX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bremux\b").unwrap());
static RE_MEDIUM_BLURAY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(Blu-?ray|BDISO|BDMV)").unwrap());
static RE_MEDIUM_UHD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(UHD|UltraHD)\b").unwrap());
static RE_MEDIUM_HDTV: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bHDTV\b").unwrap());
static RE_MEDIUM_ENCODE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(Encode|BDRip)\b").unwrap());
static RE_MEDIUM_DVD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(DVDRip|DVDISO|DVD)\b").unwrap());

static RE_CODEC_H264: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(H\.?264|x\.?264|AVC)\b").unwrap());
static RE_CODEC_H265: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(H\.?265|x\.?265|HEVC)\b").unwrap());
static RE_CODEC_VC1: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bVC-?1\b").unwrap());
static RE_CODEC_MPEG2: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bMPEG-?2\b").unwrap());
static RE_CODEC_AV1: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bAV1\b").unwrap());

static RE_RES_2160P: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b2160p\b").unwrap());
static RE_RES_1080P: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b1080p\b").unwrap());
static RE_RES_1080I: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b1080i\b").unwrap());
static RE_RES_720P: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b720p\b").unwrap());
static RE_RES_SD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(480p|576p)\b").unwrap());

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
///
/// Ref: auto_feed.js `fill_raw_info` + target-site form filling logic.
/// The source site extractor provides `raw` with whatever it can scrape from the detail page.
/// This function:
///   1. Formats the title (whitespace only — no watermark stripping, titles come from detail pages)
///   2. Cleans the subtitle (removes review markers like `[checked by ...]`)
///   3. Cleans and translates description BBCode for the target site
///   4. Fills in missing fields (medium/codec/resolution) by inference from the title
///   5. Fills in missing IMDB/Douban URLs by extraction from the description
///   6. Maps category/source/codec/resolution to target-site numeric IDs
pub fn adapt_torrent_info(
    raw: &RawTorrentInfo,
    target_site: &str,
    mapping: Option<&AdapterMapping>,
) -> Result<AdaptedTorrentInfo, CoreError> {
    let name = format_title(&raw.name);
    let small_descr = format_subtitle(&raw.small_descr);
    let cleaned_descr = clean_description(&raw.descr);
    let translated_descr = translate_bbcode(&cleaned_descr, target_site);
    let descr = mapping
        .and_then(|m| m.description_template.as_deref())
        .map(|template| apply_description_template(template, raw, &translated_descr))
        .unwrap_or(translated_descr);

    // Infer missing fields from the title (ref: auto_feed.js fill_raw_info)
    let medium = if raw.medium.trim().is_empty() {
        infer_medium(&raw.name)
    } else {
        raw.medium.clone()
    };
    let video_codec = if raw.video_codec.trim().is_empty() {
        infer_codec(&raw.name)
    } else {
        raw.video_codec.clone()
    };
    let resolution = if raw.resolution.trim().is_empty() {
        infer_resolution(&raw.name)
    } else {
        raw.resolution.clone()
    };

    // Fill missing IMDB/Douban URLs from description (ref: auto_feed.js fill_raw_info)
    let imdb_url = raw
        .imdb_url
        .clone()
        .or_else(|| extract_imdb_url(&raw.descr));
    let douban_url = raw
        .douban_url
        .clone()
        .or_else(|| extract_douban_url(&raw.descr));

    let (category_id, source_id, codec_id, resolution_id) = if let Some(m) = mapping {
        let cat = find_mapping(&raw.torrent_type, &m.categories, |c| {
            (&c.torrent_type, &c.aliases)
        })
        .map(|c| c.category_id)
        .or_else(|| default_category_id(&raw.torrent_type));
        let src = find_mapping(&medium, &m.sources, |s| (&s.medium, &s.aliases))
            .map(|s| s.source_id)
            .or_else(|| default_source_id(&medium));
        let codec = find_mapping(&video_codec, &m.codecs, |c| (&c.codec, &c.aliases))
            .map(|c| c.codec_id)
            .or_else(|| default_codec_id(&video_codec));
        let res = find_mapping(&resolution, &m.resolutions, |r| {
            (&r.resolution, &r.aliases)
        })
        .map(|r| r.resolution_id)
        .or_else(|| default_resolution_id(&resolution));

        require_mapping("category", &raw.torrent_type, cat)?;
        (cat, src, codec, res)
    } else {
        let cat = default_category_id(&raw.torrent_type);
        require_mapping("category", &raw.torrent_type, cat)?;
        (
            cat,
            default_source_id(&medium),
            default_codec_id(&video_codec),
            default_resolution_id(&resolution),
        )
    };

    debug!(
        target_site = target_site,
        category_id = ?category_id,
        codec_id = ?codec_id,
        resolution_id = ?resolution_id,
        source_id = ?source_id,
        inferred_medium = %medium,
        inferred_codec = %video_codec,
        inferred_resolution = %resolution,
        "adapted torrent info"
    );

    Ok(AdaptedTorrentInfo {
        name,
        small_descr,
        descr,
        imdb_url,
        douban_url,
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
        "2" | "uhd" | "ultrahd" => Some(2),
        "3" | "remux" => Some(3),
        "4" | "hdtv" => Some(4),
        "5" | "web-dl" | "webdl" | "web" | "webrip" => Some(5),
        "6" | "dvd" | "dvdrip" | "dvdiso" => Some(6),
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
        "5" | "av1" => Some(5),
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
        "5" | "sd" | "480p" | "576p" => Some(5),
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

// ---------------------------------------------------------------------------
// Title / subtitle formatting
// ---------------------------------------------------------------------------

/// Format the torrent title: only normalise whitespace.
///
/// Unlike auto_feed.js `deal_with_title` which reconstructs titles from file-names (replacing
/// dots, fixing audio channel numbers, etc.), our input is the human-readable title scraped from
/// the detail page — it does NOT contain file-name artefacts or site UI watermarks, so we only
/// collapse extra spaces and trim.
fn format_title(title: &str) -> String {
    let result = title.trim();
    let result = RE_MULTI_SPACES.replace_all(result, " ");
    result.trim().to_string()
}

/// Clean the subtitle by removing review/audit markers.
///
/// Ref: auto_feed.js `deal_with_subtitle`:
///   subtitle.replace(/\[checked by.*?\]/i, '').replace(/autoup/i, '').trim()
fn format_subtitle(subtitle: &str) -> String {
    let result = RE_CHECKED_BY.replace_all(subtitle, "");
    let result = RE_AUTOUP.replace_all(&result, "");
    RE_MULTI_SPACES
        .replace_all(result.trim(), " ")
        .trim()
        .to_string()
}

// ---------------------------------------------------------------------------
// Description cleaning
// ---------------------------------------------------------------------------

/// Clean raw description before BBCode translation.
///
/// Ref: auto_feed.js `fill_raw_info`:
///   - decode URL-encoded colons/slashes (%3A → :, %2F → /)
///   - remove empty [quote][/quote] and [b][/b]
///   - collapse consecutive blank lines
fn clean_description(descr: &str) -> String {
    let mut result = descr.replace("%3A", ":").replace("%2F", "/");
    result = RE_EMPTY_QUOTE.replace_all(&result, "").into_owned();
    result = RE_EMPTY_BOLD.replace_all(&result, "").into_owned();
    result = RE_CONSECUTIVE_NEWLINES
        .replace_all(&result, "\n\n")
        .into_owned();
    result
}

/// Translate BBCode dialect differences between sites.
fn translate_bbcode(content: &str, target_site: &str) -> String {
    let mut result = content.to_string();
    result = RE_QUOTE_ATTR
        .replace_all(&result, "[quote]")
        .into_owned();
    result = RE_IMG_SIZED.replace_all(&result, "[img]").into_owned();

    let target_lower = target_site.to_ascii_lowercase();
    if SITES_NO_MEDIAINFO
        .iter()
        .any(|&s| target_lower.contains(s))
    {
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
// Field inference from title (ref: auto_feed.js fill_raw_info + prototype methods)
// ---------------------------------------------------------------------------

/// Infer medium/source from title when the extractor didn't provide one.
///
/// Ref: auto_feed.js `String.prototype.medium_sel`
fn infer_medium(title: &str) -> String {
    if RE_MEDIUM_WEBDL.is_match(title) && !RE_MEDIUM_WEBRIP.is_match(title) {
        "WEB-DL".to_string()
    } else if RE_MEDIUM_WEBRIP.is_match(title) {
        "WEB-DL".to_string()
    } else if RE_MEDIUM_REMUX.is_match(title) {
        "Remux".to_string()
    } else if RE_MEDIUM_UHD.is_match(title) {
        "UHD".to_string()
    } else if RE_MEDIUM_BLURAY.is_match(title) {
        "Blu-ray".to_string()
    } else if RE_MEDIUM_HDTV.is_match(title) {
        "HDTV".to_string()
    } else if RE_MEDIUM_ENCODE.is_match(title) {
        "Encode".to_string()
    } else if RE_MEDIUM_DVD.is_match(title) {
        "DVD".to_string()
    } else {
        String::new()
    }
}

/// Infer video codec from title when the extractor didn't provide one.
///
/// Ref: auto_feed.js `String.prototype.codec_sel`
fn infer_codec(title: &str) -> String {
    if RE_CODEC_H264.is_match(title) {
        "H.264".to_string()
    } else if RE_CODEC_H265.is_match(title) {
        "H.265".to_string()
    } else if RE_CODEC_AV1.is_match(title) {
        "AV1".to_string()
    } else if RE_CODEC_VC1.is_match(title) {
        "VC-1".to_string()
    } else if RE_CODEC_MPEG2.is_match(title) {
        "MPEG-2".to_string()
    } else {
        String::new()
    }
}

/// Infer resolution from title when the extractor didn't provide one.
///
/// Ref: auto_feed.js `String.prototype.standard_sel`
fn infer_resolution(title: &str) -> String {
    if RE_RES_2160P.is_match(title) {
        "4K".to_string()
    } else if RE_RES_1080P.is_match(title) {
        "1080p".to_string()
    } else if RE_RES_1080I.is_match(title) {
        "1080i".to_string()
    } else if RE_RES_720P.is_match(title) {
        "720p".to_string()
    } else if RE_RES_SD.is_match(title) {
        "SD".to_string()
    } else {
        String::new()
    }
}

// ---------------------------------------------------------------------------
// URL extraction from description
// ---------------------------------------------------------------------------

/// Extract IMDB URL from description text.
///
/// Ref: auto_feed.js `fill_raw_info`:
///   var url = raw_info.descr.match(/http(s*):\/\/www.imdb.com\/title\/tt(\d+)/i);
fn extract_imdb_url(descr: &str) -> Option<String> {
    RE_IMDB_URL
        .find(descr)
        .map(|m| m.as_str().to_string())
}

/// Extract Douban URL from description text.
///
/// Ref: auto_feed.js `fill_raw_info`:
///   var dburl = raw_info.descr.match(/http(s*):\/\/.*?douban.com\/subject\/(\d+)/i);
fn extract_douban_url(descr: &str) -> Option<String> {
    RE_DOUBAN_URL
        .find(descr)
        .map(|m| m.as_str().to_string())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    // -- format_title --

    #[test]
    fn title_trims_whitespace() {
        assert_eq!(format_title("  hello  "), "hello");
    }

    #[test]
    fn title_collapses_multiple_spaces() {
        assert_eq!(
            format_title("Movie  Name   2024  1080p"),
            "Movie Name 2024 1080p"
        );
    }

    #[test]
    fn title_preserves_group_tags() {
        // Titles from detail pages legitimately end with release group names —
        // these must NOT be stripped (the old watermark logic would have removed them).
        let title = "Movie.2024.1080p.BluRay.x264-GROUP";
        assert_eq!(format_title(title), title);
    }

    #[test]
    fn title_preserves_at_suffix() {
        // "@HDSky" in a detail-page title is part of the release, not a watermark.
        let title = "Movie.2024.1080p.BluRay @HDSky";
        assert_eq!(format_title(title), title);
    }

    // -- format_subtitle --

    #[test]
    fn subtitle_removes_checked_by() {
        assert_eq!(
            format_subtitle("好片推荐 [Checked by admin]"),
            "好片推荐"
        );
    }

    #[test]
    fn subtitle_removes_autoup() {
        assert_eq!(format_subtitle("autoup 好片推荐"), "好片推荐");
    }

    #[test]
    fn subtitle_preserves_normal_content() {
        assert_eq!(format_subtitle("这是副标题"), "这是副标题");
    }

    // -- clean_description --

    #[test]
    fn description_decodes_url_encoding() {
        assert_eq!(
            clean_description("https%3A%2F%2Fexample.com"),
            "https://example.com"
        );
    }

    #[test]
    fn description_removes_empty_quote() {
        assert_eq!(clean_description("before [quote][/quote] after"), "before  after");
    }

    #[test]
    fn description_removes_empty_bold() {
        assert_eq!(clean_description("before [b][/b] after"), "before  after");
    }

    #[test]
    fn description_collapses_newlines() {
        let input = "Line1\n\n\n\n\nLine2";
        assert_eq!(clean_description(input), "Line1\n\nLine2");
    }

    // -- translate_bbcode (unchanged behaviour) --

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

    // -- infer_medium --

    #[test]
    fn infer_medium_webdl() {
        assert_eq!(infer_medium("Movie.2024.1080p.WEB-DL.x264"), "WEB-DL");
    }

    #[test]
    fn infer_medium_remux() {
        assert_eq!(infer_medium("Movie.2024.1080p.Remux.AVC"), "Remux");
    }

    #[test]
    fn infer_medium_bluray() {
        assert_eq!(infer_medium("Movie.2024.1080p.Blu-ray.AVC"), "Blu-ray");
    }

    #[test]
    fn infer_medium_uhd() {
        assert_eq!(infer_medium("Movie.2024.2160p.UHD.HEVC"), "UHD");
    }

    #[test]
    fn infer_medium_empty_when_unknown() {
        assert_eq!(infer_medium("Movie 2024"), "");
    }

    // -- infer_codec --

    #[test]
    fn infer_codec_h264() {
        assert_eq!(infer_codec("Movie.2024.1080p.x264"), "H.264");
    }

    #[test]
    fn infer_codec_h265() {
        assert_eq!(infer_codec("Movie.2024.2160p.HEVC"), "H.265");
    }

    #[test]
    fn infer_codec_av1() {
        assert_eq!(infer_codec("Movie.2024.2160p.AV1"), "AV1");
    }

    #[test]
    fn infer_codec_empty_when_unknown() {
        assert_eq!(infer_codec("Movie 2024"), "");
    }

    // -- infer_resolution --

    #[test]
    fn infer_resolution_4k() {
        assert_eq!(infer_resolution("Movie.2024.2160p.UHD"), "4K");
    }

    #[test]
    fn infer_resolution_1080p() {
        assert_eq!(infer_resolution("Movie.2024.1080p.BluRay"), "1080p");
    }

    #[test]
    fn infer_resolution_720p() {
        assert_eq!(infer_resolution("Movie.2024.720p.WEB-DL"), "720p");
    }

    #[test]
    fn infer_resolution_empty_when_unknown() {
        assert_eq!(infer_resolution("Movie 2024"), "");
    }

    // -- extract URLs --

    #[test]
    fn extract_imdb_from_descr() {
        let descr = "Some text https://www.imdb.com/title/tt1234567/ more text";
        assert_eq!(
            extract_imdb_url(descr),
            Some("https://www.imdb.com/title/tt1234567".to_string())
        );
    }

    #[test]
    fn extract_imdb_none_when_missing() {
        assert_eq!(extract_imdb_url("no url here"), None);
    }

    #[test]
    fn extract_douban_from_descr() {
        let descr = "Some text https://movie.douban.com/subject/12345678/ more";
        assert_eq!(
            extract_douban_url(descr),
            Some("https://movie.douban.com/subject/12345678".to_string())
        );
    }

    #[test]
    fn extract_douban_none_when_missing() {
        assert_eq!(extract_douban_url("no url here"), None);
    }
}
