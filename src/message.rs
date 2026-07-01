use crate::app::CollectionSubTab;
use crate::collection::PostmanRequestNode;
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
}
