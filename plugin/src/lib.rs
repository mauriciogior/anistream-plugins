// SPDX-License-Identifier: GPL-3.0-or-later
//! ani-cli protocol adapted from pystardust/ani-cli 5.1.6 (0301d67).
//! Network access is supplied and restricted by Anistream's plugin host.
#[allow(warnings)]
mod bindings {
    wit_bindgen::generate!({path: "wit/anistream-provider.wit", world: "plugin"});
}
use base64::{Engine as _, engine::general_purpose::STANDARD};
use bindings::{
    anistream::provider::host,
    exports::anistream::provider::provider::{
        Episode, Guest, Manifest, MediaStream, ProviderError, SearchHit, SourceCandidate, Subtitle,
    },
};
fn captures(pattern: &str, input: &str) -> Vec<Vec<String>> {
    #[cfg(target_arch = "wasm32")]
    {
        host::regex_captures(pattern, input)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        regex::Regex::new(pattern)
            .unwrap()
            .captures_iter(input)
            .map(|c| {
                c.iter()
                    .map(|m| m.map_or("", |m| m.as_str()).to_owned())
                    .collect()
            })
            .collect()
    }
}
use serde_json::Value;
use url::Url;
const BASE: &str = "https://hianime.at/";
const HOSTS: &[&str] = &["hianime.at", "zokoanime.video"];
struct Component;
fn parse_error(message: impl Into<String>) -> ProviderError {
    ProviderError::Parse(message.into())
}
fn web_url(raw: &str) -> Result<Url, ProviderError> {
    let url = Url::parse(raw).map_err(|e| parse_error(e.to_string()))?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(parse_error("invalid web URL"));
    }
    Ok(url)
}
fn episode_order(a: &str, b: &str) -> std::cmp::Ordering {
    match (a.parse::<f64>(), b.parse::<f64>()) {
        (Ok(a), Ok(b)) => a.total_cmp(&b),
        _ => a.cmp(b),
    }
}
fn translation(value: &str) -> Result<(), ProviderError> {
    match value {
        "sub" | "dub" => Ok(()),
        _ => Err(ProviderError::NotFound),
    }
}
fn response_text(status: u16, bytes: Vec<u8>) -> Result<String, ProviderError> {
    if status == 404 {
        return Err(ProviderError::NotFound);
    }
    if !(200..300).contains(&status) {
        return Err(ProviderError::Blocked(format!("HTTP {status}")));
    }
    let body = String::from_utf8(bytes).map_err(|_| parse_error("response was not UTF-8"))?;
    if body.to_ascii_lowercase().contains("<title>just a moment") {
        return Err(ProviderError::Blocked("browser challenge required".into()));
    }
    Ok(body)
}
fn fetch(url: &Url) -> Result<String, ProviderError> {
    // The host checks this again. Check locally too so external embeds never widen permissions.
    if !HOSTS.iter().any(|host| url.host_str() == Some(*host)) {
        return Err(ProviderError::Blocked("undeclared embed host".into()));
    }
    let response = host::fetch(&host::HttpRequest {
        method: "GET".into(),
        url: url.to_string(),
        headers: vec![],
        body: None,
    })
    .map_err(|error| match error {
        host::HostError::Denied(_) => ProviderError::Blocked("host denied request".into()),
        host::HostError::Timeout => ProviderError::Other("request timed out".into()),
        host::HostError::Transport(_) => ProviderError::Other("network request failed".into()),
    })?;
    response_text(response.status, response.body)
}
fn base_url(path: &str) -> Result<Url, ProviderError> {
    Url::parse(BASE)
        .unwrap()
        .join(path)
        .map_err(|e| parse_error(e.to_string()))
}
fn episode_rows(id: &str) -> Result<Vec<(String, Episode)>, ProviderError> {
    let numeric_id = id
        .rsplit('-')
        .next()
        .filter(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()))
        .ok_or_else(|| parse_error("invalid title key"))?;
    let url = base_url(&format!("api/theme/episode/list/{numeric_id}"))?;
    parse_episode_rows(&fetch(&url)?, id)
}
// AJAX responses wrap markup in JSON. Parsing JSON avoids destroying escaped title text.
fn markup(raw: &str) -> Result<String, ProviderError> {
    if raw.trim_start().starts_with('{') {
        let json: Value = serde_json::from_str(raw).map_err(|e| parse_error(e.to_string()))?;
        json.get("html")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| parse_error("missing html in AJAX response"))
    } else {
        Ok(raw.to_owned())
    }
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let pattern = format!(r#"(?:^|\s){}\s*=\s*(?:"([^"]*)"|'([^']*)')"#, name);
    let found = captures(&pattern, tag).into_iter().next()?;
    let value = found
        .get(1)
        .filter(|s| !s.is_empty())
        .or_else(|| found.get(2))?;
    Some(entities(value))
}
fn entities(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let Some(end) = rest.find(';').filter(|end| *end <= 12) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let token = &rest[1..end];
        let decoded = match token {
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "lt" => Some('<'),
            "gt" => Some('>'),
            _ => token
                .strip_prefix("#x")
                .and_then(|v| u32::from_str_radix(v, 16).ok())
                .or_else(|| token.strip_prefix('#').and_then(|v| v.parse().ok()))
                .and_then(char::from_u32),
        };
        if let Some(ch) = decoded {
            out.push(ch)
        } else {
            out.push_str(&rest[..=end])
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

fn parse_hits(html: &str) -> Result<Vec<SearchHit>, ProviderError> {
    let html = html.split("id=\"main-sidebar\"").next().unwrap_or(html);
    let matches = captures(
        r#"(?s)<h3\b[^>]*class=["'][^"']*film-name[^"']*["'][^>]*>\s*<a\b([^>]*)>"#,
        html,
    );
    let mut hits = Vec::new();
    for tag in matches {
        if let (Some(href), Some(title)) = (attr(&tag[1], "href"), attr(&tag[1], "title")) {
            let key = href
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or_default();
            if !key.is_empty() && !hits.iter().any(|h: &SearchHit| h.id == key) {
                hits.push(SearchHit {
                    id: key.into(),
                    title,
                    synonyms: vec![],
                    episode_count: None,
                    year: None,
                    format: None,
                });
            }
        }
    }
    if hits.is_empty() && !html.contains("film_list") && !html.contains("No results") {
        return Err(parse_error("unrecognized search page"));
    }
    Ok(hits)
}

fn parse_episode_rows(raw: &str, key: &str) -> Result<Vec<(String, Episode)>, ProviderError> {
    let html = markup(raw)?;
    let tags = captures(r"<a\b[^>]*>", &html);
    let mut rows = Vec::new();
    for tag in tags {
        let tag = tag[0].as_str();
        if let (Some(number), Some(id), Some(href)) = (
            attr(tag, "data-number"),
            attr(tag, "data-id"),
            attr(tag, "href"),
        ) {
            if !href
                .split('?')
                .next()
                .unwrap_or_default()
                .ends_with(&format!("/watch/{key}"))
            {
                continue;
            }
            if !id.bytes().all(|b| b.is_ascii_digit()) || id.is_empty() {
                continue;
            }
            let mut episode = Episode {
                number,
                title: None,
                duration_secs: None,
                description: None,
                thumbnail: None,
                air_date: None,
                filler: None,
            };
            episode.title = attr(tag, "title");
            rows.push((id, episode));
        }
    }
    rows.sort_by(|a, b| episode_order(&a.1.number, &b.1.number));
    rows.dedup_by(|a, b| a.1.number == b.1.number);
    if rows.is_empty() {
        return Err(ProviderError::NotFound);
    }
    Ok(rows)
}

fn embed_url(raw: &str, translation: &str) -> Result<Url, ProviderError> {
    let html = markup(raw)?;
    let tags = captures(r"<[a-zA-Z][^>]*>", &html);
    for tag in tags {
        let tag = tag[0].as_str();
        if attr(tag, "data-type").as_deref() == Some(translation)
            && attr(tag, "data-server-name").as_deref() == Some("ZokoAnime")
        {
            let encoded = attr(tag, "data-hash").ok_or_else(|| parse_error("missing embed URL"))?;
            let bytes = STANDARD
                .decode(encoded)
                .map_err(|e| parse_error(e.to_string()))?;
            return web_url(std::str::from_utf8(&bytes).map_err(|e| parse_error(e.to_string()))?);
        }
    }
    Err(ProviderError::NotFound)
}

fn stream_config(html: &str) -> Result<Value, ProviderError> {
    let blob = captures(r#"window\.__P\s*=\s*"([^"]+)""#, html)
        .into_iter()
        .next()
        .ok_or_else(|| parse_error("missing embed configuration"))?;
    let bytes = STANDARD
        .decode(&blob[1])
        .map_err(|e| parse_error(e.to_string()))?;
    let key = b"otaku-embed-v1";
    let decoded: Vec<u8> = bytes
        .iter()
        .enumerate()
        .map(|(i, b)| b ^ key[i % key.len()])
        .collect();
    serde_json::from_slice(&decoded)
        .map_err(|e| parse_error(format!("invalid embed configuration: {e}")))
}

impl Guest for Component {
    fn describe() -> Manifest {
        Manifest {
            id: "ani-cli".into(),
            display_name: "ani-cli".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            allowed_hosts: HOSTS.iter().map(|s| (*s).into()).collect(),
            translation_types: vec!["sub".into(), "dub".into()],
            capabilities: vec![],
        }
    }
    fn search(query: String, mode: String) -> Result<Vec<SearchHit>, ProviderError> {
        translation(&mode)?;
        let mut url = base_url("search")?;
        url.query_pairs_mut().append_pair("keyword", &query);
        parse_hits(&fetch(&url)?)
    }
    fn list_episodes(id: String, mode: String) -> Result<Vec<Episode>, ProviderError> {
        translation(&mode)?;
        Ok(episode_rows(&id)?
            .into_iter()
            .map(|(_, episode)| episode)
            .collect())
    }
    fn resolve(
        id: String,
        episode: String,
        mode: String,
    ) -> Result<Vec<MediaStream>, ProviderError> {
        translation(&mode)?;
        let rows = episode_rows(&id)?;
        let (episode_id, _) = rows
            .iter()
            .find(|(_, e)| e.number == episode)
            .ok_or(ProviderError::NotFound)?;
        let mut url = base_url("api/theme/episode/servers")?;
        url.query_pairs_mut().append_pair("episodeId", episode_id);
        let embed = embed_url(&fetch(&url)?, &mode)?;
        let config = stream_config(&fetch(&embed)?)?;
        streams(
            &config,
            &format!("{}/", embed.origin().ascii_serialization()),
        )
    }
    fn sources(
        _id: String,
        _episode: String,
        mode: String,
    ) -> Result<Vec<SourceCandidate>, ProviderError> {
        translation(&mode)?;
        Ok(vec![])
    }
    fn resolve_source(
        _id: String,
        _episode: String,
        _mode: String,
        _source_id: String,
    ) -> Result<Vec<MediaStream>, ProviderError> {
        Err(ProviderError::NotFound)
    }
}
fn streams(config: &Value, referer: &str) -> Result<Vec<MediaStream>, ProviderError> {
    let url = web_url(
        config
            .get("src")
            .and_then(Value::as_str)
            .ok_or_else(|| parse_error("missing stream URL"))?,
    )?;
    let mut subtitles = vec![];
    if let Some(tracks) = config.get("subtitles").and_then(Value::as_array) {
        for track in tracks {
            if let Some(src) = track.get("src").and_then(Value::as_str)
                && let Ok(url) = web_url(src)
            {
                subtitles.push(Subtitle {
                    url: url.to_string(),
                    hard: false,
                    format: Some("vtt".into()),
                    language: track
                        .get("srclang")
                        .or_else(|| track.get("label"))
                        .and_then(Value::as_str)
                        .unwrap_or("und")
                        .to_owned(),
                });
            }
        }
    }
    Ok(vec![MediaStream {
        url: url.to_string(),
        kind: "hls".into(),
        quality: None,
        headers: vec![("Referer".into(), referer.into())],
        subtitles,
        download_source: None,
        pick_note: None,
    }])
}
#[cfg(target_arch = "wasm32")]
bindings::export!(Component with_types_in bindings);
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn search_handles_entities_reordered_attributes_and_duplicates() {
        let html = r#"<h3 class="film-name"><a title="Stars &amp; Rain &#039;2&#039;" href="/stars-42">x</a></h3>
        <h3 class="film-name"><a href="/stars-42" title="Duplicate">x</a></h3>"#;
        let hits = parse_hits(html).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Stars & Rain '2'");
        assert_eq!(hits[0].id, "stars-42");
        assert!(parse_hits("<html>unexpected</html>").is_err());
    }

    #[test]
    fn ajax_episode_list_filters_other_titles_and_sorts_numerically() {
        let raw =
            json!({"html": r#"<a href="/watch/stars-42?ep=10" data-id="10" data-number="10"></a>
        <a data-number="2" href="/watch/stars-42?ep=2" data-id="2"></a>
        <a data-number="1" data-id="1" href="/watch/other-8?ep=1"></a>"#})
            .to_string();
        let rows = parse_episode_rows(&raw, "stars-42").unwrap();
        assert_eq!(
            rows.iter()
                .map(|(_, e)| e.number.as_str())
                .collect::<Vec<_>>(),
            ["2", "10"]
        );
        assert!(parse_episode_rows(r#"{"unexpected":true}"#, "stars-42").is_err());
    }

    #[test]
    fn server_selection_respects_translation_and_rejects_non_web_urls() {
        let sub = STANDARD.encode("https://embed.example/sub");
        let dub = STANDARD.encode("https://embed.example/dub");
        let html = format!(
            r#"<div data-hash="{sub}" data-server-name="ZokoAnime" data-type="sub"></div>
        <div data-type="dub" data-hash="{dub}" data-server-name="ZokoAnime"></div>"#
        );
        assert_eq!(embed_url(&html, "dub").unwrap().path(), "/dub");
        assert!(web_url("file:///etc/passwd").is_err());
        assert!(web_url("https://user:password@embed.example/").is_err());
    }

    #[test]
    fn configuration_decodes_and_malformed_blobs_report_errors() {
        let payload = br#"{"src":"https://cdn.example/master.m3u8","subtitles":[]}"#;
        let key = b"otaku-embed-v1";
        let encoded = STANDARD.encode(
            payload
                .iter()
                .enumerate()
                .map(|(i, b)| b ^ key[i % key.len()])
                .collect::<Vec<_>>(),
        );
        assert_eq!(
            stream_config(&format!(r#"window.__P="{encoded}";"#)).unwrap()["src"],
            "https://cdn.example/master.m3u8"
        );
        assert!(stream_config(r#"window.__P="%%%";"#).is_err());
    }
}

#[cfg(test)]
mod boundary_tests {
    use super::*;
    #[test]
    fn errors_preserve_not_found_and_challenges() {
        assert!(matches!(
            response_text(404, vec![]),
            Err(ProviderError::NotFound)
        ));
        assert!(matches!(
            response_text(403, vec![]),
            Err(ProviderError::Blocked(_))
        ));
        assert!(matches!(
            response_text(200, b"<title>Just a moment</title>".to_vec()),
            Err(ProviderError::Blocked(_))
        ));
        assert!(matches!(
            response_text(200, vec![255]),
            Err(ProviderError::Parse(_))
        ));
    }
    #[test]
    fn streams_preserve_hls_headers_and_soft_subtitles() {
        let result = streams(&serde_json::json!({"src":"https://cdn.example/master.m3u8", "subtitles":[{"src":"https://cdn.example/sub?id=1", "srclang":"en"},{"src":"file:///etc/passwd"}]}), "https://embed.example/").unwrap();
        assert_eq!(result[0].kind, "hls");
        assert_eq!(
            result[0].headers,
            [("Referer".into(), "https://embed.example/".into())]
        );
        assert_eq!(result[0].subtitles.len(), 1);
        assert_eq!(result[0].subtitles[0].format.as_deref(), Some("vtt"));
        assert_eq!(result[0].subtitles[0].language, "en");
        assert!(!result[0].subtitles[0].hard);
        assert!(streams(&serde_json::json!({"src":"file:///etc/passwd"}), "").is_err());
    }
    #[test]
    fn unsupported_translations_and_source_ids_are_not_found() {
        assert!(matches!(
            Component::search("unused".into(), "unknown".into()),
            Err(ProviderError::NotFound)
        ));
        assert!(matches!(
            Component::resolve_source("unused".into(), "1".into(), "sub".into(), "unknown".into()),
            Err(ProviderError::NotFound)
        ));
        assert!(
            Component::sources("unused".into(), "1".into(), "sub".into())
                .unwrap()
                .is_empty()
        );
    }
}
