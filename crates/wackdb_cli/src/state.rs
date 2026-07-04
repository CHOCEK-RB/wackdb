use wackdb_storage::PageId;

pub struct AppState {
    pub data_dir: String,
    pub logs: Vec<String>,
    pub root_id: Option<PageId>,
    pub vertical_scroll: usize,
    pub active_table: Option<String>,
}

impl AppState {
    pub fn new(data_dir: &str) -> Self {
        Self {
            data_dir: data_dir.to_string(),
            logs: vec![],
            root_id: None,
            vertical_scroll: 0,
            active_table: None,
        }
    }

    pub fn log(&mut self, msg: impl Into<String>) {
        let text = msg.into();
        for line in text.lines() {
            self.logs.push(line.to_string());
        }
        self.vertical_scroll = usize::MAX;
    }
}
