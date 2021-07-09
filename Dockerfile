FROM rust:1 AS builder
MAINTAINER Mikkel Kroman <mk@maero.dk>

WORKDIR /usr/src

RUN apt-get update \
  && apt install -y \
    libssl-dev \
    libsqlite3-dev \
  && rm -rf /var/lib/apt/lists/*

COPY . .

RUN cargo fetch

RUN cargo build \
  --release

RUN strip \
  --strip-all \
  target/release/offstream

FROM debian:buster-slim

ENV DATABASE_PATH=/data/films.db

RUN apt-get update \
  && apt install -y \
    libssl1.1 \
    libsqlite3-0 \
    ca-certificates \
  && rm -rf /var/lib/apt/lists/*

VOLUME /data
WORKDIR /root

COPY --from=builder /usr/src/target/release/offstream .

WORKDIR /data

ENTRYPOINT ["/root/offstream", "--database-path", "${DATABASE_PATH}"]
