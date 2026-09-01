use crate::app::CollectionSubTab;
use crate::collection::collection::{PostmanCollection, PostmanRequestNode};
use crate::http_client::HttpResponse;
use crate::ui::menu::menu::DropdownMessage;
use crate::ui::menu::menu_message::MenuMessage;
use crate::ui::tab::TabMessage;
use crate::ui::toast::toast::ToastStatus;
use crate::updater::UpdateInfo;

#[derive(Debug, Clone)]
pub enum Message {
    TabSelected(usize),
    NewTabPressed,
    SidebarCollectionRootClicked(usize),
    CloseTabPressed(usize),
    ActiveTabMessage(TabMessage),
    SendPressed,
    ResponseReceived(usize, Result<HttpResponse, String>),
    TabNameDoubleClick(usize),
    TabNameChanged(usize, String),
    TabNameSave(usize),
    ImportCollectionPressed,
    ExportCollectionPressed(usize),

    CollectionLoaded(Option<std::path::PathBuf>, String),
    SaveCollectionPressed(usize),

    SidebarRequestClicked(PostmanRequestNode),

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
    SaveRequestPressed(usize), // tab index — opens the chooser
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
        request_id: usize,
    },
    CloseContextMenu,
    MenuInteraction(DropdownMessage<MenuMessage>),
    SaveActiveRequestShortcut,

    // git
    InitGitCollectionPressed(usize), // "Save as git folder" on a collection
    GitCollectionDirChosen(usize, Option<std::path::PathBuf>),
    ImportGitCollectionPressed,
    GitCollectionLoaded(
        Option<std::path::PathBuf>,
        Result<PostmanCollection, String>,
    ),
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
