mod handlers;
use crate::handlers::audio::{
    handle_media_link, handle_playlist_link, confirm_tags, ask_manual_input,
    receive_manual_input, use_source_tags, cancel_tags,
    choose_playlist_mode_auto, choose_playlist_mode_manual,
    ytdirect_callback, ytskip_callback, PlaylistContext, TrackMeta,
};
use crate::handlers::video::cmd_video;

use teloxide::adaptors::DefaultParseMode;
use teloxide::{prelude::*, types::{ParseMode, CallbackQuery}};
use teloxide::macros::BotCommands;
use teloxide::dispatching::dialogue::InMemStorage;
use teloxide::dptree;
use SimpleMusicBot::config::bot_token;
use SimpleMusicBot::state;
use SimpleMusicBot::state::DownloadMode;

#[derive(Clone, Default)]
pub enum State {
    #[default]
    Idle,
    WaitingConfirmation {
        mp3_path: String,
        tags: TrackMeta,
        source_tags: TrackMeta,
        playlist: Option<PlaylistContext>,
    },
    WaitingManualInput {
        mp3_path: String,
        tags: TrackMeta,
        source_tags: TrackMeta,
        playlist: Option<PlaylistContext>,
    },
    WaitingPlaylistMode {
        queue: Vec<String>,
        total: usize,
    },
    WaitingTrackFallback {
        playlist: Option<PlaylistContext>,
    },
}

pub type UserDialogue = Dialogue<State, InMemStorage<State>>;

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "Доступные команды:")]
pub enum Commands {
    #[command(description = "Запуск")]
    Start,
    #[command(description = "Смена режима")]
    Source,
    #[command(description = "Информация")]
    Info,
    #[command(description = "Скачать видео")]
    Video
}

async fn commands_handler(bot: DefaultParseMode<Bot>, msg: Message, cmd: Commands) -> ResponseResult<()> {
    let start_command_txt = concat!(
        "Привет! Пришли ссылку на YouTube(Можно плейлсты!), SoundCloud или другие сервисы ",
        "скачаю трек, подберу обложку и метаданные и перед ",
        "сохранением покажу, что нашёл, чтобы ты подтвердил.\n\n",
        "YouTube часто блокирует скачивание с датацентровых IP — если ",
        "ловишь 403 или пустые файлы - /source - переключит режим бота.\n\n",
        "Если хочешь скачать не трек, а видео /video\n\n",
        "Разработчик: https://github.com/jacobballnarch \n\n",
        "Исходники открыты в репо: https://github.com/jacobballnarch/SimpleMusicBot"
    );
    match cmd {
        Commands::Start => {
            bot.send_message(msg.chat.id, start_command_txt).await?;
        }
        Commands::Source => {
            let current_mode = state::toggle_mode(msg.chat.id.0);
            log::info!("chat {}: Смена режима: {:?}", msg.chat.id.0, current_mode);
            let text = match current_mode {
                DownloadMode::Resolve => concat!(
                "🔀 Режим переключён: <b>resolve</b>.\n\n",
                "Для YouTube-ссылок теперь беру название/исполнителя через oEmbed ",
                "(без скачивания с YouTube), а сам файл ищу и качаю с SoundCloud/Bandcamp.\n\n",
                "Вернуть скачивание напрямую с YouTube — снова /source"
                ),
                DownloadMode::Direct => concat!(
                "🔀 Режим переключён: <b>direct</b>.\n\n",
                "Скачиваю треки напрямую с YouTube, как раньше."
                ),
            };
            bot.send_message(msg.chat.id, text).await?;
        }
        Commands::Info => {
            bot.send_message(msg.chat.id, concat!("Разработчик: https://github.com/jacobballnarch \n\n",
            "Исходники открыты в репо: https://github.com/jacobballnarch/SimpleMusicBot")).await?;
        }
        Commands::Video => {
            cmd_video(msg, bot).await?;
        }
    }
    Ok(())
}


#[tokio::main]
async fn main() {
    env_logger::init();

    if bot_token().is_empty() {
        panic!("BOT TOKEN IS EMPTY CHECK YOUR .env");
    }
    let bot = Bot::new(bot_token()).parse_mode(ParseMode::Html);
    log::info!("Starting Bot");

    let command_branch = Update::filter_message()
        .filter_command::<Commands>()
        .endpoint(commands_handler);
    let manual_input_branch = Update::filter_message()
        .branch(dptree::filter(|state: State| matches!(state, State::WaitingManualInput { .. })).endpoint(receive_manual_input));

    let playlist_link_branch = Update::filter_message()
        .filter(|msg: Message| SimpleMusicBot::downloader::is_playlist_link(msg.text()))
        .endpoint(handle_playlist_link);

    let media_link_branch = Update::filter_message()
        .filter(|msg: Message| SimpleMusicBot::downloader::is_media_link(msg.text()))
        .endpoint(handle_media_link);

    let message_handler = Update::filter_message()
        .enter_dialogue::<Message, InMemStorage<State>, State>()
        .branch(command_branch)
        .branch(manual_input_branch)
        .branch(playlist_link_branch)
        .branch(media_link_branch);

    let confirmation_branch = dptree::filter(|state: State| matches!(state, State::WaitingConfirmation { .. }))
        .branch(dptree::filter(|q: CallbackQuery| q.data.as_deref() == Some("confirm")).endpoint(confirm_tags))
        .branch(dptree::filter(|q: CallbackQuery| q.data.as_deref() == Some("edit")).endpoint(ask_manual_input))
        .branch(dptree::filter(|q: CallbackQuery| q.data.as_deref() == Some("use_source")).endpoint(use_source_tags))
        .branch(dptree::filter(|q: CallbackQuery| q.data.as_deref() == Some("cancel")).endpoint(cancel_tags));

    let playlist_mode_branch = dptree::filter(|state: State| matches!(state, State::WaitingPlaylistMode { .. }))
        .branch(dptree::filter(|q: CallbackQuery| q.data.as_deref() == Some("pl_auto")).endpoint(choose_playlist_mode_auto))
        .branch(dptree::filter(|q: CallbackQuery| q.data.as_deref() == Some("pl_manual")).endpoint(choose_playlist_mode_manual));

    let ytdirect_branch = Update::filter_callback_query()
        .filter(|q: CallbackQuery| q.data.as_deref().is_some_and(|d| d.starts_with("ytdirect:")))
        .endpoint(ytdirect_callback);

    let ytskip_branch = Update::filter_callback_query()
        .filter(|q: CallbackQuery| q.data.as_deref().is_some_and(|d| d.starts_with("ytskip:")))
        .endpoint(ytskip_callback);

    let callback_handler = Update::filter_callback_query()
        .enter_dialogue::<CallbackQuery, InMemStorage<State>, State>()
        .branch(confirmation_branch)
        .branch(playlist_mode_branch)
        .branch(ytdirect_branch)
        .branch(ytskip_branch);

    let handler = dptree::entry()
        .branch(message_handler)
        .branch(callback_handler);

    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![InMemStorage::<State>::new()])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;
}

