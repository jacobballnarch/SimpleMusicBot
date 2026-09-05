![Rust](https://img.shields.io/badge/rust-1.98-orange)
![License](https://img.shields.io/badge/license-Apache%202.0-blue)
# SimpleMusicBot
Название говорит само за себя. Простой и быстрый бот для Telegram для скачивания музыки с разных ресурсов и проставлении обложек/метаданных для них.

## Зачем нужен SMB?
Изначально бот создавался для решения личной проблемы - для своей mp3 коллекции приходилось скачивать трек с площадок через сторонние ресурсы и через другие ресурсы искать обложки/метаданные для трека. Это неудобно.
Этот проект же делает всё за один запрос/комманду.

### Функции

1. Просто прислать ссылку -> бот скачает трек и через inline кнопки вы подтвердите правильность найденных метаданных. 
2. Не смотря на то что бот создавался для скачивания музыки - /video позволяет качать и видео тоже. 
3. Если youtube блочит по IP и даже с cookies и POT-токеном не позволяет скачать - есть фоллбек на SoundCloud - /source. Присылаете ссылку на трек с ютуба, а качаете с SC. 
4. Вся информация о боте - /start и /info.

## Установка
Перед этим предупрежу, что ютуб очень сильно любит блочить IP с датацентров, частично это решается через POT-токены и Cookies - но это не панацея.
```
git clone https://github.com/jacobballnarch/SimpleMusicBot
```
После создайте .env файл в корне проекта с такими переменными: (Если используете сервис для деплоя обычно у них можно вписать напрямую без создания файла)
```
BOT_TOKEN="" #Токен от бота в тг
DOWNLOAD_DIR="downloads" #Папка куда временно качаются треки
POT_PROVIDER_URL="" #Опцианально: Подробнее ниже
YOUTUBE_COOKIES_B64="" #Опцианально: Куки ютуба в base64 формате т.к. у некоторых деплой сервисов есть ограничения по символам
```
Далее подтяните зависимости из Dockerfile или лучше всего просто используйте Docker.\
Если не используете Docker, вам понадобятся:
- **Rust 1.98+**
- **Python3** (любой актуальной версии)
- **ffmpeg / ffprobe** — конвертация аудио
    - Ubuntu/Debian: `sudo apt install ffmpeg`
    - macOS: `brew install ffmpeg`
    - Windows: скачать с ffmpeg.org и добавить в PATH
- **deno** — нужен yt-dlp для обхода JS-челленджа YouTube
    - `curl -fsSL https://deno.land/install.sh | sh` (Linux/macOS)
    - Windows: `irm https://deno.land/install.ps1 | iex`
- **yt-dlp**: `pip install yt-dlp yt-dlp-ejs bgutil-ytdlp-pot-provider`

После установки зависимостей:
`cargo build --release`\
Исполняемый файл будет лежать в `target/release/`

Готово! Если вы всё правильно сделали - то просто запускайте готовый исполняемый файл.
### POT-PROVIDER
Используется [`bgutil-ytdlp-pot-provider`](https://github.com/Brainicism/bgutil-ytdlp-pot-provider) поднимите его как отдельный сервис рядом с ботом и укажите ссылку на него. Есть готовый образ на [`docker hub`](https://hub.docker.com/r/brainicism/bgutil-ytdlp-pot-provider).
## Лицензия
Проект распространяется под [Apache License 2.0](LICENSE).\
Это инструмент для личного использования — используй его для контента,
которым имеешь право распоряжаться (например, треки, которые ты уже купил
или на которые распространяется добросовестное использование в твоей
юрисдикции). За соблюдение авторских прав отвечает пользователь бота.

### Стороннее ПО и API
- [`yt-dlp`](https://github.com/yt-dlp/yt-dlp) - Unlicense
- [`ffmpeg`](https://ffmpeg.org/) - LGPL v2.1 (используется как внешний процесс, не линкуется)
- [`deno`](https://deno.land/) - MIT
- [`bgutil-ytdlp-pot-provider`](https://github.com/Brainicism/bgutil-ytdlp-pot-provider) - GPL-3.0
- Метаданные треков - публичный [iTunes Search API](https://developer.apple.com/library/archive/documentation/AudioVideo/Conceptual/iTuneSearchAPI/)
- Для фоллбек функций - публичный [YouTube oEmbed API](https://oembed.com/)