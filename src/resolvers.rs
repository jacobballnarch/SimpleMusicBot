use std::collections::HashSet;
use std::time::Duration;
use crate::downloader::*;
const MIN_MATCH_SCORE: f32 = 0.5;
const BANDCAMP_SEARCH_URL: &str = "https://bandcamp.com/api/bcsearch_public_api/1/autocomplete_elastic";

#[derive(Debug)]
pub struct TrackNotFoundError {
    pub youtube_url: String,
}

fn tokenize(text: &str) -> HashSet<String>{
    text.split(|c: char| !(c.is_alphabetic() || c == '_' || c == '\''))
        .filter(|s| !s.chars().count() > 1)
        .map(|s| s.to_lowercase())
        .collect()
}
fn match_score(candidate_title: &str, artist: &str, title: &str) -> f32 {
    let mut wanted = tokenize(artist);
    wanted.extend(tokenize(title));
    if wanted.is_empty() {
        return 0.0;
    }
    let found = tokenize(candidate_title);
    let matched = wanted.intersection(&found).count();
    matched as f32 / wanted.len() as f32
}

async fn best_soundcloud_candidate(query: &str, artist: &str, title: &str, limit: Option<usize>) -> Option<PlaylistEntry> {
    let actual_limit = limit.unwrap_or(5);
    let search_query = format!("scsearch{actual_limit}:{query}");
    let candidates = get_playlist_entries(&search_query, None).await.ok()?;
    let mut best = Option::None;
    let mut best_score = 0.0;
    for c in candidates {
        let candidate_title = c.title.as_str();
        let score = match_score(candidate_title, artist, title);
        if score > best_score {
            best_score = score;
            best = Some(c);
        }
    }
    if best_score >= MIN_MATCH_SCORE {
        best
    } else {
        Option::None
    }
}

pub async fn search_bandcamp(query: &str, artist: &str, title: &str) -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .ok()?;
    let resp = client.post(BANDCAMP_SEARCH_URL)
        .json(&serde_json::json!({
            "fan_id": null,
            "full_page": false,
            "search_filter": "a",
            "search_text": query,
        }))
        .send()
        .await
        .ok()?;
    let data: serde_json::Value = resp
        .json()
        .await
        .ok()?;

    let result = data["auto"]["results"].as_array().cloned().unwrap_or_default();
    let mut best = Option::None;
    let mut best_score = 0.0;
    for r in result {
        let type_ = r["type"].as_str().unwrap_or("");
        if type_ != "t" && type_ != "a" {
        continue;
        }
        let name = r["name"].as_str().unwrap_or("");
        let band_name = r["band_name"].as_str().unwrap_or("");
        let candidate_title = format!("{} {}", band_name, name);
        let score = match_score(&candidate_title, artist, title);
        if score > best_score {
            best_score = score;
            best = Some(r);
        }
    }
    if best.is_none() || best_score < MIN_MATCH_SCORE {
        return Option::None;
    }
    let best = best?;
    best["item_url_root"].as_str()
        .or(best["url"].as_str())
        .map(|s| s.to_string())
}

pub async fn resolve_track_from_alternate_sources(artist: &str, title: &str, out_dir: &str, youtube_url: &str) -> Result<DownloadResult, TrackNotFoundError> {
    let queries = [format!("{artist} {title}").trim().to_string(), title.trim().to_string()];
    for query in queries.iter() {
        if query.is_empty() {
            continue;
        }
        let candidate = best_soundcloud_candidate(query, artist, title, None).await;
        if let Some(candidate) = candidate {
            if let Ok(result) = download_audio(&candidate.url, out_dir).await{
                return Ok(result);
            }
        }
    }
    let bandcamp_query = format!("{artist} {title}").trim().to_string();
    if !bandcamp_query.is_empty() {
        if let Some(bandcamp_url) = search_bandcamp(&bandcamp_query, artist, title).await{
            if let Ok(result) = download_audio(&bandcamp_url, out_dir).await {
                return Ok(result);
            }
        }
    }

    Err(TrackNotFoundError {
        youtube_url: youtube_url.to_string(),
    })

}



