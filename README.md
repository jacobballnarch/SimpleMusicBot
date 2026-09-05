# SimpleMusicBot
The name speaks for itself. A simple, fast Telegram bot for downloading music from various sources and tagging it with covers/metadata.

## Why SMB?
This bot started as a fix for a personal annoyance — for my mp3 collection I had to download tracks from platforms through third-party sites, then hunt down covers/metadata somewhere else. Kind of a pain.
This project does it all in one request/command.

### Features

1. Just send a link -> the bot downloads the track and lets you confirm the metadata it found via inline buttons.
2. Even though it's built for music, `/video` lets you download videos too.
3. If YouTube is blocking your IP and even cookies + POT-token can't get through — there's a SoundCloud fallback via `/source`. Send a YouTube link, get the file from SC instead.
4. All info about the bot — `/start` and `/info`.

## Installation
YouTube really likes blocking datacenter IPs. POT-tokens and cookies help, sometimes.
```
git clone https://github.com/jacobballnarch/SimpleMusicBot
```
Then create a `.env` file in the project root with these variables (if you're using a deploy service, you can usually set them directly without a file):
```
BOT_TOKEN="" #Your Telegram bot token
DOWNLOAD_DIR="downloads" #Folder where tracks are temporarily downloaded
POT_PROVIDER_URL="" #Optional: see below
YOUTUBE_COOKIES_B64="" #Optional: YouTube cookies in base64, since some deploy services limit env variable length
```
Next, pull in the dependencies from the Dockerfile — or better yet, just use Docker.\
If you're not using Docker, you'll need:
- **Rust 1.98+**
- **Python3** (any recent version)
- **ffmpeg / ffprobe** — audio conversion
    - Ubuntu/Debian: `sudo apt install ffmpeg`
    - macOS: `brew install ffmpeg`
    - Windows: grab it from ffmpeg.org and add to PATH
- **deno** — required by yt-dlp to solve YouTube's JS challenge
    - `curl -fsSL https://deno.land/install.sh | sh` (Linux/macOS)
    - Windows: `irm https://deno.land/install.ps1 | iex`
- **yt-dlp**: `pip install yt-dlp yt-dlp-ejs bgutil-ytdlp-pot-provider`

Once dependencies are installed:
`cargo build --release`\
The executable will be in `target/release/`

That's it! If everything went right, just run the executable.
### POT-PROVIDER
Uses [`bgutil-ytdlp-pot-provider`](https://github.com/Brainicism/bgutil-ytdlp-pot-provider) — spin it up as a separate service alongside the bot and point to it. There's a ready-made image on [Docker Hub](https://hub.docker.com/r/brainicism/bgutil-ytdlp-pot-provider).
## License
This project is licensed under [Apache License 2.0](LICENSE).\
This is a personal-use tool — only use it for content you have the right to, e.g. tracks you already own or that fall under fair use in your jurisdiction. Copyright compliance is the responsibility of the bot's user.

### Third-party software and APIs
- [`yt-dlp`](https://github.com/yt-dlp/yt-dlp) - Unlicense
- [`ffmpeg`](https://ffmpeg.org/) - LGPL v2.1 (used as an external process, not linked)
- [`deno`](https://deno.land/) - MIT
- [`bgutil-ytdlp-pot-provider`](https://github.com/Brainicism/bgutil-ytdlp-pot-provider) - GPL-3.0
- Track metadata - public [iTunes Search API](https://developer.apple.com/library/archive/documentation/AudioVideo/Conceptual/iTuneSearchAPI/)
- For fallback features - public [YouTube oEmbed API](https://oembed.com/)
