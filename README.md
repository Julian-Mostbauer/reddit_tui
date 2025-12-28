# reddit_tui

This project contains a small terminal UI to browse Reddit.


## Testing

Optional live integration tests
-------------------------------
There is an optional live test that fetches `https://www.reddit.com/.json`.
To run it, set the environment variable `REDDIT_TEST=1` and run the test:

```bash
REDDIT_TEST=1 cargo test --package reddit_tui --bin reddit_tui -- reddit_api::model::tests::test_reddit_home_page --exact --nocapture
```

Note: the HTTP client sets a `User-Agent` header (`reddit_tui/0.1`) because Reddit rejects requests with empty/default User-Agent headers.
