use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use teloxide::adaptors::DefaultParseMode;
use teloxide::payloads::{SendAudioSetters, SendMessageSetters, SendPhotoSetters};
use teloxide::prelude::*;
use teloxide::types::{CallbackQuery, ChatId, InlineKeyboardButton, InlineKeyboardMarkup, InputFile};
use teloxide::requests::ResponseResult;
use tokio::fs;

use SimpleMusicBot::config::download_dir;
use SimpleMusicBot::downloader::{
    download_audio, extract_media_link, extract_playlist_link, get_playlist_entries,
    get_youtube_oembed, guess_artist_title, is_youtube_link,
};
use SimpleMusicBot::metadata_service::search_track_metadata;
use SimpleMusicBot::resolvers::{resolve_track_from_alternate_sources, TrackNotFoundError};
use SimpleMusicBot::state::{self, DownloadMode};
use SimpleMusicBot::tagging::{download_artwork, embed_tags, TrackTags};

use crate::{State, UserDialogue};

const MAX_PLAYLIST_TRACKS: usize = 100;

#[derive(Clone, Default)]
pub struct TrackMeta {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub year: String,
    pub artwork_url: String,
}

#[derive(Clone)]
pub struct PlaylistContext {
    pub queue: Vec<String>,
    pub index: usize,
    pub total: usize,
}

enum ResolveError {
    NotFound(String), 
    Other(String),
}

struct ResolvedTrack {
    path: PathBuf,
    tags: TrackMeta,
    source_tags: TrackMeta,
}

fn cq_chat_id(q: &CallbackQuery) -> Option<ChatId> {
    q.message.as_ref().map(|m| m.chat().id)
}

fn meta_from_itunes(m: SimpleMusicBot::metadata_service::TrackMetadata) -> TrackMeta {
    TrackMeta { title: m.title, artist: m.artist, album: m.album, genre: m.genre, year: m.year, artwork_url: m.artwork_url }
}


pub async fn handle_media_link(bot: DefaultParseMode<Bot>, msg: Message, dialogue: UserDialogue) -> ResponseResult<()> {
    let Some(url) = extract_media_link(msg.text().unwrap_or("")) else { return Ok(()); };
    start_tagging_flow(&bot, msg.chat.id, &dialogue, &url, None).await
}

pub async fn handle_playlist_link(bot: DefaultParseMode<Bot>, msg: Message, dialogue: UserDialogue) -> ResponseResult<()> {
    let Some(url) = extract_playlist_link(msg.text().unwrap_or("")) else { return Ok(()); };
    let status = bot.send_message(msg.chat.id, "🔎 Получаю список треков плейлиста...").await?;

    let entries = match get_playlist_entries(&url, Some(MAX_PLAYLIST_TRACKS)).await {
        Ok(e) => e,
        Err(e) => {
            log::error!("Не удалось получить плейлист {url}: {e}");
            bot.edit_message_text(msg.chat.id, status.id, format!("❌ Не удалось получить плейлист: {e}")).await?;
            return Ok(());
        }
    };
    if entries.is_empty() {
        bot.edit_message_text(msg.chat.id, status.id, "❌ Плейлист пуст или недоступен.").await?;
        return Ok(());
    }

    let queue: Vec<String> = entries.into_iter().map(|e| e.url).collect();
    let total = queue.len();
    let note = if total >= MAX_PLAYLIST_TRACKS {
        format!("\n\n(показаны первые {MAX_PLAYLIST_TRACKS} треков — плейлист может быть длиннее)")
    } else { String::new() };
    bot.edit_message_text(msg.chat.id, status.id, format!("Нашёл {total} треков.{note}")).await?;

    let kb = InlineKeyboardMarkup::new(vec![vec![
        InlineKeyboardButton::callback("⚡ Авто", "pl_auto"),
        InlineKeyboardButton::callback("✏️ Вручную по каждому", "pl_manual"),
    ]]);
    bot.send_message(
        msg.chat.id,
        "Как обработать метаданные треков?\n\n\
         ⚡ <b>Авто</b> — беру найденное в iTunes (или как есть) без подтверждений.\n\
         ✏️ <b>Вручную</b> — как с одиночным треком, подтверждение по каждому.",
    ).reply_markup(kb).await?;

    dialogue.update(State::WaitingPlaylistMode { queue, total }).await.ok();
    Ok(())
}

