pub mod config;
pub mod constants;
pub mod errors;
pub mod types;

pub use config::Config;
pub use constants::{
    INVALID_FRAME_ID, INVALID_LSN, INVALID_PAGE_ID, INVALID_SLOT_ID, INVALID_TXN_ID, PAGE_SIZE,
};
pub use types::{
    BufferTag, FrameId, LocalPageId, Lsn, PageId, Rid, SegmentIndex, SlotId, TransactionId,
};

pub use errors::DatabaseError;

pub type Result<T> = std::result::Result<T, DatabaseError>;
