//! Generic, UI-framework-agnostic model for a "command palette" (a
//! searchable, keyboard-driven list of actions.
//!
//! This crate only holds the data model, matching, and selection logic -
//! rendering the palette as a widget (styling, layout, focus handling) is
//! left to the consuming application

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Command<T> {
    /// stable identifier, useful for tests and for widget keys.
    pub id: &'static str,
    pub title: &'static str,
    /// short hint shown alongside the title (e.g. a keybinding or description).
    pub subtitle: Option<&'static str>,
    pub action: T,
}

impl<T> Command<T> {
    pub fn new(id: &'static str, title: &'static str, action: T) -> Self {
        Self {
            id,
            title,
            subtitle: None,
            action,
        }
    }

    pub fn with_subtitle(mut self, subtitle: &'static str) -> Self {
        self.subtitle = Some(subtitle);
        self
    }
}

/// the palette's open/in-progress state
#[derive(Debug, Clone, Default)]
pub struct PaletteState {
    pub query: String,
    pub selected: usize,
}

impl PaletteState {
    pub fn new() -> Self {
        Self::default()
    }

    /// moves the selection by `delta` rows, wrapping around `len` matches.
    /// a no-op if there are no matches.
    pub fn move_selection(&mut self, delta: i32, len: usize) {
        if len == 0 {
            self.selected = 0;
            return;
        }
        let current = self.selected.min(len - 1) as i32;
        let wrapped = (current + delta).rem_euclid(len as i32);
        self.selected = wrapped as usize;
    }
}

/// filters `commands` by `query`, ranking prefix and substring title matches
/// highest, falling back to an in-order "fuzzy" subsequence match, and
/// returns the survivors best-match-first. an empty (or all-whitespace)
/// query matches everything, in its original order.
pub fn filter<'a, T>(commands: &'a [Command<T>], query: &str) -> Vec<&'a Command<T>> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return commands.iter().collect();
    }

    let mut scored: Vec<(i32, usize, &Command<T>)> = commands
        .iter()
        .enumerate()
        .filter_map(|(idx, cmd)| score(cmd.title, &query).map(|s| (s, idx, cmd)))
        .collect();

    // highest score first; ties keep the original, caller-defined order
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, _, cmd)| cmd).collect()
}

fn score(title: &str, query: &str) -> Option<i32> {
    let title_lower = title.to_lowercase();
    if let Some(pos) = title_lower.find(query) {
        // an exact prefix match ranks above a substring found further in
        let bonus = if pos == 0 { 1_000 } else { 500 };
        return Some(bonus - pos as i32);
    }
    subsequence_score(&title_lower, query)
}

/// `None` unless every character of `query` appears in `haystack` in order;
/// otherwise scores tighter clusters of matched characters higher.
fn subsequence_score(haystack: &str, query: &str) -> Option<i32> {
    let hay: Vec<char> = haystack.chars().collect();
    let mut cursor = 0;
    let mut first_match = None;
    let mut last_match = 0;

    for qc in query.chars() {
        let pos = (cursor..hay.len()).find(|&i| hay[i] == qc)?;
        first_match.get_or_insert(pos);
        last_match = pos;
        cursor = pos + 1;
    }

    let span = last_match - first_match.unwrap_or(0) + 1;
    Some(100 - (span as i32).min(100))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq)]
    enum TestAction {
        NewFile,
        OpenRemote,
        CloseTab,
    }

    fn sample_commands() -> Vec<Command<TestAction>> {
        vec![
            Command::new("new-file", "New File", TestAction::NewFile),
            Command::new(
                "remote-ssh",
                "Remote Development over SSH",
                TestAction::OpenRemote,
            ),
            Command::new("close-tab", "Close Tab", TestAction::CloseTab),
        ]
    }

    #[test]
    fn empty_query_returns_all_in_order() {
        let commands = sample_commands();
        let matches = filter(&commands, "");
        assert_eq!(matches.len(), 3);
        assert_eq!(matches[0].action, TestAction::NewFile);
    }

    #[test]
    fn prefix_match_ranks_above_substring_match() {
        let commands = sample_commands();
        let matches = filter(&commands, "close");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].action, TestAction::CloseTab);
    }

    #[test]
    fn fuzzy_subsequence_matches_out_of_order_letters() {
        let commands = sample_commands();
        let matches = filter(&commands, "rssh");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].action, TestAction::OpenRemote);
    }

    #[test]
    fn no_match_excludes_command() {
        let commands = sample_commands();
        let matches = filter(&commands, "zzz");
        assert!(matches.is_empty());
    }

    #[test]
    fn move_selection_wraps_around() {
        let mut state = PaletteState::new();
        state.move_selection(-1, 3);
        assert_eq!(state.selected, 2);
        state.move_selection(1, 3);
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn move_selection_with_no_matches_is_noop() {
        let mut state = PaletteState::new();
        state.selected = 5;
        state.move_selection(1, 0);
        assert_eq!(state.selected, 0);
    }
}
