use std::time::Duration;
use id3::{Frame, Tag, TagLike};
use id3::frame::{Content, Picture, PictureType};

pub fn download_artwork(url: &str) -> Option<Vec<u8>> {
    if url.is_empty(){
        return None;
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .ok()?;
    let resp = client.get(url)
        .send()
        .ok()?;
    let bytes = resp.bytes().ok()?;
    Some(bytes.to_vec())
}
#[derive(Clone)]
pub struct TrackTags {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub year: Option<String>,
    pub artwork_bytes: Option<Vec<u8>>,
}

pub fn embed_tags(mp3_path: &str, tags: TrackTags) {
    let mut audio = Tag::read_from_path(mp3_path).unwrap_or_else(|_| Tag::new());
    if let Some(title) = &tags.title {
        audio.set_title(title);
    }
    if let Some(artist) = &tags.artist {
        audio.set_artist(artist);
    }
    if let Some(album) = &tags.album {
        audio.set_album(album);
    }
    if let Some(genre) = &tags.genre {
        audio.set_genre(genre);
    }
    if let Some(year) = &tags.year {
        if let Ok(year_num) = year.parse::<i32>() {
            audio.set_year(year_num);
        }
    }
    let artwork_bytes = tags.artwork_bytes.unwrap_or_else(Vec::new);

    let picture = Picture {
        mime_type: "image/jpeg".to_string(),
        picture_type: PictureType::CoverFront,
        description: "Cover".to_string(),
        data: artwork_bytes.clone(),
    };
    audio.add_frame(Frame::with_content("APIC", Content::Picture(picture)));

    audio.write_to_path(mp3_path, id3::Version::Id3v24).expect("Не удалось сохранить теги в файл");

}