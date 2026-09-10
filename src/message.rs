use crate::app::CollectionSubTab;
use crate::collection::collection::{PostmanCollection, PostmanRequestNode};
use crate::collection::git_ops::GitStatusSnapshot;
use crate::http_client::HttpResponse;
use crate::ui::confirm_dialog::ConfirmDialogState;
use crate::ui::context_menu::FieldTarget;
use crate::ui::menu::menu::DropdownMessage;
use crate::ui::menu::menu_message::MenuMessage;
use crate::ui::tab::TabMessage;
use crate::ui::toast::toast::ToastStatus;
use crate::updater::UpdateInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeKind {
    Sidebar,
    RequestPane,
    ConsolePanel,
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
    NewTabPressed,
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

    SidebarRequestClicked {
        req_node: PostmanRequestNode,
        collection_id: usize,
        parent_path: Vec<String>,
    },

    // sidebar drag-and-drop (reorder / move requests & folders)
    SidebarDragStarted(SidebarDragItem),
    SidebarDropped(SidebarDropTarget),

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
    EnvVariableValueChanged {
        env_idx: usize,
        var_idx: usize,
        value: String,
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

    //
    ShowCollectionContextMenu(usize),
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
    CopyToClipboard(String),
    PasteIntoField(FieldTarget),
    TextFieldPasteResolved(FieldTarget, Option<String>),

    MenuInteraction(DropdownMessage<MenuMessage>),
    SaveActiveRequestShortcut,

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

    ShowToast(String, ToastStatus),
    DismissToast(usize),
    ToastActionPressed(usize),

    // self update
    CheckForUpdate,
    UpdateCheckResult(Result<Option<UpdateInfo>, String>),
    InstallUpdate,
    UpdateInstallResult(Result<String, String>),

    AppExit,
    None,
}