async fn resolve_and_download(url: &str, chat_id: i64) -> Result<ResolvedTrack, ResolveError> {
    if is_youtube_link(Some(url)) && state::get_mode(chat_id) == DownloadMode::Resolve {
        let oembed = get_youtube_oembed(url).await
            .filter(|o| !o.title.is_empty())
            .ok_or_else(|| ResolveError::Other("Не удалось получить название видео с YouTube (oEmbed)".into()))?;

        let (guessed_artist, guessed_title) = guess_artist_title(&oembed.title, &oembed.author_name);
        let source_tags = TrackMeta { title: guessed_title.clone(), artist: guessed_artist.clone(), ..Default::default() };
        let query = format!("{guessed_artist} {guessed_title}");
        let meta = tokio::task::spawn_blocking(move || search_track_metadata(query)).await.ok().flatten();
        let mut tags = match meta {
            Some(m) if !m.title.is_empty() => meta_from_itunes(m),
            _ => source_tags.clone(),
        };

        let artist_for_search = if tags.artist.is_empty() { guessed_artist.clone() } else { tags.artist.clone() };
        let title_for_search = if tags.title.is_empty() { guessed_title.clone() } else { tags.title.clone() };

        let result = resolve_track_from_alternate_sources(&artist_for_search, &title_for_search, download_dir(), url)
            .await
            .map_err(|e: TrackNotFoundError| ResolveError::NotFound(e.youtube_url))?;

        if tags.artwork_url.is_empty() {
            tags.artwork_url = result.thumbnail.clone();
        }
        return Ok(ResolvedTrack { path: result.path, tags, source_tags });
    }

    let result = download_audio(url, download_dir()).await.map_err(ResolveError::Other)?;
    let (guessed_artist, guessed_title) = guess_artist_title(&result.title, &result.uploader);
    let source_tags = TrackMeta {
        title: guessed_title.clone(), artist: guessed_artist.clone(),
        artwork_url: result.thumbnail.clone(), ..Default::default()
    };
    let query = format!("{guessed_artist} {guessed_title}");
    let meta = tokio::task::spawn_blocking(move || search_track_metadata(query)).await.ok().flatten();
    let tags = match meta {
        Some(m) if !m.title.is_empty() => meta_from_itunes(m),
        _ => source_tags.clone(),
    };
    Ok(ResolvedTrack { path: result.path, tags, source_tags })
}


fn start_tagging_flow<'a>(
    bot: &'a DefaultParseMode<Bot>, chat_id: ChatId, dialogue: &'a UserDialogue,
    url: &'a str, playlist: Option<PlaylistContext>,
) -> Pin<Box<dyn Future<Output = ResponseResult<()>> + Send + 'a>> {
    Box::pin(async move {
        let status = bot.send_message(chat_id, "⏳ Качаю аудио...").await?;
        fs::create_dir_all(download_dir()).await.ok();

        match resolve_and_download(url, chat_id.0).await {
            Ok(resolved) => {
                bot.delete_message(chat_id, status.id).await.ok();
                enter_confirmation(bot, chat_id, dialogue, resolved.path, resolved.tags, resolved.source_tags, playlist).await
            }
            Err(ResolveError::NotFound(youtube_url)) => {
                bot.edit_message_text(chat_id, status.id, "⚠️ Не нашёл трек ни на SoundCloud, ни на Bandcamp.").await.ok();
                let context = if playlist.is_some() { "manual" } else { "single" };
                offer_direct_fallback(bot, dialogue, chat_id, &youtube_url, context, playlist).await
            }
            Err(ResolveError::Other(e)) => {
                bot.edit_message_text(chat_id, status.id, format!("❌ Не удалось скачать трек: {e}")).await.ok();
                advance_playlist_or_stop(bot, chat_id, dialogue, playlist).await
            }
        }
    })
}

