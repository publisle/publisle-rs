FROM rust:1.90.0-bookworm@sha256:3914072ca0c3b8aad871db9169a651ccfce30cf58303e5d6f2db16d1d8a7e58f AS compiler
WORKDIR /build
COPY native/pdf/Cargo.toml native/pdf/Cargo.lock ./
COPY native/pdf/src ./src
RUN --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/build/target \
    cargo build --release --locked && cp target/release/publisle-pdf /publisle-pdf

FROM node:24.21.0-bookworm-slim@sha256:d6aa754f16b3197301076f047b5def2f02ea1dbbc2ca920407d46d7ec7f87b20 AS fonts
RUN apt-get update && apt-get install -y --no-install-recommends fonts-noto-core fonts-noto-cjk fonts-dejavu-core \
    && mkdir -p /fonts/licenses \
    && cp /usr/share/fonts/truetype/noto/NotoSerif-Regular.ttf /usr/share/fonts/truetype/noto/NotoSerif-Bold.ttf \
          /usr/share/fonts/truetype/noto/NotoSerif-Italic.ttf /usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf \
          /usr/share/fonts/truetype/noto/NotoNaskhArabic-Regular.ttf /usr/share/fonts/truetype/dejavu/DejaVuSans.ttf \
          /usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc /fonts/ \
    && cp /usr/share/doc/fonts-noto-core/copyright /fonts/licenses/noto-core.txt \
    && cp /usr/share/doc/fonts-noto-cjk/copyright /fonts/licenses/noto-cjk.txt \
    && cp /usr/share/doc/fonts-dejavu-core/copyright /fonts/licenses/dejavu.txt

FROM node:24.21.0-bookworm-slim@sha256:d6aa754f16b3197301076f047b5def2f02ea1dbbc2ca920407d46d7ec7f87b20 AS runtime
WORKDIR /app
COPY deploy/vps/runtime/package.json deploy/vps/runtime/package-lock.json ./
RUN npm ci --omit=dev --ignore-scripts && npm cache clean --force
COPY --from=compiler /publisle-pdf /usr/local/bin/publisle-pdf
COPY --from=fonts /fonts /app/fonts
COPY deploy/vps/native-fonts.json /app/native-fonts.json
COPY packages/pdf-service/dist/worker.mjs /app/worker.mjs
ENV HOME=/tmp VIPS_CONCURRENCY=1 MALLOC_ARENA_MAX=2
USER 1000:1000
ENTRYPOINT ["node", "--max-old-space-size=96", "/app/worker.mjs"]
