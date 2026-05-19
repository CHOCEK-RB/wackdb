/// A single entry in the slot directory, tracking where a tuple lives in the page.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct PageSlot {
    /// The byte offset within the page where the tuple starts.
    pub offset: u16,
    /// The length of the tuple data in bytes (excluding headers).
    pub length: u16,
}
