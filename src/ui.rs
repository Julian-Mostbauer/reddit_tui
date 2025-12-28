// UI module root - functionality split across submodules for clarity
pub mod app;
pub mod events;
pub mod helpers;
pub mod render;

pub use events::run;

// keep ui.rs minimal: main behavior lives in `events` and rendering in `render`
