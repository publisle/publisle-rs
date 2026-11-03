FROM rust:1.90.0-bookworm@sha256:3914072ca0c3b8aad871db9169a651ccfce30cf58303e5d6f2db16d1d8a7e58f AS compiler
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/build/target \
    CARGO_BUILD_JOBS=2 cargo build --release --locked -p publisle-pdf && cp target/release/publisle-pdf /publisle-pdf

FROM node:24.21.0-bookworm-slim@sha256:d6aa754f16b3197301076f047b5def2f02ea1dbbc2ca920407d46d7ec7f87b20 AS fonts
# Monochrome Noto Emoji (OFL), not Noto Color Emoji.
# Commit b979dba422e445492b0eb9951ac52ee0b4d648c3
# sha256 de6c18832938afc99caf132b39d6a30a19bac7f2e812e28db2535b4608d27551
# Noto Sans Math (OFL), role "math", required for mathematical alphanumeric
# symbols and OpenType MATH delimiters.
# Commit dbd1ab6e65dc59bcda3ca8de9fd372f58f98e0af
# sha256 3f495fe933c06786e4d5f6d86b8ee70b6753a68ee3b9d87528726de0f6e2c47d
RUN apt-get update && apt-get install -y --no-install-recommends fonts-noto-core fonts-noto-cjk fonts-dejavu-core fonts-dejavu-extra ca-certificates curl \
    && mkdir -p /fonts/licenses \
    && cp /usr/share/fonts/truetype/noto/NotoSerif-Regular.ttf /usr/share/fonts/truetype/noto/NotoSerif-Bold.ttf \
          /usr/share/fonts/truetype/noto/NotoSerif-Italic.ttf /usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf \
          /usr/share/fonts/truetype/noto/NotoNaskhArabic-Regular.ttf /usr/share/fonts/truetype/dejavu/DejaVuSans.ttf \
          /usr/share/fonts/truetype/noto/NotoSerif-BoldItalic.ttf \
          /usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf \
          /usr/share/fonts/truetype/dejavu/DejaVuSansMono-Oblique.ttf \
          /usr/share/fonts/truetype/dejavu/DejaVuSansMono-BoldOblique.ttf \
          /usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf \
          /usr/share/fonts/truetype/dejavu/DejaVuSans-Oblique.ttf \
          /usr/share/fonts/truetype/dejavu/DejaVuSans-BoldOblique.ttf \
          /usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc /fonts/ \
    && cp /usr/share/doc/fonts-noto-core/copyright /fonts/licenses/noto-core.txt \
    && cp /usr/share/doc/fonts-noto-cjk/copyright /fonts/licenses/noto-cjk.txt \
    && cp /usr/share/doc/fonts-dejavu-core/copyright /fonts/licenses/dejavu.txt \
    && curl -fsSL -o /fonts/NotoEmoji-Regular.ttf \
        "https://github.com/google/fonts/raw/b979dba422e445492b0eb9951ac52ee0b4d648c3/ofl/notoemoji/NotoEmoji%5Bwght%5D.ttf" \
    && echo "de6c18832938afc99caf132b39d6a30a19bac7f2e812e28db2535b4608d27551  /fonts/NotoEmoji-Regular.ttf" | sha256sum -c - \
    && curl -fsSL -o /fonts/licenses/noto-emoji.txt \
        "https://github.com/google/fonts/raw/b979dba422e445492b0eb9951ac52ee0b4d648c3/ofl/notoemoji/OFL.txt" \
    && curl -fsSL -o /fonts/NotoSansMath-Regular.ttf \
        "https://github.com/google/fonts/raw/dbd1ab6e65dc59bcda3ca8de9fd372f58f98e0af/ofl/notosansmath/NotoSansMath-Regular.ttf" \
    && echo "3f495fe933c06786e4d5f6d86b8ee70b6753a68ee3b9d87528726de0f6e2c47d  /fonts/NotoSansMath-Regular.ttf" | sha256sum -c - \
    && curl -fsSL -o /fonts/licenses/noto-sans-math.txt \
        "https://github.com/google/fonts/raw/dbd1ab6e65dc59bcda3ca8de9fd372f58f98e0af/ofl/notosansmath/OFL.txt"
