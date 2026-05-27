FROM rust:1-bookworm AS builder

WORKDIR /app

RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        ca-certificates \
        libssl-dev \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock README.md /app/
COPY migrations /app/migrations
COPY src /app/src

RUN cargo build --release --locked

FROM debian:bookworm-slim

ARG APP_UID=1000
ARG APP_GID=1000

RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        ca-certificates \
        libssl3 \
    && groupadd --gid "${APP_GID}" limenet \
    && useradd --uid "${APP_UID}" --gid "${APP_GID}" --create-home --home-dir /home/limenet --shell /usr/sbin/nologin limenet \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/limenet /usr/local/bin/limenet
COPY migrations /app/migrations

ENV LIMENET_BIND=0.0.0.0:3000
ENV HOME=/home/limenet

EXPOSE 3000

USER limenet

ENTRYPOINT ["limenet"]
