pub use rustrest_core::session::{SavedSession, SavedTabEntry};

pub fn load() -> Option<SavedSession> {
    rustrest_core::session::load(crate::APP_NAME)
}