async fn enter_confirmation(
    bot: &DefaultParseMode<Bot>, chat_id: ChatId, dialogue: &UserDialogue,
    mp3_path: PathBuf, tags: TrackMeta, source_tags: TrackMeta, playlist: Option<PlaylistContext>,
) -> ResponseResult<()> {
    send_confirmation(bot, chat_id, &tags).await?;
    dialogue.update(State::WaitingConfirmation {
        mp3_path: mp3_path.to_string_lossy().to_string(), tags, source_tags, playlist,
    }).await.ok();
    Ok(())
}

async fn send_confirmation(bot: &DefaultParseMode<Bot>, chat_id: ChatId, tags: &TrackMeta) -> ResponseResult<()> {
    let kb = InlineKeyboardMarkup::new(vec![
        vec![InlineKeyboardButton::callback("✅ Это она", "confirm"), InlineKeyboardButton::callback("✏️ Ввести вручную", "edit")],
        vec![InlineKeyboardButton::callback("📥 Взять как есть", "use_source"), InlineKeyboardButton::callback("❌ Отмена", "cancel")],
    ]);
    let d = |s: &str| if s.is_empty() { "—".to_string() } else { s.to_string() };
    let caption = format!(
        "Нашёл вот такой вариант:\n\n🎤 Исполнитель: {}\n🎵 Название: {}\n💿 Альбом: {}\n📅 Год: {}\n\nВсё верно?",
        d(&tags.artist), d(&tags.title), d(&tags.album), d(&tags.year),
    );
    match (!tags.artwork_url.is_empty()).then(|| reqwest::Url::parse(&tags.artwork_url)).and_then(Result::ok) {
        Some(u) => { bot.send_photo(chat_id, InputFile::url(u)).caption(caption).reply_markup(kb).await?; }
        None => { bot.send_message(chat_id, caption).reply_markup(kb).await?; }
    }
    Ok(())
}


pub async fn confirm_tags(bot: DefaultParseMode<Bot>, q: CallbackQuery, dialogue: UserDialogue) -> ResponseResult<()> {
    bot.answer_callback_query(q.id.clone()).await?;
    let Some(chat_id) = cq_chat_id(&q) else { return Ok(()); };
    if let Some(State::WaitingConfirmation { mp3_path, tags, playlist, .. }) = dialogue.get().await.ok().flatten() {
        finalize(&bot, chat_id, &dialogue, mp3_path, tags, playlist).await?;
    }
    Ok(())
}

pub async fn ask_manual_input(bot: DefaultParseMode<Bot>, q: CallbackQuery, dialogue: UserDialogue) -> ResponseResult<()> {
    bot.answer_callback_query(q.id.clone()).await?;
    let Some(chat_id) = cq_chat_id(&q) else { return Ok(()); };
    if let Some(State::WaitingConfirmation { mp3_path, tags, source_tags, playlist }) = dialogue.get().await.ok().flatten() {
        bot.send_message(chat_id,
                         "Пришли метаданные одной строкой в формате:\n<code>Исполнитель - Название - Альбом - Год</code>\n\nАльбом и год можно не указывать."
        ).await?;
        dialogue.update(State::WaitingManualInput { mp3_path, tags, source_tags, playlist }).await.ok();
    }
    Ok(())
}

pub async fn receive_manual_input(bot: DefaultParseMode<Bot>, msg: Message, dialogue: UserDialogue) -> ResponseResult<()> {
    let Some(State::WaitingManualInput { mp3_path, mut tags, playlist, .. }) = dialogue.get().await.ok().flatten() else {
        return Ok(());
    };
    let text = msg.text().unwrap_or("");
    let parts: Vec<&str> = text.split(" - ").map(str::trim).collect();
    if let Some(v) = parts.first().filter(|s| !s.is_empty()) { tags.artist = v.to_string(); }
    if let Some(v) = parts.get(1).filter(|s| !s.is_empty()) { tags.title = v.to_string(); }
    if let Some(v) = parts.get(2).filter(|s| !s.is_empty()) { tags.album = v.to_string(); }
    if let Some(v) = parts.get(3).filter(|s| !s.is_empty()) { tags.year = v.to_string(); }
    finalize(&bot, msg.chat.id, &dialogue, mp3_path, tags, playlist).await
}

