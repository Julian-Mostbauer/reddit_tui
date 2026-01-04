# Simplified Alpine version - without .env handling
FROM rust:1.92-alpine AS builder

# Install build dependencies
RUN apk add --no-cache \
    musl-dev \
    gcc \
    openssl-dev \
    openssl-libs-static

WORKDIR /usr/src/reddit_tui

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Cache dependencies
RUN mkdir -p src && \
    echo "fn main() {}" > src/main.rs && \
    cargo fetch && \
    cargo build --release

# Copy source
COPY src ./src

# Force rebuild with actual source
RUN rm -f target/release/deps/reddit_tui* && \
    cargo build --release --locked

# Runtime image
FROM alpine:latest

# Install runtime dependencies
RUN apk add --no-cache \
    ca-certificates \
    openssl \
    ncurses-terminfo-base

# Create non-root user
RUN addgroup -g 1000 reddit_tui_user && \
    adduser -D -u 1000 -G reddit_tui_user reddit_tui_user

WORKDIR /app

# Copy binary from builder
COPY --from=builder /usr/src/reddit_tui/target/release/reddit_tui .

# Set permissions
RUN chown -R reddit_tui_user:reddit_tui_user /app
USER reddit_tui_user

ENTRYPOINT ["./reddit_tui"]
