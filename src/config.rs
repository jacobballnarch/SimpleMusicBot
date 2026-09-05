use std::env;
use std::sync::OnceLock;

static BOT_TOKEN: OnceLock<String> = OnceLock::new();
static DOWNLOAD_DIR: OnceLock<String> = OnceLock::new();

pub fn bot_token() -> &'static str {
    BOT_TOKEN.get_or_init(|| {
        let _ = dotenvy::dotenv();
        env::var("BOT_TOKEN").expect("BOT_TOKEN is not set")
    })
}

pub fn download_dir() -> &'static str {
    DOWNLOAD_DIR.get_or_init(|| {
        let _ = dotenvy::dotenv();
        env::var("DOWNLOAD_DIR").unwrap_or_else(|_| "downloads".to_string())
    })
}