pub async fn use_source_tags(bot: DefaultParseMode<Bot>, q: CallbackQuery, dialogue: UserDialogue) -> ResponseResult<()> {
    bot.answer_callback_query(q.id.clone()).await?;
    let Some(chat_id) = cq_chat_id(&q) else { return Ok(()); };
    if let Some(State::WaitingConfirmation { mp3_path, source_tags, playlist, .. }) = dialogue.get().await.ok().flatten() {
        finalize(&bot, chat_id, &dialogue, mp3_path, source_tags, playlist).await?;
    }
    Ok(())
}

pub async fn cancel_tags(bot: DefaultParseMode<Bot>, q: CallbackQuery, dialogue: UserDialogue) -> ResponseResult<()> {
    bot.answer_callback_query(q.id.clone()).await?;
    let Some(chat_id) = cq_chat_id(&q) else { return Ok(()); };
    if let Some(State::WaitingConfirmation { mp3_path, playlist, .. }) = dialogue.get().await.ok().flatten() {
        if tokio::fs::metadata(&mp3_path).await.is_ok() {
            tokio::fs::remove_file(&mp3_path).await.ok();
        }
        bot.send_message(chat_id, "Окей, отменил и удалил файл.").await?;
        advance_playlist_or_stop(&bot, chat_id, &dialogue, playlist).await?;
    }
    Ok(())
}

async fn finalize(bot: &DefaultParseMode<Bot>, chat_id: ChatId, dialogue: &UserDialogue, mp3_path: String, tags: TrackMeta, playlist: Option<PlaylistContext>) -> ResponseResult<()> {
    let processing = bot.send_message(chat_id, "🏷 Прописываю метаданные...").await?;
    send_finished_track(bot, chat_id, PathBuf::from(mp3_path), tags).await;
    bot.delete_message(chat_id, processing.id).await.ok();
    advance_playlist_or_stop(bot, chat_id, dialogue, playlist).await
}


async fn send_finished_track(bot: &DefaultParseMode<Bot>, chat_id: ChatId, mp3_path: PathBuf, tags: TrackMeta) {
    let artwork_bytes = if tags.artwork_url.is_empty() {
        None
    } else {
        let url = tags.artwork_url.clone();
        tokio::task::spawn_blocking(move || download_artwork(&url)).await.ok().flatten()
    };

    let track_tags = TrackTags {
        title: (!tags.title.is_empty()).then(|| tags.title.clone()),
        artist: (!tags.artist.is_empty()).then(|| tags.artist.clone()),
        album: (!tags.album.is_empty()).then(|| tags.album.clone()),
        genre: (!tags.genre.is_empty()).then(|| tags.genre.clone()),
        year: (!tags.year.is_empty()).then(|| tags.year.clone()),
        artwork_bytes,
    };

    let embed_path = mp3_path.to_string_lossy().to_string();
    let _ = tokio::task::spawn_blocking(move || embed_tags(&embed_path, track_tags)).await;

    let artist = if tags.artist.is_empty() { "Unknown".to_string() } else { tags.artist };
    let title = if tags.title.is_empty() { "Unknown".to_string() } else { tags.title };
    let safe_name = format!("{artist} - {title}.mp3").replace('/', "-");

    let audio_file = InputFile::file(&mp3_path).file_name(safe_name);
    let _ = bot.send_audio(chat_id, audio_file).title(title).performer(artist).caption("Готово! 🎶").await;

    if tokio::fs::metadata(&mp3_path).await.is_ok() {
        tokio::fs::remove_file(&mp3_path).await.ok();
    }
}

