//! Fundamental data types for wackdb.

/// Unique identifier for a page in the storage system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct PageId(pub u32);

impl PageId {
    pub fn to_le_bytes(self) -> [u8; 4] {
        self.0.to_le_bytes()
    }

    pub fn from_le_bytes(bytes: [u8; 4]) -> Self {
        Self(u32::from_le_bytes(bytes))
    }
}

/// Unique identifier for a frame in the buffer pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct FrameId(pub u32);

impl FrameId {
    pub fn to_le_bytes(self) -> [u8; 4] {
        self.0.to_le_bytes()
    }

    pub fn from_le_bytes(bytes: [u8; 4]) -> Self {
        Self(u32::from_le_bytes(bytes))
    }
}

/// Unique identifier for a slot within a page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct SlotId(pub u16);

impl SlotId {
    pub fn to_le_bytes(self) -> [u8; 2] {
        self.0.to_le_bytes()
    }

    pub fn from_le_bytes(bytes: [u8; 2]) -> Self {
        Self(u16::from_le_bytes(bytes))
    }
}

/// Unique identifier for a transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct TransactionId(pub u64);

impl TransactionId {
    pub fn to_le_bytes(self) -> [u8; 8] {
        self.0.to_le_bytes()
    }

    pub fn from_le_bytes(bytes: [u8; 8]) -> Self {
        Self(u64::from_le_bytes(bytes))
    }
}

/// Log Sequence Number, used for recovery and logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct Lsn(pub u64);

impl Lsn {
    pub fn to_le_bytes(self) -> [u8; 8] {
        self.0.to_le_bytes()
    }

    pub fn from_le_bytes(bytes: [u8; 8]) -> Self {
        Self(u64::from_le_bytes(bytes))
    }
}

/// Record Identifier, uniquely identifies a record's physical location.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct Rid {
    pub page_id: PageId,
    pub slot_id: SlotId,
}

/// Buffer Tag, identifies a page within the buffer pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct BufferTag {
    pub table_id: u32,
    pub page_id: PageId,
}

/// Index of a segment in the architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct SegmentIndex(pub u32);

impl SegmentIndex {
    pub fn to_le_bytes(self) -> [u8; 4] {
        self.0.to_le_bytes()
    }

    pub fn from_le_bytes(bytes: [u8; 4]) -> Self {
        Self(u32::from_le_bytes(bytes))
    }
}

/// Local page identifier within a segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct LocalPageId(pub u32);

impl LocalPageId {
    pub fn to_le_bytes(self) -> [u8; 4] {
        self.0.to_le_bytes()
    }

    pub fn from_le_bytes(bytes: [u8; 4]) -> Self {
        Self(u32::from_le_bytes(bytes))
    }
}

impl From<u32> for PageId {
    fn from(id: u32) -> Self {
        Self(id)
    }
}

impl From<PageId> for u32 {
    fn from(id: PageId) -> Self {
        id.0
    }
}

impl From<u32> for FrameId {
    fn from(id: u32) -> Self {
        Self(id)
    }
}

impl From<FrameId> for u32 {
    fn from(id: FrameId) -> Self {
        id.0
    }
}

impl From<u16> for SlotId {
    fn from(id: u16) -> Self {
        Self(id)
    }
}

impl From<SlotId> for u16 {
    fn from(id: SlotId) -> Self {
        id.0
    }
}

impl From<u64> for TransactionId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

impl From<TransactionId> for u64 {
    fn from(id: TransactionId) -> Self {
        id.0
    }
}

impl From<u64> for Lsn {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

impl From<Lsn> for u64 {
    fn from(id: Lsn) -> Self {
        id.0
    }
}

impl From<u32> for SegmentIndex {
    fn from(id: u32) -> Self {
        Self(id)
    }
}

impl From<SegmentIndex> for u32 {
    fn from(id: SegmentIndex) -> Self {
        id.0
    }
}

impl From<u32> for LocalPageId {
    fn from(id: u32) -> Self {
        Self(id)
    }
}

impl From<LocalPageId> for u32 {
    fn from(id: LocalPageId) -> Self {
        id.0
    }
}
