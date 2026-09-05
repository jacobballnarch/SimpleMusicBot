use std::collections::HashMap;
use std::time::Duration;

const ITUNES_SEARCH_URL: &str = "https://itunes.apple.com/search";

#[derive(Debug)]
pub struct TrackMetadata {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub year: String,
    pub artwork_url: String,
}

pub fn search_track_metadata(query: String) -> Option<TrackMetadata> {
    let mut params = HashMap::new();
    params.insert(String::from("term"), query);
    params.insert(String::from("media"), "music".to_string());
    params.insert(String::from("entity"), "song".to_string());
    params.insert(String::from("limit"), "1".to_string());

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .ok()?;

    let resp = client.get(ITUNES_SEARCH_URL)
        .query(&params)
        .send()
        .ok()?;

    let data: serde_json::Value = resp.json().ok()?;

    let first = data["results"].get(0)?;

    let title = first["trackName"].as_str().unwrap_or("").to_string();
    let artist = first["artistName"].as_str().unwrap_or("").to_string();
    let album = first["collectionName"].as_str().unwrap_or("").to_string();
    let genre = first["primaryGenreName"].as_str().unwrap_or("").to_string();

    let artwork_url_raw = first["artworkUrl100"].as_str().unwrap_or("");
    let artwork_url = artwork_url_raw.replace("100x100bb", "600x600bb");

    let release_date = first["releaseDate"].as_str().unwrap_or("");
    let year = release_date[..4.min(release_date.len())].to_string();

    Some(TrackMetadata { title, artist, album, genre, year, artwork_url })
}

