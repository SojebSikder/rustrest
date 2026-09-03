pub use rustrest_core::session::{SavedSession, SavedTabEntry};

pub fn save(session: &SavedSession) {
    rustrest_core::session::save(session, crate::APP_NAME);
}

pub fn load() -> Option<SavedSession> {
    rustrest_core::session::load(crate::APP_NAME)
}
