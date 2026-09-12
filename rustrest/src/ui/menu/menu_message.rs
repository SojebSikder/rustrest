#[derive(Debug, Clone, Copy)]
pub enum MenuMessage {
    FileNew,
    FileOpen,
    FileOpenGitFolder,
    FileExit,
    HelpAbout,
    CheckForUpdate,
}
