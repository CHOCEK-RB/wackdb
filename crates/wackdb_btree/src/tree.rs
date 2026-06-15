#![allow(missing_docs)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::indexing_slicing)]
#![allow(clippy::cast_ptr_alignment)]
#![allow(clippy::ptr_as_ptr)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::unnecessary_lazy_evaluations)]
#![allow(clippy::elidable_lifetime_names)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::unimplemented)]

use crate::node::{
    BTreePageHeader, InternalNode, KeyType, LeafNode, NodeType, ValueType, INVALID_PAGE_ID,
};
use thiserror::Error;
use wackdb_buffer::buffer_pool::BufferPoolManager;
use wackdb_storage::{DiskManager, PageId};

#[derive(Error, Debug)]
pub enum BTreeError {
    #[error("Buffer pool error: {0}")]
    BufferError(#[from] wackdb_buffer::BufferError),
    #[error("Key not found")]
    KeyNotFound,
    #[error("Duplicate key")]
    DuplicateKey,
    #[error("Root page not initialized")]
    Uninitialized,
}

pub struct BTreeIndex<'a, const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>> {
    buffer_pool: &'a BufferPoolManager<PAGE_SIZE, D>,
    root_page_id: parking_lot::RwLock<Option<PageId>>,
}

impl<'a, const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>> BTreeIndex<'a, PAGE_SIZE, D> {
    pub fn new(
        buffer_pool: &'a BufferPoolManager<PAGE_SIZE, D>,
        root_page_id: Option<PageId>,
    ) -> Self {
        Self {
            buffer_pool,
            root_page_id: parking_lot::RwLock::new(root_page_id),
        }
    }

    pub fn insert(&self, key: KeyType, value: ValueType) -> Result<(), BTreeError> {
        let mut root_lock = self.root_page_id.write();
        let root_id = *root_lock;
        if root_id.is_none() {
            // Create root leaf page
            let (frame_id, root_page_id) = self.buffer_pool.new_page(0)?;
            let mut page_data = self.buffer_pool.write_page(frame_id);
            let leaf = unsafe { &mut *(page_data.data.as_mut_ptr() as *mut LeafNode) };

            leaf.header.node_type = NodeType::Leaf as u8;
            leaf.header.num_keys = 0;
            let max_keys = (PAGE_SIZE - std::mem::size_of::<BTreePageHeader>())
                / (std::mem::size_of::<KeyType>() + std::mem::size_of::<ValueType>());
            leaf.header.max_keys = max_keys as u16;
            leaf.header.parent_page_id = INVALID_PAGE_ID;
            leaf.header.next_page_id = INVALID_PAGE_ID;

            leaf.keys[0] = key;
            leaf.values[0] = value;
            leaf.header.num_keys = 1;

            *root_lock = Some(root_page_id);

            drop(page_data);
            self.buffer_pool.unpin_page(root_page_id, true)?;
            return Ok(());
        }
        drop(root_lock);

        // Find leaf
        let (leaf_frame, leaf_page_id) =
            self.find_leaf_page(key)?.ok_or(BTreeError::KeyNotFound)?;

        // Pin leaf for writing
        let mut page_data = self.buffer_pool.write_page(leaf_frame);
        let leaf = unsafe { &mut *(page_data.data.as_mut_ptr() as *mut LeafNode) };

        let num_keys = leaf.header.num_keys as usize;

        // Check duplicate
        if leaf.keys[..num_keys].binary_search(&key).is_ok() {
            drop(page_data);
            self.buffer_pool.unpin_page(leaf_page_id, false)?;
            return Err(BTreeError::DuplicateKey);
        }

        // We can just try to insert
        if num_keys < leaf.header.max_keys as usize {
            // Find pos
            let mut insert_idx = num_keys;
            for i in 0..num_keys {
                if leaf.keys[i] > key {
                    insert_idx = i;
                    break;
                }
            }

            // Shift
            for i in (insert_idx..num_keys).rev() {
                leaf.keys[i + 1] = leaf.keys[i];
                leaf.values[i + 1] = leaf.values[i];
            }
            leaf.keys[insert_idx] = key;
            leaf.values[insert_idx] = value;
            leaf.header.num_keys += 1;

            drop(page_data);
            self.buffer_pool.unpin_page(leaf_page_id, true)?;
            return Ok(());
        }

        // Split required!
        drop(page_data);
        self.buffer_pool.unpin_page(leaf_page_id, false)?;
        unimplemented!("B+ Tree page splitting is not yet implemented.");
    }

