# reddit_tui

This project contains a small terminal UI to browse Reddit.

## Build

> Using Rust 1.92

Rust/Cargo is required to build. Installation guide: https://doc.rust-lang.org/cargo/getting-started/installation.html

Run `cargo build --release` to build the project locally

## Docker

[Docker image can be found here](https://hub.docker.com/repository/docker/julianmostbauer/reddit_tui/general)

Docker version is missing these features:
- open post in browser
- view a posts media locally

If these are important build the project locally

## Testing

Optional live integration tests
-------------------------------
There is an optional live test that fetches `https://www.reddit.com/.json`.
To run it, set the environment variable `REDDIT_TEST=1` and run the test:

```bash
REDDIT_TEST=1 cargo test --package reddit_tui --bin reddit_tui -- reddit_api::model::tests::test_reddit_home_page --exact --nocapture
```

Note: the HTTP client sets a `User-Agent` header (`reddit_tui/0.1`) because Reddit rejects requests with empty/default User-Agent headers.
