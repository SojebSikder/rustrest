use crate::app::CollectionSubTab;
use crate::collection::collection::PostmanRequestNode;
use crate::http_client::HttpResponse;
use crate::tab::TabMessage;

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
    EnvSelected(Option<String>),

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
    None,
}
