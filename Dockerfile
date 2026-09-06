FROM rust:1.98-slim AS builder
WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends cmake pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src && echo "fn main() {}" > src/main.rs && touch src/lib.rs \
    && cargo build --release \
    && rm -rf src

COPY . .

RUN find src -name '*.rs' -exec touch {} + && cargo build --release

FROM debian:trixie-slim
WORKDIR /app

RUN apt-get update \
    && apt-get install -y --no-install-recommends curl ca-certificates python3 python3-pip xz-utils unzip \
    && rm -rf /var/lib/apt/lists/*

RUN pip install --no-cache-dir --break-system-packages \
    yt-dlp yt-dlp-ejs bgutil-ytdlp-pot-provider

RUN pip install --no-cache-dir --break-system-packages --upgrade \
    yt-dlp yt-dlp-ejs bgutil-ytdlp-pot-provider

RUN curl -L https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-amd64-static.tar.xz \
    | tar -xJ -C /usr/local/bin --strip-components=1 --wildcards '*/ffmpeg' '*/ffprobe'

RUN curl -fsSL https://deno.land/install.sh | sh

COPY --from=builder /app/target/release/SimpleMusicBot ./bot
RUN mkdir -p /app/downloads

CMD ["./bot"]