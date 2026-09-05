use teloxide::adaptors::DefaultParseMode;
use teloxide::Bot;
use teloxide::payloads::SendVideoSetters;
use teloxide::prelude::Requester;
use teloxide::types::Message;
use tokio::fs;
use SimpleMusicBot::downloader::{download_video, extract_media_link};
use SimpleMusicBot::config::download_dir;
use teloxide::types::InputFile;
use teloxide::requests::ResponseResult;

pub async fn cmd_video(message: Message, bot: DefaultParseMode<Bot>) -> ResponseResult<()> {
    let text = message.text().unwrap_or("");
    let url_text = text.splitn(2, char::is_whitespace).nth(1).unwrap_or("");
    let url = extract_media_link(url_text);

    let Some(url) = url else {
        bot.send_message(message.chat.id, "Пришли ссылку после команды.").await?;
        return Ok(());
    };

    download_and_send_video(message, &url, bot).await;
    Ok(())
}

pub async fn download_and_send_video(message: Message, url: &str, bot: DefaultParseMode<Bot>) {
    let Ok(status_msg) = bot.send_message(message.chat.id, "⏳ Качаю видео...").await else {
        return;
    };
    fs::create_dir_all(download_dir()).await.ok();

    let result = match download_video(url, download_dir()).await {
        Ok(result) => result,
        Err(e) => {
            let _ = bot.edit_message_text(message.chat.id, status_msg.id, format!("❌ Не удалось скачать видео: {e}")).await;
            return;
        }
    };

    let mp4_path = result.path.clone();
    let title = result.title.trim();
    let safe_title = if title.is_empty() { "video" } else { title };
    let safe_name = format!("{}.mp4", safe_title.replace('/', "-"));

    let video_file = InputFile::file(&mp4_path).file_name(safe_name);
    let _ = bot.edit_message_text(message.chat.id, status_msg.id, "📤 Отправляю файл...").await;

    let mut send = bot.send_video(message.chat.id, video_file).caption("Готово! 🎬");
    if result.width > 0 {
        send = send.width(result.width as u32);
    }
    if result.height > 0 {
        send = send.height(result.height as u32);
    }
    if result.duration > 0.0 {
        send = send.duration(result.duration as u32);
    }

    match send.await {
        Ok(_) => {
            let _ = bot.delete_message(message.chat.id, status_msg.id).await;
        }
        Err(e) => {
            let _ = bot.edit_message_text(message.chat.id, status_msg.id, format!("❌ Не удалось отправить видео: {e}")).await;
        }
    }

    if tokio::fs::metadata(&mp4_path).await.is_ok() {
        let _ = tokio::fs::remove_file(&mp4_path).await;
    }
}

