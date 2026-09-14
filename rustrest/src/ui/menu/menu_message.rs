#[derive(Debug, Clone)]
pub enum MenuMessage {
    FileNew,
    FileOpen,
    FileOpenGitFolder,
    FileExit,
    CommandPalette,
    HelpAbout,
    CheckForUpdate,
    OpenPluginManager,
    /// plugin_id, command_id.
    Plugin(String, String),
}