async fn advance_playlist_or_stop(bot: &DefaultParseMode<Bot>, chat_id: ChatId, dialogue: &UserDialogue, playlist: Option<PlaylistContext>) -> ResponseResult<()> {
    let Some(pl) = playlist else {
        dialogue.exit().await.ok();
        return Ok(());
    };
    let next_index = pl.index + 1;
    if next_index >= pl.queue.len() {
        bot.send_message(chat_id, format!("✅ Плейлист обработан: {next_index}/{} треков пройдено.", pl.total)).await?;
        dialogue.exit().await.ok();
        return Ok(());
    }
    let next_url = pl.queue[next_index].clone();
    bot.send_message(chat_id, format!("➡️ Трек {}/{}...", next_index + 1, pl.total)).await?;
    let next_ctx = PlaylistContext { queue: pl.queue, index: next_index, total: pl.total };
    start_tagging_flow(bot, chat_id, dialogue, &next_url, Some(next_ctx)).await
}


pub async fn choose_playlist_mode_auto(bot: DefaultParseMode<Bot>, q: CallbackQuery, dialogue: UserDialogue) -> ResponseResult<()> {
    bot.answer_callback_query(q.id.clone()).await?;
    let Some(chat_id) = cq_chat_id(&q) else { return Ok(()); };
    let Some(State::WaitingPlaylistMode { queue, total }) = dialogue.get().await.ok().flatten() else { return Ok(()); };
    bot.send_message(chat_id, format!("⚡ Начинаю автообработку {total} треков...")).await?;
    dialogue.exit().await.ok();
    process_playlist_auto(&bot, chat_id, queue, total).await;
    Ok(())
}

pub async fn choose_playlist_mode_manual(bot: DefaultParseMode<Bot>, q: CallbackQuery, dialogue: UserDialogue) -> ResponseResult<()> {
    bot.answer_callback_query(q.id.clone()).await?;
    let Some(chat_id) = cq_chat_id(&q) else { return Ok(()); };
    let Some(State::WaitingPlaylistMode { queue, total }) = dialogue.get().await.ok().flatten() else { return Ok(()); };
    bot.send_message(chat_id, format!("✏️ Начинаю ручную обработку {total} треков, трек 1/{total}...")).await?;
    let first = queue[0].clone();
    let ctx = PlaylistContext { queue, index: 0, total };
    start_tagging_flow(&bot, chat_id, &dialogue, &first, Some(ctx)).await
}

async fn process_playlist_auto(bot: &DefaultParseMode<Bot>, chat_id: ChatId, queue: Vec<String>, total: usize) {
    fs::create_dir_all(download_dir()).await.ok();
    let Ok(progress) = bot.send_message(chat_id, format!("🎧 0/{total} готово")).await else { return; };
    let mut sent = 0usize;
    let mut failed = 0usize;

    for (i, url) in queue.iter().enumerate() {
        let idx = i + 1;
        match resolve_and_download(url, chat_id.0).await {
            Ok(resolved) => {
                send_finished_track(bot, chat_id, resolved.path, resolved.tags).await;
                sent += 1;
            }
            Err(ResolveError::NotFound(youtube_url)) => {
                failed += 1;
                let pid = state::register_problem_track(youtube_url, chat_id.0, "auto".to_string());
                let kb = InlineKeyboardMarkup::new(vec![
                    vec![InlineKeyboardButton::callback("⬇️ Скачать с YouTube напрямую", format!("ytdirect:{pid}"))],
                    vec![InlineKeyboardButton::callback("⏭ Пропустить", format!("ytskip:{pid}"))],
                ]);
                let _ = bot.send_message(chat_id, "Не нашёл трек ни на SoundCloud, ни на Bandcamp.").reply_markup(kb).await;
            }
            Err(ResolveError::Other(_)) => { failed += 1; }
        }

        if idx % 3 == 0 || idx == total {
            let _ = bot.edit_message_text(chat_id, progress.id, format!("🎧 {idx}/{total} обработано ({sent} отправлено, {failed} ошибок)")).await;
        }
    }

    let tail = if failed > 0 { format!(", {failed} с ошибками") } else { String::new() };
    let _ = bot.send_message(chat_id, format!("✅ Готово: {sent}/{total} треков отправлено{tail}")).await;
}



