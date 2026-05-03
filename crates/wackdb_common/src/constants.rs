//! Common constants for wackdb.

use crate::types::{FrameId, Lsn, PageId, SlotId, TransactionId};

/// Size of a page in bytes (4KB).
pub const PAGE_SIZE: usize = 4096;

/// Invalid PageId constant.
pub const INVALID_PAGE_ID: PageId = PageId(u32::MAX);

/// Invalid FrameId constant.
pub const INVALID_FRAME_ID: FrameId = FrameId(u32::MAX);

/// Invalid SlotId constant.
pub const INVALID_SLOT_ID: SlotId = SlotId(u16::MAX);

/// Invalid TransactionId constant.
pub const INVALID_TXN_ID: TransactionId = TransactionId(0);

/// Invalid LSN constant.
pub const INVALID_LSN: Lsn = Lsn(0);
