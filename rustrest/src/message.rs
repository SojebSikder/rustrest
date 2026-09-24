use crate::app::{CollectionSubTab, NewTabProtocol};
use crate::collection::collection::{PostmanCollection, PostmanRequestNode};
use crate::collection::git_ops::{GitRemoteOp, GitStatusSnapshot};
use crate::http_client::HttpResponse;
use crate::plugin_gallery::GalleryEntry;
use crate::ui::command_palette::AppCommand;
use crate::ui::confirm_dialog::ConfirmDialogState;
use crate::ui::context_menu::FieldTarget;
use crate::ui::menu::menu::DropdownMessage;
use crate::ui::menu::menu_message::MenuMessage;
use crate::ui::remote::RemoteAuthKind;
use crate::ui::tab::TabMessage;
use crate::ui::toast::toast::ToastStatus;
use crate::updater::UpdateInfo;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeKind {
    Sidebar,
    RequestPane,
    ConsolePanel,
    RightPanel,
    MultilineField(MultilineFieldKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MultilineFieldKind {
    Auth(usize),
    AuthJwtPayload(usize),
    RawBody(usize),
    GraphQlQuery(usize),
    GraphQlVariables(usize),
    GrpcRequestJson(usize),
    CommitMessage,
    EnvVarValue {
        env_idx: usize,
        var_idx: usize,
    },
    KvValue {
        tab_id: usize,
        field: KvValueField,
        row: usize,
    },
    FormDataValue {
        tab_id: usize,
        row: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KvValueField {
    Param,
    Header,
    Cookie,
    Urlencoded,
}

/// identifies the sidebar item currently being dragged.
#[derive(Debug, Clone)]
pub enum SidebarDragItem {
    Request {
        collection_id: usize,
        parent_path: Vec<String>,
        request_id: usize,
    },
    Folder {
        collection_id: usize,
        path: Vec<String>,
    },
}

/// identifies a single collection/folder/request row in the sidebar, used to
/// track the current multi-selection (Ctrl/Cmd-click toggle, Shift-click range).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SidebarItemKey {
    Collection(usize),
    Folder {
        collection_id: usize,
        path: Vec<String>,
    },
    Request {
        collection_id: usize,
        parent_path: Vec<String>,
        request_id: usize,
    },
}

impl SidebarItemKey {
    pub fn from_drag_item(item: &SidebarDragItem) -> Self {
        match item {
            SidebarDragItem::Request {
                collection_id,
                parent_path,
                request_id,
            } => SidebarItemKey::Request {
                collection_id: *collection_id,
                parent_path: parent_path.clone(),
                request_id: *request_id,
            },
            SidebarDragItem::Folder {
                collection_id,
                path,
            } => SidebarItemKey::Folder {
                collection_id: *collection_id,
                path: path.clone(),
            },
        }
    }

    /// folders and requests can be dragged/moved; a whole collection can't.
    pub fn as_drag_item(&self) -> Option<SidebarDragItem> {
        match self {
            SidebarItemKey::Folder {
                collection_id,
                path,
            } => Some(SidebarDragItem::Folder {
                collection_id: *collection_id,
                path: path.clone(),
            }),
            SidebarItemKey::Request {
                collection_id,
                parent_path,
                request_id,
            } => Some(SidebarDragItem::Request {
                collection_id: *collection_id,
                parent_path: parent_path.clone(),
                request_id: *request_id,
            }),
            SidebarItemKey::Collection(_) => None,
        }
    }
}

/// identifies where a dragged sidebar item was dropped.
#[derive(Debug, Clone)]
pub enum SidebarDropTarget {
    /// dropped onto a folder header - moves the item into that folder.
    Folder {
        collection_id: usize,
        folder_path: Vec<String>,
    },
    /// dropped onto a collection header - moves the item to that collection's root.
    CollectionRoot(usize),
    /// dropped onto a request row - slots the item in right before that sibling.
    Request {
        collection_id: usize,
        parent_path: Vec<String>,
        request_id: usize,
    },
}

#[derive(Debug, Clone)]
pub enum Message {
    /// wraps a context menu option's message so pressing it always closes
    /// the menu, whether or not the action itself does so.
    ContextMenuAction(Box<Message>),
    NewTabPressed,
    NewTerminalTabPressed,
    /// widget -> app: write encoded key bytes to a terminal's PTY.
    TerminalInput(u64, Vec<u8>),
    /// widget -> app: the terminal widget was resized to this cell grid.
    TerminalResized(u64, usize, usize, u16, u16),
    /// PTY subscription -> app: a session needs a redraw, or has exited.
    TerminalNotice(u64, rustrest_terminal::TerminalNotice),
    SidebarCollectionRootClicked(usize),
    CloseTabPressed(usize),
    ActiveTabMessage(TabMessage),
    SendPressed,
    ResponseReceived(usize, Result<HttpResponse, String>),
    TabNameDoubleClick(usize),
    TabNameChanged(usize, String),
    TabNameSave(usize),
    TabRenameBlur,
    TabRenameInputHover(bool),
    ImportCollectionPressed,
    ExportCollectionPressed(usize),

    CollectionLoaded(Option<std::path::PathBuf>, String),
    SaveCollectionPressed(usize),
    CollectionFirstSaved(usize, std::path::PathBuf),

    /// the active workspace's collections finished loading on a background
    /// thread after startup (the window is already showing by this point).
    StartupWorkspaceLoaded(Vec<Result<PostmanCollection, String>>),
    /// installed plugins finished discovering/compiling on a background
    /// thread after startup; instantiating them is still done on the main
    /// thread (cheap, in-memory linking) by the handler.
    StartupPluginsLoaded(Vec<rustrest_plugin_host::PreparedPlugin>),

    SidebarRequestClicked {
        req_node: PostmanRequestNode,
        collection_id: usize,
        parent_path: Vec<String>,
    },

    // saved response ("example") sidebar actions
    SidebarSavedResponseClicked {
        req_node: PostmanRequestNode,
        collection_id: usize,
        index: usize,
    },
    ShowSavedResponseContextMenu {
        collection_id: usize,
        request_id: usize,
        index: usize,
    },
    RenameSavedResponsePressed {
        collection_id: usize,
        request_id: usize,
        index: usize,
    },
    SavedResponseNameChanged {
        collection_id: usize,
        request_id: usize,
        index: usize,
        new_name: String,
    },
    SaveSavedResponseNamePressed,
    DeleteSavedResponsePressed {
        collection_id: usize,
        request_id: usize,
        index: usize,
    },

    // sidebar drag-and-drop (reorder / move requests & folders)
    SidebarDragStarted(SidebarDragItem),
    SidebarDropped(SidebarDropTarget),

    // sidebar multi-select (Ctrl/Cmd-click toggle, Shift-click range) + batch ops
    SidebarItemToggleSelect(SidebarItemKey),
    SidebarItemRangeSelect(SidebarItemKey),
    ClearSidebarSelection,
    BatchDeleteSelectedPressed,
    BatchDeleteConfirmed,
    ModifiersChanged(iced::keyboard::Modifiers),

    // sidebar collapse/expand
    ToggleCollectionCollapsed(usize),
    ToggleFolderCollapsed {
        collection_id: usize,
        folder_path: Vec<String>,
    },

    // tab bar drag-to-reorder
    TabDragStarted(usize),
    TabDragEntered(usize),
    TabDragEnded,

    // environment Actions
    EditEnvironmentPressed(usize),
    CloseEnvEditorPressed,
    EnvSelected(Option<String>),
    CreateEnvironmentPressed,
    DeleteEnvironmentPressed(usize),
    AddEnvVariablePressed(usize),
    DeleteEnvVariablePressed {
        env_idx: usize,
        var_idx: usize,
    },
    EnvVariableKeyChanged {
        env_idx: usize,
        var_idx: usize,
        key: String,
    },
    EnvVariableValueEditorAction {
        env_idx: usize,
        var_idx: usize,
        action: iced::widget::text_editor::Action,
    },
    EnvVariableToggled {
        env_idx: usize,
        var_idx: usize,
        is_active: bool,
    },

    RenameEnvironmentPressed(usize),
    EnvNameChanged(usize, String),
    SaveEnvNamePressed(usize),

    // workspace actions
    WorkspaceSelected(String),
    CreateWorkspacePressed,
    DeleteWorkspacePressed(usize),
    RenameWorkspacePressed(usize),
    WorkspaceNameChanged(usize, String),
    SaveWorkspaceNamePressed(usize),

    // collection viewer actions
    CollectionSubTabSelected(CollectionSubTab),
    /// an edit in a collection root tab's Authorization sub-tab
    CollectionAuth(usize, crate::ui::tab::messages::AuthMessage),
    CollectionScriptTabChanged(usize, crate::ui::tab::types::ScriptTab),
    CollectionScriptAction(
        usize,
        crate::ui::tab::types::ScriptTab,
        iced::widget::text_editor::Action,
    ),
    CollectionVariableChanged {
        collection_id: usize,
        index: usize,
        key: String,
        value: String,
    },
    CollectionVariableToggled {
        collection_id: usize,
        index: usize,
        is_active: bool,
    },
    AddCollectionVariablePressed(usize),
    DeleteCollectionVariablePressed(usize, usize),

    // collection CRUD actions
    CreateNewCollectionPressed,
    DeleteCollectionPressed(usize),

    // folder CRUD actions
    AddFolderPressed {
        collection_id: usize,
        parent_folder_path: Vec<String>,
    },
    DeleteFolderPressed {
        collection_id: usize,
        folder_path: Vec<String>,
    },

    AddRequestPressed {
        collection_id: usize,
        parent_folder_path: Vec<String>,
    },

    DeleteRequestPressed {
        collection_id: usize,
        request_id: usize,
        parent_folder_path: Vec<String>,
    },

    // request rename actions
    RenameRequestPressed {
        collection_id: usize,
        request_id: usize,
    },
    RequestNameChanged {
        collection_id: usize,
        request_id: usize,
        new_name: String,
    },
    SaveRequestNamePressed {
        collection_id: usize,
        request_id: usize,
    },

    // sidebar collapse/expand of a request's saved-responses list
    ToggleSavedResponsesCollapsed(usize),

    // collection Rename Actions
    RenameCollectionPressed(usize),       // trigger edit mode
    CollectionNameChanged(usize, String), // inline text change
    SaveCollectionNamePressed(usize),

    // folder Rename Actions
    RenameFolderPressed {
        collection_id: usize,
        folder_path: Vec<String>, // current path to target folder
    },
    FolderNameChanged {
        collection_id: usize,
        folder_path: Vec<String>,
        new_name: String,
    },
    SaveFolderNamePressed {
        collection_id: usize,
        folder_path: Vec<String>,
    },

    // save request model action
    SaveRequestPressed(usize), // tab index, opens the chooser
    SaveRequestModalCollectionSelected(usize), // pick target collection
    SaveRequestModalFolderSelected(Vec<String>), // pick target folder (optional)
    SaveRequestNameChanged(String),
    SaveRequestConfirmed,
    CloseSaveRequestModal,

    // response timing modal
    CloseResponseTimingModal,

    // Help > About modal
    ShowAboutModal,
    CloseAboutModal,
    AboutModalAction(iced::widget::text_editor::Action),

    // Help > View Release Notes tab
    ViewReleaseNotes,
    ReleaseNotesLoaded(Result<String, String>),
    ReleaseNotesLinkClicked(String),

    //
    ShowCollectionContextMenu(usize),
    ShowGitActionsMenu(usize),
    ShowFolderContextMenu {
        collection_id: usize,
        folder_path: Vec<String>,
    },
    ShowRequestContextMenu {
        collection_id: usize,
        folder_path: Vec<String>,
        request_id: usize,
    },
    CloseContextMenu,
    CursorMoved(iced::Point),

    // reusable text field context menu (Copy/Paste)
    ShowTextFieldContextMenu(FieldTarget, String),
    /// Copy-only context menu for read-only plugin-panel text
    ShowPluginTextContextMenu(String),
    CopyToClipboard(String),
    PasteIntoField(FieldTarget),
    /// copies the given selection and deletes it from the field
    CutFromField(FieldTarget, String),
    TextFieldPasteResolved(FieldTarget, Option<String>),

    MenuInteraction(DropdownMessage<MenuMessage>),
    SaveActiveRequestShortcut,
    CloseActiveTabShortcut,

    // panel resizing
    ResizeDragStarted(ResizeKind),
    ResizeDragEnded,

    // console panel
    ToggleConsolePanel,
    ClearConsoleLogs,

    // git
    InitGitCollectionPressed(usize), // "Save as git folder" on a collection
    GitCollectionDirChosen(usize, Option<std::path::PathBuf>),
    ImportGitCollectionPressed,
    GitCollectionLoaded(
        Option<std::path::PathBuf>,
        Result<PostmanCollection, String>,
    ),
    /// an import target an already-open collection's storage_dir; user must
    /// confirm before we discard in-memory state and reload from disk.
    ReplaceCollectionConfirmed(usize, Box<PostmanCollection>),

    // git status/diff panel (collection root "Git" sub-tab)
    GitStatusRequested(usize),
    GitStatusLoaded(usize, Result<GitStatusSnapshot, String>),
    GitDiffRequested(usize, std::path::PathBuf),
    GitDiffLoaded(usize, std::path::PathBuf, Result<String, String>),

    // git remote sync (push/pull/fetch)
    GitPushPressed(usize),
    GitPullPressed(usize),
    GitFetchPressed(usize),
    GitRemoteOpResult(usize, GitRemoteOp, Result<String, String>),

    // commit modal
    CommitChangesPressed(usize),
    CommitStatusLoaded(usize, String, GitStatusSnapshot),
    CommitMessageChanged(iced::widget::text_editor::Action),
    CommitConfirmed,
    CommitCancelled,
    CommitResult(usize, Result<(), String>),

    // generic reusable confirm dialog
    ShowConfirmDialog(ConfirmDialogState),
    ConfirmDialogAccepted,
    ConfirmDialogCancelled,
    // end git

    // temporary data stores
    AutosaveTick,
    // advances the loading-spinner animation frame; only subscribed to while
    // `Rustrest::any_spinner_active()` is true.
    SpinnerTick,

    ShowToast(String, ToastStatus),
    DismissToast(usize),
    ToastActionPressed(usize),

    // self update
    CheckForUpdate,
    CheckForUpdateSilently,
    UpdateCheckResult(Result<Option<UpdateInfo>, String>),
    SilentUpdateCheckResult(Result<Option<UpdateInfo>, String>),
    InstallUpdate,
    UpdateInstallProgress(crate::updater::UpdateProgress),
    UpdateInstallResult(Result<String, String>),

    // remote development (SSH) - inline "add host" form
    RemoteProfileNameChanged(String),
    RemoteProfileHostChanged(String),
    RemoteProfilePortChanged(String),
    RemoteProfileUsernameChanged(String),
    RemoteProfileAuthKindChanged(RemoteAuthKind),
    RemoteProfileKeyPathChanged(String),
    RemoteAddProfilePressed,
    RemoteDeleteProfilePressed(usize), // profile id

    // remote development (SSH) - connecting
    RemoteConnectPressed(usize), // profile id
    RemoteConnectSecretChanged(String),
    RemoteConnectConfirmed,
    RemoteConnectCancelled,
    RemoteConnected(usize, Result<Arc<rustrest_remote::RemoteSession>, String>), // profile id
    RemoteDisconnectPressed(usize),                                              // profile id

    // remote development (SSH) - terminal
    RemoteOpenTerminalPressed(usize), // profile id
    /// a `ShellChannel` isn't `Clone`, but `Message` derives it; the
    /// mutex-guarded option is a take-once box so this variant can still be
    /// constructed once and handled once, same as any other message.
    RemoteShellReady(
        usize,
        Result<Arc<std::sync::Mutex<Option<rustrest_remote::ShellChannel>>>, String>,
    ),

    // remote development (SSH) - file explorer
    RemoteExplorerToggled(usize), // profile id
    RemoteExplorerPathChanged(usize, String),
    RemoteExplorerGoPressed(usize),
    RemoteDirListingLoaded(
        usize,
        String,
        Result<Vec<rustrest_remote::RemoteEntry>, String>,
    ),
    RemoteEntryClicked(usize, String), // profile id, absolute path
    RemoteFileLoaded(usize, String, Result<Vec<u8>, String>), // profile id, path, bytes

    // remote development (SSH) - remote collections
    /// import an existing remote directory (in the dir-collection format) as
    /// a collection. profile id, absolute remote path.
    RemoteImportDirAsCollectionPressed(usize, String),
    RemoteCollectionImported(usize, String, Result<Box<PostmanCollection>, String>), // profile id, root path, result
    /// (re)loads a remote-backed collection already in `app.collections`
    /// (e.g. right after connecting). collection id, result.
    RemoteCollectionLoaded(usize, Result<Box<PostmanCollection>, String>),
    RemoteNewCollectionNameChanged(usize, String), // profile id, name
    RemoteNewCollectionPressed(usize),             // profile id

    // remote development (SSH) - open remote file tab
    RemoteFileContentChanged(usize, iced::widget::text_editor::Action), // tab id
    RemoteFileSavePressed(usize),                                       // tab id
    RemoteFileSaved(usize, Result<(), String>),                         // tab id

    // command palette (Ctrl+Shift+P)
    ToggleCommandPalette,
    CommandPaletteQueryChanged(String),
    CommandPaletteMoveSelection(i32),
    CommandPaletteConfirm,
    CommandPaletteClosed,
    CommandPaletteItemClicked(AppCommand),

    // remote development (SSH) - configuration modal
    OpenRemoteConfig,
    CloseRemoteConfigPressed,
    WindowCloseRequested(iced::window::Id),

    // native plugins (wasm)
    /// plugin_id, command_id - dispatched from the command palette or menu.
    PluginCommand(String, String),
    /// plugin_id, panel_id, event - a widget interaction inside an open
    /// plugin panel tab.
    PluginPanelEvent(String, String, rustrest_plugin_host::UiEvent),
    /// opens (or focuses) a tab for the given plugin's sidebar panel.
    OpenPluginPanel(String, String),
    /// opens (or focuses) the "Manage Plugins" tab.
    OpenPluginManagerPressed,
    TogglePluginEnabled(String, bool),
    /// opens a folder dialog to pick a local plugin directory (containing
    /// `plugin.toml` + `plugin.wasm`) to install.
    InstallPluginPressed,
    /// the folder picked by `InstallPluginPressed` (or `None` if cancelled).
    PluginInstallFolderPicked(Option<std::path::PathBuf>),
    /// a background thread finished compiling the picked plugin's wasm and
    /// copying it into the plugins directory (or failed)
    PluginInstallPrepared(
        Result<
            (
                String,
                rustrest_plugin_host::PluginManifest,
                rustrest_plugin_host::Module,
            ),
            String,
        >,
    ),
    /// driven by a timer; drains buffered output from any process a plugin
    /// spawned via the `ExternalProcess` capability and delivers it into the
    /// owning plugin. Also drains any outbound HTTP requests started via
    /// `http_request` (same capability).
    PluginProcessTick,
    /// opens the given plugin's right panel, or closes it if it's already
    /// the one open (a single generic toggle, not specific to any plugin).
    ToggleRightPanel(String, String),
    /// plugin_id, panel_id, event - a widget interaction inside the open
    /// right panel.
    RightPanelEvent(String, String, rustrest_plugin_host::UiEvent),
    /// applies a collection-tree operation a `RightPanel` plugin proposed
    /// (immediately for non-destructive ops, or after the user accepts the
    /// confirm dialog shown for destructive ones).
    ApplyPluginCollectionOp(rustrest_plugin_host::CollectionOperation),
    /// plugin_id - shows a confirmation dialog before uninstalling.
    UninstallPluginPressed(String),
    /// plugin_id - the uninstall confirmation was accepted.
    UninstallPluginConfirmed(String),
    /// plugin_id, result - a background thread finished deleting the
    /// plugin's directory from disk (or failed).
    PluginUninstallFinished(String, Result<(), String>),

    /// switches the Manage Plugins tab between the installed list and the
    /// remote gallery; fetches the index the first time Browse is shown.
    ShowPluginManagerView(crate::ui::plugin_manager::PluginManagerView),
    /// fetches the remote plugin gallery index.
    FetchPluginGallery,
    /// a background thread finished fetching the gallery index (or failed).
    GalleryIndexFetched(Result<Vec<GalleryEntry>, String>),
    /// the "Install" button was pressed for a gallery entry.
    InstallFromGalleryPressed(GalleryEntry),
    /// the Manage Plugins search box changed; filters whichever of
    /// Installed/Browse is currently showing.
    PluginManagerSearchChanged(String),

    /// plugin_id, format_id, file-picker extensions - opens a file dialog
    /// and routes the picked file through `PluginManager::import`.
    ImportCollectionViaPluginPressed(String, String, Vec<String>),
    /// plugin_id, format_id, path, raw file bytes - the file picked by
    /// `ImportCollectionViaPluginPressed` has been read from disk.
    PluginImportFileLoaded(String, String, Option<std::path::PathBuf>, Vec<u8>),
    /// collection_id, plugin_id, format_id, file-picker extensions.
    ExportCollectionViaPluginPressed(usize, String, String, Vec<String>),
    /// collection_id - either exports directly (a single installed
    /// plugin/format) or opens `export_plugin_picker` (more than one).
    ExportViaPluginPressed(usize),
    CloseExportPluginPicker,

    // settings
    OpenSettingsPressed,
    CloseSettingsPressed,
    SettingsTabSelected(crate::ui::settings::SettingsTab),
    ThemeSelected(crate::ui::settings::AppTheme),
    CloseOnOutsideClickToggled(bool),

    // new tab protocol picker
    ShowNewTabMenu,
    NewProtocolTabPressed(NewTabProtocol),

    // websocket
    ActiveWsMessage(crate::ui::tab::ws::WsTabMessage),
    WsEvent(usize, rustrest_ws::WsEvent),
    WsClosed(usize),

    // server-sent events (SSE): a streaming mode on a regular HTTP request tab
    SseEvent(usize, rustrest_sse::SseEvent),

    // graphql
    ActiveGraphQlMessage(crate::ui::tab::graphql::GraphQlTabMessage),
    GraphQlResponseReceived(usize, Result<rustrest_graphql::GraphQlResponse, String>),
    GraphQlSchemaLoaded(usize, Result<rustrest_graphql::GraphQlResponse, String>),
    GraphQlSubscriptionEvent(usize, rustrest_graphql::SubscriptionEvent),

    // grpc
    ActiveGrpcMessage(crate::ui::tab::grpc::GrpcTabMessage),
    GrpcDiscovered(usize, Result<rustrest_grpc::GrpcTarget, String>),
    GrpcResponse(usize, Result<String, String>),
    GrpcInvokeFinished(usize),

    /// opens (or focuses) the folder's tab
    SidebarFolderClicked {
        collection_id: usize,
        folder_path: Vec<String>,
    },
    /// routed to the active collection/folder tab's docs pane
    Docs(crate::ui::docs_view::DocsMessage),

    AppExit,
    None,
}