    fn find_leaf_page(&self, key: KeyType) -> Result<Option<(usize, PageId)>, BTreeError> {
        let root_id = *self.root_page_id.read();
        let mut curr_page_id = match root_id {
            Some(id) => id,
            None => return Ok(None),
        };

        loop {
            let frame_id = self.buffer_pool.fetch_page(curr_page_id)?;
            let page_data = self.buffer_pool.read_page(frame_id);
            let header = unsafe { &*(page_data.data.as_ptr() as *const BTreePageHeader) };

            if header.node_type == NodeType::Leaf as u8 {
                return Ok(Some((frame_id, curr_page_id)));
            }

            let internal_node = unsafe { &*(page_data.data.as_ptr() as *const InternalNode) };
            let num_keys = header.num_keys as usize;
            let mut child_idx = num_keys;
            for i in 0..num_keys {
                if key < internal_node.keys[i] {
                    child_idx = i;
                    break;
                }
            }
            let next_page_id = internal_node.children[child_idx];

            drop(page_data);
            self.buffer_pool.unpin_page(curr_page_id, false)?;
            curr_page_id = next_page_id;
        }
    }

    pub fn search(&self, key: KeyType) -> Result<ValueType, BTreeError> {
        let (leaf_frame_id, curr_page_id) =
            self.find_leaf_page(key)?.ok_or(BTreeError::KeyNotFound)?;

        let page_data = self.buffer_pool.read_page(leaf_frame_id);
        let leaf_node = unsafe { &*(page_data.data.as_ptr() as *const LeafNode) };

        let mut found_val = None;
        let num_keys = leaf_node.header.num_keys as usize;

        if let Ok(idx) = leaf_node.keys[..num_keys].binary_search(&key) {
            found_val = Some(leaf_node.values[idx]);
        }

        drop(page_data);
        self.buffer_pool.unpin_page(curr_page_id, false)?;
        found_val.ok_or(BTreeError::KeyNotFound)
    }
}

impl<'a, const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>> crate::traits::Index
    for BTreeIndex<'a, PAGE_SIZE, D>
{
    fn insert(
        &mut self,
        key: i32,
        ctid: wackdb_storage::CTID,
    ) -> Result<(), crate::node::BTreeError> {
        BTreeIndex::insert(self, key, ctid).map_err(|_| crate::node::BTreeError::NodeFull)
    }

    fn search(&self, key: i32) -> Result<wackdb_storage::CTID, crate::node::BTreeError> {
        BTreeIndex::search(self, key).map_err(|_| crate::node::BTreeError::KeyNotFound)
    }

    fn range_search(
        &self,
        start_key: i32,
        end_key: i32,
    ) -> Result<Vec<wackdb_storage::CTID>, crate::node::BTreeError> {
        let mut results = Vec::new();

        let mut curr_page_id = match self.find_leaf_page(start_key).map_err(|_| crate::node::BTreeError::KeyNotFound)? {
            Some((_frame_id, pid)) => pid,
            None => return Ok(results),
        };

        loop {
            let frame_id = self.buffer_pool.fetch_page(curr_page_id).map_err(|_| crate::node::BTreeError::KeyNotFound)?;
            let page_data = self.buffer_pool.read_page(frame_id);
            let leaf_node = unsafe { &*(page_data.data.as_ptr() as *const crate::node::LeafNode) };
            let num_keys = leaf_node.header.num_keys as usize;

            let mut stop = false;
            for i in 0..num_keys {
                let k = leaf_node.keys[i];
                if k >= start_key && k <= end_key {
                    results.push(leaf_node.values[i]);
                } else if k > end_key {
                    stop = true;
                    break;
                }
            }

            let next_page_id = leaf_node.header.next_page_id;
            drop(page_data);
            let _ = self.buffer_pool.unpin_page(curr_page_id, false);

            if stop || next_page_id == crate::node::INVALID_PAGE_ID {
                break;
            }
            curr_page_id = next_page_id;
        }

        Ok(results)
    }

    fn delete(&mut self, key: i32) -> Result<(), crate::node::BTreeError> {
        let (leaf_frame, leaf_page_id) = self
            .find_leaf_page(key)
            .map_err(|_| crate::node::BTreeError::KeyNotFound)?
            .ok_or_else(|| crate::node::BTreeError::KeyNotFound)?;

        let mut page_data = self.buffer_pool.write_page(leaf_frame);
        let leaf = unsafe { &mut *(page_data.data.as_mut_ptr() as *mut crate::node::LeafNode) };
        let num_keys = leaf.header.num_keys as usize;

        let mut delete_idx = None;
        for i in 0..num_keys {
            if leaf.keys[i] == key {
                delete_idx = Some(i);
                break;
            }
        }

        if let Some(idx) = delete_idx {
            // Shift left
            for i in idx..num_keys - 1 {
                leaf.keys[i] = leaf.keys[i + 1];
                leaf.values[i] = leaf.values[i + 1];
            }
            leaf.header.num_keys -= 1;
            drop(page_data);
            let _ = self.buffer_pool.unpin_page(leaf_page_id, true);
            Ok(())
        } else {
            drop(page_data);
            let _ = self.buffer_pool.unpin_page(leaf_page_id, false);
            Err(crate::node::BTreeError::KeyNotFound)
        }
    }
}
