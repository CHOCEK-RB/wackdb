use crate::types::{FrameId, Lsn, PageId, SlotId, TransactionId};

pub const PAGE_SIZE: usize = 8192;

pub const INVALID_PAGE_ID: PageId = PageId(u32::MAX);

pub const INVALID_FRAME_ID: FrameId = FrameId(u32::MAX);

pub const INVALID_SLOT_ID: SlotId = SlotId(u16::MAX);

pub const INVALID_TXN_ID: TransactionId = TransactionId(0);

pub const INVALID_LSN: Lsn = Lsn(0);