async fn offer_direct_fallback(
    bot: &DefaultParseMode<Bot>, dialogue: &UserDialogue, chat_id: ChatId,
    youtube_url: &str, context: &str, playlist: Option<PlaylistContext>,
) -> ResponseResult<()> {
    let pid = state::register_problem_track(youtube_url.to_string(), chat_id.0, context.to_string());
    let kb = InlineKeyboardMarkup::new(vec![
        vec![InlineKeyboardButton::callback("⬇️ Скачать с YouTube напрямую", format!("ytdirect:{pid}"))],
        vec![InlineKeyboardButton::callback("⏭ Пропустить", format!("ytskip:{pid}"))],
    ]);
    bot.send_message(chat_id, "Можно попробовать скачать его напрямую с YouTube (не гарантия), либо пропустить.").reply_markup(kb).await?;
    dialogue.update(State::WaitingTrackFallback { playlist }).await.ok();
    Ok(())
}

pub async fn ytdirect_callback(bot: DefaultParseMode<Bot>, q: CallbackQuery, dialogue: UserDialogue) -> ResponseResult<()> {
    bot.answer_callback_query(q.id.clone()).await?;
    let Some(chat_id) = cq_chat_id(&q) else { return Ok(()); };
    let pid = q.data.as_deref().unwrap_or("").trim_start_matches("ytdirect:").to_string();
    let Some(info) = state::pop_problem_track(&pid) else {
        bot.send_message(chat_id, "Эта карточка уже неактуальна — пришли ссылку заново.").await?;
        return Ok(());
    };
    let playlist = match dialogue.get().await.ok().flatten() {
        Some(State::WaitingTrackFallback { playlist }) => playlist,
        _ => None,
    };

    bot.send_message(chat_id, "⏳ Пробую скачать напрямую с YouTube...").await?;
    fs::create_dir_all(download_dir()).await.ok();

    let result = match download_audio(&info.url, download_dir()).await {
        Ok(r) => r,
        Err(e) => {
            bot.send_message(chat_id, format!("❌ И напрямую не вышло: {e}")).await?;
            if info.context == "manual" {
                return advance_playlist_or_stop(&bot, chat_id, &dialogue, playlist).await;
            }
            return Ok(());
        }
    };

    let (guessed_artist, guessed_title) = guess_artist_title(&result.title, &result.uploader);
    let source_tags = TrackMeta { title: guessed_title.clone(), artist: guessed_artist.clone(), artwork_url: result.thumbnail.clone(), ..Default::default() };
    let query = format!("{guessed_artist} {guessed_title}");
    let meta = tokio::task::spawn_blocking(move || search_track_metadata(query)).await.ok().flatten();
    let tags = match meta {
        Some(m) if !m.title.is_empty() => meta_from_itunes(m),
        _ => source_tags.clone(),
    };

    if info.context == "single" || info.context == "manual" {
        return enter_confirmation(&bot, chat_id, &dialogue, result.path, tags, source_tags, playlist).await;
    }
    send_finished_track(&bot, chat_id, result.path, tags).await;
    Ok(())
}

pub async fn ytskip_callback(bot: DefaultParseMode<Bot>, q: CallbackQuery, dialogue: UserDialogue) -> ResponseResult<()> {
    bot.answer_callback_query(q.id.clone()).await?;
    let Some(chat_id) = cq_chat_id(&q) else { return Ok(()); };
    let pid = q.data.as_deref().unwrap_or("").trim_start_matches("ytskip:").to_string();
    let info = state::pop_problem_track(&pid);
    bot.send_message(chat_id, "⏭ Пропустил этот трек.").await?;
    if info.map(|i| i.context) == Some("manual".to_string()) {
        let playlist = match dialogue.get().await.ok().flatten() {
            Some(State::WaitingTrackFallback { playlist }) => playlist,
            _ => None,
        };
        return advance_playlist_or_stop(&bot, chat_id, &dialogue, playlist).await;
    }
    Ok(())
}