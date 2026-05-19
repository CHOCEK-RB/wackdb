/// A physical page identifier composed of a file ID and a page number within that file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PageId {
    /// The unique identifier of the file where this page resides.
    pub file_id: u32,
    /// The page offset within the file.
    pub page_num: u32,
}

/// A physical tuple identifier pointing to a specific record within a slotted page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CTID {
    /// The page where the tuple is stored.
    pub page_id: PageId,
    /// The slot index inside the page.
    pub slot_idx: u16,
}
