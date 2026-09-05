use std::collections::HashMap;
use serde_json::Value;
use std::path::{Path, PathBuf};
use crate::ytdlp_config::base_ydl_cli_args;
use std::sync::OnceLock;
use regex::Regex;
use tokio::process::Command;
use reqwest::Client;
use std::time::Duration;


fn build_client() -> Option<Client> {
    Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .ok()
}

fn media_link_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(
        r"(?i)(https?://)?(www\.)?(m\.)?(youtube\.com|youtu\.be|music\.youtube\.com|soundcloud\.com|snd\.sc|bandcamp\.com|vimeo\.com|mixcloud\.com|audiomack\.com|twitch\.tv|twitter\.com|x\.com|tiktok\.com|vt\.tiktok\.com|vm\.tiktok\.com)/\S+"
    ).unwrap())
}
fn youtube_domain_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(
        r"(?i)(https?://)?(www\.)?(m\.)?(music\.)?(youtube\.com|youtu\.be)/"
    ).unwrap())
}
fn youtube_playlist_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(
        r"(?i)(https?://)?(www\.)?(m\.)?(music\.)?(youtube\.com|youtu\.be)/\S*[?&]list=[\w-]+"
    ).unwrap())
}
fn extract_match(re: &Regex, text: &str) -> Option<String> {
    re.find(text).map(|m| m.as_str().to_string())
}

pub fn extract_media_link(text: &str) -> Option<String> {
    extract_match(media_link_regex(), text)
}

pub fn is_media_link(text: Option<&str>) -> bool {
    text.is_some_and(|t| media_link_regex().is_match(t))
}

pub fn is_youtube_link(url: Option<&str>) -> bool {
    url.is_some_and(|u| youtube_domain_regex().is_match(u))
}

pub fn is_playlist_link(text: Option<&str>) -> bool {
    text.is_some_and(|t| youtube_playlist_regex().is_match(t))
}

pub fn extract_playlist_link(text: &str) -> Option<String> {
    extract_match(youtube_playlist_regex(), text)
}

pub fn guess_artist_title(source_title: &str, uploader: &str) -> (String, String) {
    for sep in [" - ", " – ", " — ", " | "] {
        if source_title.contains(sep) {
            let parts: Vec<&str> = source_title.splitn(2, sep).collect();
            return (parts[0].trim().to_string(), parts[1].trim().to_string());
        }
    }
    (uploader.trim().to_string(), source_title.trim().to_string())

}
async fn run_ytdlp(args: &[String]) -> Result<serde_json::Value, String> {
    let output = spawn_ytdlp(args).await?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    for line in stdout.lines().rev() {
        let line = line.trim();
        if line.starts_with('{') {
            return serde_json::from_str(line)
                .map_err(|e| format!("Не удалось распарсить JSON от yt-dlp: {e}"));
        }
    }
    Err("yt-dlp не вернул JSON с метаданными".to_string())
}

async fn spawn_ytdlp(args: &[String]) -> Result<std::process::Output, String> {
    let output = Command::new("python3")
        .arg("-m")
        .arg("yt_dlp")
        .args(args)
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let msg = stderr.trim();
        let err = if msg.is_empty() {
            format!("yt-dlp завершился с кодом {:?}", output.status.code())
        } else {
            msg.to_string()
        };
        log::error!("yt-dlp упал: {err}");
        return Err(err);
    }

    Ok(output)
}
pub struct YoutubeOembed {
    pub title: String,
    pub author_name: String,
}
const YOUTUBE_OEMBED_URL: &str = "https://www.youtube.com/oembed";
pub async fn get_youtube_oembed(url: &str) -> Option<YoutubeOembed> {
    let client = build_client()?;
    let mut params = HashMap::new();
    params.insert("url", url);
    params.insert("format", "json");
    let resp = client.get(YOUTUBE_OEMBED_URL)
        .query(&params)
        .send()
        .await.ok()?;
    let data:serde_json::Value = resp.json().await.ok()?;
    Some(YoutubeOembed{
        title: data["title"].as_str().unwrap_or("").to_string(),
        author_name: data["author_name"].as_str().unwrap_or("").to_string()})
}



pub struct DownloadResult {
    pub path: PathBuf,
    pub title: String,
    pub uploader: String,
    pub duration: f64,
    pub thumbnail: String,
    pub width: u64,
    pub height: u64,
}


async fn execute_ytdlp_with_retry(
    args: &[String],
    out_dir: &str,
    file_id: &str,
    extension: &str
) -> Result<Value, String> {
    let mut info: Option<Value> = None;
    let mut last_error = None;

    for attempt in 0..3 {
        match run_ytdlp(args).await {
            Ok(json_data) => {
                info = Some(json_data);
                break;
            }
            Err(e) => {
                let err_string = e.to_string();
                let retryable = err_string.contains("403")
                    || err_string.contains("Requested format is not available")
                    || err_string.contains("Sign in to confirm");

                if attempt == 2 || !retryable {
                    log::error!("yt-dlp не смог обработать после {} попыток: {err_string}", attempt + 1);
                    return Err(err_string);
                }
                log::warn!("yt-dlp попытка {} не удалась, повтор: {err_string}", attempt + 1);
                last_error = Some(err_string);
            }
        }
    }

    let info = match info {
        Some(data) => data,
        None => return Err(last_error.unwrap_or_else(|| "Unknown error".to_string())),
    };

    // Проверяем физическое существование файла (.mp3 или .mp4)
    let expected_path = Path::new(out_dir).join(format!("{file_id}.{extension}"));
    if !expected_path.exists() {
        return Err(format!("Файл не был создан после обработки утилитой ({extension})"));
    }

    Ok(info)
}


