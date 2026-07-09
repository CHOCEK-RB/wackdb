#![allow(missing_docs)]
#![allow(trivial_casts)]
#![allow(clippy::ptr_as_ptr)]
#![allow(clippy::ref_as_ptr)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::cast_possible_truncation)]
use thiserror::Error;
use wackdb_storage::PageId;

#[derive(Error, Debug)]
pub enum BTreeError {
    #[error("Node is full")]
    NodeFull,
    #[error("Duplicate key")]
    DuplicateKey,
    #[error("Key not found")]
    KeyNotFound,
    #[error("Invalid node")]
    InvalidNode,
}

pub const INVALID_PAGE_ID: PageId = PageId {
    file_id: u32::MAX,
    page_num: u32::MAX,
};

pub type KeyType = i32;
pub type ValueType = wackdb_storage::CTID;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum NodeType {
    Internal = 1,
    Leaf = 2,
}

#[derive(Debug, Clone, Copy)]
#[repr(C, align(4))]
pub struct BTreePageHeader {
    pub node_type: u8, // Casted from NodeType
    pub num_keys: u16,
    pub max_keys: u16,
    pub parent_page_id: PageId,
    pub next_page_id: PageId, // For leaf node linking
}

#[derive(Clone, Copy)]
#[repr(C, align(4))]
pub struct LeafNode {
    pub header: BTreePageHeader,
}

#[derive(Clone, Copy)]
#[repr(C, align(4))]
pub struct InternalNode {
    pub header: BTreePageHeader,
}

#[must_use]
pub const fn leaf_max_keys(page_size: usize) -> u16 {
    let header_size = std::mem::size_of::<BTreePageHeader>();
    let pair_size = std::mem::size_of::<KeyType>() + std::mem::size_of::<ValueType>();
    ((page_size - header_size) / pair_size) as u16
}

#[must_use]
pub const fn internal_max_keys(page_size: usize) -> u16 {
    let header_size = std::mem::size_of::<BTreePageHeader>();
    let pair_size = std::mem::size_of::<KeyType>() + std::mem::size_of::<PageId>();
    ((page_size - header_size - std::mem::size_of::<PageId>()) / pair_size) as u16
}

impl LeafNode {
    pub fn keys_mut(&mut self) -> &mut [KeyType] {
        let max_keys = self.header.max_keys as usize;
        unsafe {
            let ptr = (self as *mut Self).add(1) as *mut KeyType;
            std::slice::from_raw_parts_mut(ptr, max_keys)
        }
    }

    pub fn keys_and_values_mut(&mut self) -> (&mut [KeyType], &mut [ValueType]) {
        let max_keys = self.header.max_keys as usize;
        unsafe {
            let keys_ptr = (self as *mut Self).add(1) as *mut KeyType;
            let values_ptr = keys_ptr.add(max_keys) as *mut ValueType;
            (
                std::slice::from_raw_parts_mut(keys_ptr, max_keys),
                std::slice::from_raw_parts_mut(values_ptr, max_keys),
            )
        }
    }

    pub fn keys(&self) -> &[KeyType] {
        let max_keys = self.header.max_keys as usize;
        unsafe {
            let ptr = (self as *const Self).add(1) as *const KeyType;
            std::slice::from_raw_parts(ptr, max_keys)
        }
    }

    pub fn values(&self) -> &[ValueType] {
        let max_keys = self.header.max_keys as usize;
        unsafe {
            let keys_ptr = (self as *const Self).add(1) as *const KeyType;
            let values_ptr = keys_ptr.add(max_keys) as *const ValueType;
            std::slice::from_raw_parts(values_ptr, max_keys)
        }
    }
}

impl InternalNode {
    pub fn keys_mut(&mut self) -> &mut [KeyType] {
        let max_keys = self.header.max_keys as usize;
        unsafe {
            let ptr = (self as *mut Self).add(1) as *mut KeyType;
            std::slice::from_raw_parts_mut(ptr, max_keys)
        }
    }

    pub fn keys_and_children_mut(&mut self) -> (&mut [KeyType], &mut [PageId]) {
        let max_keys = self.header.max_keys as usize;
        unsafe {
            let keys_ptr = (self as *mut Self).add(1) as *mut KeyType;
            let children_ptr = keys_ptr.add(max_keys) as *mut PageId;
            (
                std::slice::from_raw_parts_mut(keys_ptr, max_keys),
                std::slice::from_raw_parts_mut(children_ptr, max_keys + 1),
            )
        }
    }

    pub fn keys(&self) -> &[KeyType] {
        let max_keys = self.header.max_keys as usize;
        unsafe {
            let ptr = (self as *const Self).add(1) as *const KeyType;
            std::slice::from_raw_parts(ptr, max_keys)
        }
    }

    pub fn children(&self) -> &[PageId] {
        let max_keys = self.header.max_keys as usize;
        unsafe {
            let keys_ptr = (self as *const Self).add(1) as *const KeyType;
            let children_ptr = keys_ptr.add(max_keys) as *const PageId;
            std::slice::from_raw_parts(children_ptr, max_keys + 1)
        }
    }
}
