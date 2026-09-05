use std::sync::OnceLock;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use flate2::read::GzDecoder;
use std::io::Read;
use std::path::PathBuf;
use uuid::Uuid;
struct YoutubeClientArgs {
    player_client: Vec<String>,
    formats: Vec<String>,
}
fn youtube_client_extractor_tags() ->  YoutubeClientArgs {
    let result = YoutubeClientArgs {
        player_client: vec!["tv".to_string(), "web_safari".to_string(), "web".to_string(), "android_vr".to_string()],
        formats: vec!["missing_pot".to_string()],
    };
    result
}

struct PotProviderArgs {
    base_url: Vec<String>,
}
fn pot_provider_extractor_tags() -> Option<PotProviderArgs> {
    let base_url = std::env::var("POT_PROVIDER_URL").ok()?;
    Some(PotProviderArgs {
        base_url: vec![base_url],
    })
}

static COOKIES_PATH: OnceLock<Option<String>> = OnceLock::new();

fn resolve_cookies_files() -> Option<String> {
    COOKIES_PATH.get_or_init(|| {
        let raw = std::env::var("YOUTUBE_COOKIES_B64").ok()?;
        if raw.is_empty() {
            return None;
        }
        let decoded = STANDARD.decode(&raw).ok()?;
        let mut decoder = GzDecoder::new(&decoded[..]);
        let mut content_bytes = Vec::new();
        let gzip_ok = decoder.read_to_end(&mut content_bytes).is_ok();
        if !gzip_ok {
            content_bytes = decoded; // не gzip — используем как есть, тот же фолбэк, что в Python
        }
        let content = String::from_utf8(content_bytes).ok()?;
        let filename = format!("yt_cookies_{}.txt", Uuid::new_v4());
        let path: PathBuf = std::env::temp_dir().join(filename);

        std::fs::write(&path, &content).ok()?;
        let path_string = path.to_string_lossy().to_string();
        Some(path_string)

    }).clone()
}
impl YoutubeClientArgs {
    fn to_extractor_arg_string(&self) -> String {
        format!(
            "youtube:player_client={};formats={}",
            self.player_client.join(","),
            self.formats.join(",")
        )
    }
}
impl PotProviderArgs {
    fn to_extractor_arg_string(&self) -> String {
        format!("youtubepot-bgutilhttp:base_url={}", self.base_url.join(","))
    }
}


pub fn base_ydl_cli_args() -> Vec<String> {
    let mut args:Vec<String> = Vec::new();
    let youtube_args = youtube_client_extractor_tags();
    args.push("--extractor-args".to_string());
    args.push(youtube_args.to_extractor_arg_string());
    if let Some(pot_args) = pot_provider_extractor_tags(){
        args.push("--extractor-args".to_string());
        args.push(pot_args.to_extractor_arg_string())
    }
    if let Some(cookies_path) = resolve_cookies_files() {
        args.push("--cookies".to_string());
        args.push(cookies_path);
    }
    args
}



