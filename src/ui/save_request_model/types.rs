#[derive(Debug, Clone)]
pub struct SaveRequestModalState {
    pub tab_index: usize,
    pub request_name: String,
    pub selected_collection_id: Option<usize>,
    pub selected_folder_path: Vec<String>,
}