pub async fn download_audio(url: &str, out_dir: &str) -> Result<DownloadResult, String> {
    log::info!("Скачивание аудио начато: {url}");
    let file_id = uuid::Uuid::new_v4().to_string();
    let out_template = Path::new(out_dir).join(format!("{file_id}.%(ext)s"));
    let out_template_str = out_template.to_string_lossy().to_string();

    let mut args: Vec<String> = vec![
        "--no-simulate", "--print-json",
        "-f", "bestaudio/best",
        "--extract-audio", "--audio-format", "mp3", "--audio-quality", "192",
        "-o", &out_template_str,
        "--no-playlist", "--quiet", "--no-warnings",
    ]
        .into_iter()
        .map(String::from)
        .collect();

    args.extend(base_ydl_cli_args());
    args.push(url.to_string());

    let info = execute_ytdlp_with_retry(&args, out_dir, &file_id, "mp3").await?;

    let mp3_path = Path::new(out_dir).join(format!("{file_id}.mp3"));
    log::info!("Скачивание аудио завершено: {}", mp3_path.display());

    Ok(DownloadResult {
        path: mp3_path,
        title: info["title"].as_str().unwrap_or("").to_string(),
        uploader: info["uploader"].as_str().unwrap_or("").to_string(),
        duration: info["duration"].as_f64().unwrap_or(0.0),
        thumbnail: info["thumbnail"].as_str().unwrap_or("").to_string(),
        // Для аудио ставим дефолтные значения размеров
        width: 0,
        height: 0,
    })
}


pub async fn download_video(url: &str, out_dir: &str) -> Result<DownloadResult, String> {
    log::info!("Скачивание видео начато: {url}");
    let file_id = uuid::Uuid::new_v4().to_string();
    let out_template = Path::new(out_dir).join(format!("{file_id}.%(ext)s"));
    let out_template_str = out_template.to_string_lossy().to_string();

    let mut args: Vec<String> = vec![
        "--no-simulate", "--print-json",
        "-f", "bv*[ext=mp4]+ba[ext=m4a]/best[ext=mp4]/best",
        "--merge-output-format", "mp4",
        "-o", &out_template_str,
        "--no-playlist", "--quiet", "--no-warnings",
    ]
        .into_iter()
        .map(String::from)
        .collect();

    args.extend(base_ydl_cli_args());
    args.push(url.to_string());

    // Вызываем общую логику с повторами (проверяем создание .mp4)
    let info = execute_ytdlp_with_retry(&args, out_dir, &file_id, "mp4").await?;

    let mp4_path = Path::new(out_dir).join(format!("{file_id}.mp4"));
    log::info!("Скачивание видео завершено: {}", mp4_path.display());

    Ok(DownloadResult {
        path: mp4_path,
        title: info["title"].as_str().unwrap_or("").to_string(),
        uploader: info["uploader"].as_str().unwrap_or("").to_string(),
        duration: info["duration"].as_f64().unwrap_or(0.0),
        thumbnail: info["thumbnail"].as_str().unwrap_or("").to_string(),
        // Безопасно вытаскиваем размеры видео из JSON
        width: info["width"].as_u64().unwrap_or(0),
        height: info["height"].as_u64().unwrap_or(0),
    })
}


pub struct PlaylistEntry {
    pub url: String,
    pub title: String,
}

pub async fn get_playlist_entries(url: &str, limit: Option<usize>) -> Result<Vec<PlaylistEntry>, String> {
    let mut args:Vec<String> = vec!["--flat-playlist".to_string(), "--dump-json".to_string(), "--quiet".to_string(), "--no-warnings".to_string()];
    args.extend(base_ydl_cli_args());
    if let Some(n) = limit {
        args.push("--playlist-end".to_string());
        args.push(n.to_string());
    }
    args.push(url.to_string());
    
    let proc = spawn_ytdlp(&args).await?;
    let mut entries = Vec::new();
    let stdout = String::from_utf8_lossy(&proc.stdout);
    for line in stdout.lines(){
        let line = line.trim();
        if !line.starts_with('{') {
            continue;
        }
        let d: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let video_url = d["url"].as_str()
            .or(d["webpage_url"].as_str())
            .map(|s| s.to_string())
            .or_else(|| d["id"].as_str().map(|id| format!("https://www.youtube.com/watch?v={}", id)));
        let Some(video_url) = video_url else {
            continue;
        };
        let title = d["title"].as_str().unwrap_or("").to_string();

        entries.push(PlaylistEntry { url: video_url, title });

    }
    Ok(entries)
}
