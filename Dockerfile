FROM rust:1.92-alpine AS builder
RUN apk add --no-cache musl-dev gcc openssl-dev
WORKDIR /usr/src/reddit_tui
COPY . .
RUN cargo build --release

FROM alpine:latest
RUN apk add --no-cache ca-certificates openssl ncurses-terminfo-base
RUN adduser -D -u 1000 reddit_user
WORKDIR /app
COPY --from=builder /usr/src/reddit_tui/target/release/reddit_tui .
USER reddit_user
ENTRYPOINT ["./reddit_tui"]
