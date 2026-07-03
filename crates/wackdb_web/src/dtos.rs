//! Data Transfer Objects (DTOs) for the web visualizer.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// DTO representing a Page ID.
#[derive(Serialize)]
pub struct PageIdDto {
    /// The relation/file ID.
    pub file_id: u32,
    /// The logical page number within the file.
    pub page_num: u32,
}

/// DTO representing a Buffer Pool Frame.
#[derive(Serialize)]
pub struct FrameDto {
    /// The physical frame ID in memory.
    pub frame_id: usize,
    /// The ID of the page currently residing in this frame, if any.
    pub page_id: Option<PageIdDto>,
    /// Number of active pins holding this page in memory.
    pub pin_count: usize,
    /// Whether the page has been modified since it was read from disk.
    pub is_dirty: bool,
}

/// DTO representing the state of the Buffer Pool.
#[derive(Serialize)]
pub struct BufferPoolStateDto {
    /// Total number of successful cache hits.
    pub hits: usize,
    /// Total number of cache misses requiring disk reads.
    pub misses: usize,
    /// Cache hit rate as a percentage or ratio.
    pub hit_rate: f64,
    /// List of all frames currently managed by the buffer pool.
    pub frames: Vec<FrameDto>,
}

/// DTO representing the header of a Slotted Page.
#[derive(Serialize)]
pub struct PageHeaderDto {
    /// Log Sequence Number for recovery.
    pub lsn: u64,
    /// Total number of slots (used and unused).
    pub total_slots: u16,
    /// Byte offset pointing to the end of the slot array.
    pub free_space_lower: u16,
    /// Byte offset pointing to the beginning of the tuple data area.
    pub free_space_upper: u16,
    /// Flags indicating page properties.
    pub page_flags: u16,
}

/// DTO representing a slot within a Slotted Page.
#[derive(Serialize)]
pub struct SlotDto {
    /// The index of the slot.
    pub slot_idx: usize,
    /// Byte offset to the tuple data.
    pub offset: u16,
    /// Length of the tuple data in bytes.
    pub length: u16,
}

/// DTO representing a physical tuple record.
#[derive(Serialize)]
pub struct RecordDto {
    /// The index of the slot this record belongs to.
    pub slot_idx: usize,
    /// Transaction ID that created this record.
    pub xmin: u64,
    /// Transaction ID that deleted this record, if any.
    pub xmax: u64,
    /// Hexadecimal string representation of the tuple data.
    pub data_hex: String,
}

/// DTO representing an entire Slotted Page (Heap Page).
#[derive(Serialize)]
pub struct PageDto {
    /// The page header.
    pub header: PageHeaderDto,
    /// The list of slots in the page.
    pub slots: Vec<SlotDto>,
    /// The list of actual records stored in the page.
    pub records: Vec<RecordDto>,
}

// B-Tree DTOs

/// DTO representing the header of a B-Tree node page.
#[derive(Serialize)]
pub struct BTreePageHeaderDto {
    /// The type of the node ("Leaf" or "Internal").
    pub node_type: String,
    /// Number of active keys in the node.
    pub num_keys: u16,
    /// Maximum number of keys the node can hold.
    pub max_keys: u16,
    /// Page number of the parent node, if applicable.
    pub parent_page_num: Option<u32>,
    /// Page number of the next sibling node, if applicable.
    pub next_page_num: Option<u32>,
}

/// DTO representing the data portion of a B-Tree Leaf node.
#[derive(Serialize)]
pub struct BTreeLeafDataDto {
    /// List of keys in the leaf node.
    pub keys: Vec<i32>,
    /// List of stringified values (e.g., Record IDs) in the leaf node.
    pub values: Vec<String>,
}

/// DTO representing the data portion of a B-Tree Internal node.
#[derive(Serialize)]
pub struct BTreeInternalDataDto {
    /// List of keys in the internal node.
    pub keys: Vec<i32>,
    /// List of child page numbers.
    pub children: Vec<u32>,
}

/// DTO enumerating the possible data payloads of a B-Tree node.
#[derive(Serialize)]
#[serde(tag = "type", content = "data")]
pub enum BTreeNodeDataDto {
    /// A leaf node containing keys and values.
    Leaf(BTreeLeafDataDto),
    /// An internal node containing keys and child pointers.
    Internal(BTreeInternalDataDto),
}

/// DTO representing an entire B-Tree node page.
#[derive(Serialize)]
pub struct BTreePageDto {
    /// The B-Tree page header.
    pub header: BTreePageHeaderDto,
    /// The node's payload data (Leaf or Internal).
    pub node_data: BTreeNodeDataDto,
}

/// DTO representing a raw hexadecimal dump of a buffer frame.
#[derive(Serialize)]
pub struct FrameDumpDto {
    /// The physical frame ID.
    pub frame_id: usize,
    /// The hexadecimal representation of the 8KB page.
    pub hex_dump: String,
}

/// DTO representing table metadata from the catalog.
#[derive(Serialize, Deserialize, Clone)]
pub struct TableMetadataDto {
    /// Name of the table.
    pub name: String,
    /// File ID of the heap file.
    pub heap_relation_id: u32,
    /// File ID of the index file.
    pub index_relation_id: u32,
    /// Page number of the B-Tree index root, if any.
    pub root_page_num: Option<u32>,
}

/// DTO representing the complete database catalog.
#[derive(Serialize, Deserialize)]
pub struct CatalogDataDto {
    /// Next available relation ID.
    pub next_relation_id: u32,
    /// Map of table names to metadata.
    pub tables: HashMap<String, TableMetadataDto>,
}
