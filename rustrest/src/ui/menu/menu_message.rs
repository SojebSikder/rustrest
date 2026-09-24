#[derive(Debug, Clone)]
pub enum MenuMessage {
    FileNew,
    FileOpen,
    FileOpenGitFolder,
    FileExit,
    CommandPalette,
    HelpAbout,
    ViewReleaseNotes,
    CheckForUpdate,
    OpenPluginManager,
    OpenSettings,
    /// plugin_id, command_id.
    Plugin(String, String),
    /// plugin_id, format_id, file-picker extensions.
    ImportViaPlugin(String, String, Vec<String>),
}
