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
#![allow(clippy::manual_memcpy)]
#![allow(clippy::range_plus_one)]
#![allow(clippy::map_unwrap_or)]

use crate::node::{
    BTreePageHeader, INVALID_PAGE_ID, InternalNode, KeyType, LeafNode, NodeType, ValueType,
    internal_max_keys, leaf_max_keys,
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
    #[error("Invalid or corrupted node encountered")]
    InvalidNode,
    #[error("Duplicate key")]
    DuplicateKey,
    #[error("Root page not initialized")]
    Uninitialized,
}

pub struct BTreeIndex<'a, const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>> {
    buffer_pool: &'a BufferPoolManager<PAGE_SIZE, D>,
    root_page_id: parking_lot::RwLock<Option<PageId>>,
    index_file_id: u32,
}

impl<'a, const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>> BTreeIndex<'a, PAGE_SIZE, D> {
    pub fn new(
        buffer_pool: &'a BufferPoolManager<PAGE_SIZE, D>,
        root_page_id: Option<PageId>,
        index_file_id: u32,
    ) -> Self {
        Self {
            buffer_pool,
            root_page_id: parking_lot::RwLock::new(root_page_id),
            index_file_id,
        }
    }

    pub fn get_root_page_id(&self) -> Option<PageId> {
        *self.root_page_id.read()
    }

    pub fn insert(&self, key: KeyType, value: ValueType) -> Result<(), BTreeError> {
        let root_id_opt = *self.root_page_id.read();

        if root_id_opt.is_none() {
            return self.create_root_and_insert(key, value);
        }

        let (leaf_frame, leaf_page_id) =
            self.find_leaf_page(key)?.ok_or(BTreeError::KeyNotFound)?;
        self.insert_into_leaf(leaf_frame, leaf_page_id, key, value)
    }

    fn create_root_and_insert(&self, key: KeyType, value: ValueType) -> Result<(), BTreeError> {
        let mut root_lock = self.root_page_id.write();
        if root_lock.is_some() {
            // Another thread might have created it
            drop(root_lock);
            return self.insert(key, value);
        }

        let (frame_id, root_page_id) = self.buffer_pool.new_page(self.index_file_id)?;
        let mut page_data = self.buffer_pool.write_page(frame_id);
        let leaf = unsafe { &mut *(page_data.data.as_mut_ptr() as *mut LeafNode) };

        leaf.header.node_type = NodeType::Leaf as u8;
        leaf.header.num_keys = 1;
        leaf.header.max_keys = leaf_max_keys(PAGE_SIZE);
        leaf.header.parent_page_id = INVALID_PAGE_ID;
        leaf.header.next_page_id = INVALID_PAGE_ID;

        let (keys, values) = leaf.keys_and_values_mut();
        keys[0] = key;
        values[0] = value;

        *root_lock = Some(root_page_id);
        drop(page_data);
        self.buffer_pool.unpin_page(root_page_id, true)?;
        Ok(())
    }

    fn insert_into_leaf(
        &self,
        leaf_frame: usize,
        leaf_page_id: PageId,
        key: KeyType,
        value: ValueType,
    ) -> Result<(), BTreeError> {
        let mut page_data = self.buffer_pool.write_page(leaf_frame);
        let leaf = unsafe { &mut *(page_data.data.as_mut_ptr() as *mut LeafNode) };
        let num_keys = leaf.header.num_keys as usize;

        if leaf.keys()[..num_keys].binary_search(&key).is_ok() {
            drop(page_data);
            self.buffer_pool.unpin_page(leaf_page_id, false)?;
            return Err(BTreeError::DuplicateKey);
        }

        if num_keys < leaf.header.max_keys as usize {
            let insert_idx = leaf.keys()[..num_keys]
                .binary_search(&key)
                .unwrap_or_else(|e| e);

            let (keys, values) = leaf.keys_and_values_mut();

            keys.copy_within(insert_idx..num_keys, insert_idx + 1);
            values.copy_within(insert_idx..num_keys, insert_idx + 1);

            keys[insert_idx] = key;
            values[insert_idx] = value;
            leaf.header.num_keys += 1;

            drop(page_data);
            self.buffer_pool.unpin_page(leaf_page_id, true)?;
            return Ok(());
        }

        drop(page_data);
        self.buffer_pool.unpin_page(leaf_page_id, false)?;
        self.split_leaf(leaf_page_id, key, value)
    }

    fn split_leaf(
        &self,
        leaf_page_id: PageId,
        key: KeyType,
        value: ValueType,
    ) -> Result<(), BTreeError> {
        let leaf_frame = self.buffer_pool.fetch_page(leaf_page_id)?;
        let mut leaf_data = self.buffer_pool.write_page(leaf_frame);
        let leaf = unsafe { &mut *(leaf_data.data.as_mut_ptr() as *mut LeafNode) };

        let (rs_frame, rs_page_id) = self.buffer_pool.new_page(leaf_page_id.file_id)?;
        let mut rs_data = self.buffer_pool.write_page(rs_frame);
        let rs_leaf = unsafe { &mut *(rs_data.data.as_mut_ptr() as *mut LeafNode) };

        rs_leaf.header.node_type = NodeType::Leaf as u8;
        rs_leaf.header.max_keys = leaf.header.max_keys;
        rs_leaf.header.parent_page_id = leaf.header.parent_page_id;
        rs_leaf.header.next_page_id = leaf.header.next_page_id;
        leaf.header.next_page_id = rs_page_id;

        let total_keys = leaf.header.num_keys as usize;
        let mid = total_keys / 2;

        let mut temp_keys = Vec::with_capacity(total_keys + 1);
        let mut temp_vals = Vec::with_capacity(total_keys + 1);

        temp_keys.extend_from_slice(&leaf.keys()[..total_keys]);
        temp_vals.extend_from_slice(&leaf.values()[..total_keys]);

        let insert_idx = temp_keys.binary_search(&key).unwrap_or_else(|e| e);
        temp_keys.insert(insert_idx, key);
        temp_vals.insert(insert_idx, value);

        leaf.header.num_keys = mid as u16;
        let (leaf_keys, leaf_values) = leaf.keys_and_values_mut();
        leaf_keys[..mid].copy_from_slice(&temp_keys[..mid]);
        leaf_values[..mid].copy_from_slice(&temp_vals[..mid]);

        let rs_len = temp_keys.len() - mid;
        rs_leaf.header.num_keys = rs_len as u16;

        let (rs_keys, rs_values) = rs_leaf.keys_and_values_mut();
        rs_keys[..rs_len].copy_from_slice(&temp_keys[mid..]);
        rs_values[..rs_len].copy_from_slice(&temp_vals[mid..]);

        let promote_key = rs_leaf.keys()[0];
        let parent_id = leaf.header.parent_page_id;

        drop(leaf_data);
        drop(rs_data);
        let _ = self.buffer_pool.unpin_page(leaf_page_id, true);
        let _ = self.buffer_pool.unpin_page(rs_page_id, true);

        self.insert_into_parent(leaf_page_id, promote_key, rs_page_id, parent_id)
    }

    fn insert_into_parent(
        &self,
        old_node_id: PageId,
        key: KeyType,
        new_node_id: PageId,
        parent_id: PageId,
    ) -> Result<(), BTreeError> {
        if parent_id == INVALID_PAGE_ID {
            return self.create_new_root(old_node_id, key, new_node_id);
        }

        let parent_frame = self.buffer_pool.fetch_page(parent_id)?;
        let mut parent_data = self.buffer_pool.write_page(parent_frame);
        let parent = unsafe { &mut *(parent_data.data.as_mut_ptr() as *mut InternalNode) };

        let num_keys = parent.header.num_keys as usize;

        if num_keys < parent.header.max_keys as usize {
            let insert_idx = parent.keys()[..num_keys]
                .binary_search(&key)
                .unwrap_or_else(|e| e);

            let (keys, children) = parent.keys_and_children_mut();

            keys.copy_within(insert_idx..num_keys, insert_idx + 1);
            children.copy_within(insert_idx + 1..num_keys + 1, insert_idx + 2);

            keys[insert_idx] = key;
            children[insert_idx + 1] = new_node_id;
            parent.header.num_keys += 1;

            drop(parent_data);
            let _ = self.buffer_pool.unpin_page(parent_id, true);
            return Ok(());
        }

        drop(parent_data);
        let _ = self.buffer_pool.unpin_page(parent_id, false);
        self.split_internal(parent_id, key, new_node_id)
    }

    fn split_internal(
        &self,
        parent_id: PageId,
        key: KeyType,
        new_node_id: PageId,
    ) -> Result<(), BTreeError> {
        let parent_frame = self.buffer_pool.fetch_page(parent_id)?;
        let mut parent_data = self.buffer_pool.write_page(parent_frame);
        let parent = unsafe { &mut *(parent_data.data.as_mut_ptr() as *mut InternalNode) };

        let (rs_frame, rs_page_id) = self.buffer_pool.new_page(parent_id.file_id)?;
        let mut rs_data = self.buffer_pool.write_page(rs_frame);
        let rs_internal = unsafe { &mut *(rs_data.data.as_mut_ptr() as *mut InternalNode) };

        rs_internal.header.node_type = NodeType::Internal as u8;
        rs_internal.header.max_keys = parent.header.max_keys;
        rs_internal.header.parent_page_id = parent.header.parent_page_id;

        let num_keys = parent.header.num_keys as usize;
        let mut temp_keys = Vec::with_capacity(num_keys + 1);
        let mut temp_children = Vec::with_capacity(num_keys + 2);

        temp_keys.extend_from_slice(&parent.keys()[..num_keys]);
        temp_children.extend_from_slice(&parent.children()[..num_keys + 1]);

        let insert_idx = temp_keys.binary_search(&key).unwrap_or_else(|e| e);
        temp_keys.insert(insert_idx, key);
        temp_children.insert(insert_idx + 1, new_node_id);

        let total_keys = temp_keys.len();
        let mid = total_keys / 2;

        parent.header.num_keys = mid as u16;
        let (p_keys, p_children) = parent.keys_and_children_mut();
        p_keys[..mid].copy_from_slice(&temp_keys[..mid]);
        p_children[..mid + 1].copy_from_slice(&temp_children[..mid + 1]);

        let promote_key = temp_keys[mid];

        let rs_num_keys = total_keys - mid - 1;
        rs_internal.header.num_keys = rs_num_keys as u16;

        let (rs_keys, rs_children) = rs_internal.keys_and_children_mut();
        rs_keys[..rs_num_keys].copy_from_slice(&temp_keys[mid + 1..]);
        rs_children[..rs_num_keys + 1].copy_from_slice(&temp_children[mid + 1..]);

        let grand_parent_id = parent.header.parent_page_id;

        for i in 0..=rs_num_keys {
            let child_id = rs_internal.children()[i];
            let child_frame = self.buffer_pool.fetch_page(child_id)?;
            let mut child_data = self.buffer_pool.write_page(child_frame);
            let child_header =
                unsafe { &mut *(child_data.data.as_mut_ptr() as *mut BTreePageHeader) };
            child_header.parent_page_id = rs_page_id;
            drop(child_data);
            let _ = self.buffer_pool.unpin_page(child_id, true);
        }

        drop(parent_data);
        drop(rs_data);
        let _ = self.buffer_pool.unpin_page(parent_id, true);
        let _ = self.buffer_pool.unpin_page(rs_page_id, true);

        self.insert_into_parent(parent_id, promote_key, rs_page_id, grand_parent_id)
    }

    fn create_new_root(&self, left: PageId, key: KeyType, right: PageId) -> Result<(), BTreeError> {
        let (root_frame, new_root_id) = self.buffer_pool.new_page(left.file_id)?;
        let mut root_data = self.buffer_pool.write_page(root_frame);
        let root = unsafe { &mut *(root_data.data.as_mut_ptr() as *mut InternalNode) };

        root.header.node_type = NodeType::Internal as u8;
        root.header.max_keys = internal_max_keys(PAGE_SIZE);
        root.header.num_keys = 1;
        root.header.parent_page_id = INVALID_PAGE_ID;

        let (keys, children) = root.keys_and_children_mut();
        keys[0] = key;
        children[0] = left;
        children[1] = right;

        self.update_parent_id(left, new_root_id)?;
        self.update_parent_id(right, new_root_id)?;

        drop(root_data);
        self.buffer_pool.unpin_page(new_root_id, true)?;

        let mut root_lock = self.root_page_id.write();
        *root_lock = Some(new_root_id);
        Ok(())
    }

    fn update_parent_id(&self, child_id: PageId, parent_id: PageId) -> Result<(), BTreeError> {
        let frame = self.buffer_pool.fetch_page(child_id)?;
        let mut data = self.buffer_pool.write_page(frame);
        let header = unsafe { &mut *(data.data.as_mut_ptr() as *mut BTreePageHeader) };
        header.parent_page_id = parent_id;
        drop(data);
        self.buffer_pool.unpin_page(child_id, true)?;
        Ok(())
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
            } else if header.node_type != NodeType::Internal as u8 {
                return Err(BTreeError::InvalidNode);
            }

            let internal_node = unsafe { &*(page_data.data.as_ptr() as *const InternalNode) };
            let num_keys = header.num_keys as usize;

            let child_idx = internal_node.keys()[..num_keys]
                .binary_search(&key)
                .map(|i| i + 1)
                .unwrap_or_else(|i| i);

            let next_page_id = internal_node.children()[child_idx];

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
        let num_keys = leaf_node.header.num_keys as usize;

        let found_val = leaf_node.keys()[..num_keys]
            .binary_search(&key)
            .ok()
            .map(|idx| leaf_node.values()[idx]);

        drop(page_data);
        self.buffer_pool.unpin_page(curr_page_id, false)?;
        found_val.ok_or(BTreeError::KeyNotFound)
    }

    fn merge_leaf(
        &mut self,
        leaf_page_id: wackdb_storage::PageId,
        parent_id: wackdb_storage::PageId,
    ) -> Result<(), crate::node::BTreeError> {
        let parent_frame = self
            .buffer_pool
            .fetch_page(parent_id)
            .map_err(|_| crate::node::BTreeError::InvalidNode)?;
        let mut parent_data = self.buffer_pool.write_page(parent_frame);
        let parent =
            unsafe { &mut *(parent_data.data.as_mut_ptr() as *mut crate::node::InternalNode) };
        let p_keys_count = parent.header.num_keys as usize;

        let children = parent.children();
        let mut leaf_idx = None;
        #[allow(clippy::needless_range_loop)]
        for i in 0..=p_keys_count {
            if children[i] == leaf_page_id {
                leaf_idx = Some(i);
                break;
            }
        }
        let leaf_idx = leaf_idx.ok_or(crate::node::BTreeError::InvalidNode)?;

        let (sibling_idx, is_left_sibling) = if leaf_idx > 0 {
            (leaf_idx - 1, true)
        } else {
            (leaf_idx + 1, false)
        };

        if sibling_idx > p_keys_count {
            drop(parent_data);
            let _ = self.buffer_pool.unpin_page(parent_id, false);
            return Ok(());
        }

        let sibling_page_id = parent.children()[sibling_idx];

        let left_id = if is_left_sibling {
            sibling_page_id
        } else {
            leaf_page_id
        };
        let right_id = if is_left_sibling {
            leaf_page_id
        } else {
            sibling_page_id
        };

        let left_frame = self
            .buffer_pool
            .fetch_page(left_id)
            .map_err(|_| crate::node::BTreeError::InvalidNode)?;
        let right_frame = self
            .buffer_pool
            .fetch_page(right_id)
            .map_err(|_| crate::node::BTreeError::InvalidNode)?;

        let mut left_data = self.buffer_pool.write_page(left_frame);
        let mut right_data = self.buffer_pool.write_page(right_frame);

        let left_leaf =
            unsafe { &mut *(left_data.data.as_mut_ptr() as *mut crate::node::LeafNode) };
        let right_leaf =
            unsafe { &mut *(right_data.data.as_mut_ptr() as *mut crate::node::LeafNode) };

        let left_num = left_leaf.header.num_keys as usize;
        let right_num = right_leaf.header.num_keys as usize;
        let max_keys = left_leaf.header.max_keys as usize;

        if left_num + right_num > max_keys {
            drop(left_data);
            drop(right_data);
            let _ = self.buffer_pool.unpin_page(left_id, false);
            let _ = self.buffer_pool.unpin_page(right_id, false);
            drop(parent_data);
            let _ = self.buffer_pool.unpin_page(parent_id, false);
            return Ok(());
        }

        let (l_keys, l_vals) = left_leaf.keys_and_values_mut();
        let r_keys = right_leaf.keys().to_vec();
        let r_vals = right_leaf.values().to_vec();

        l_keys[left_num..left_num + right_num].copy_from_slice(&r_keys[..right_num]);
        l_vals[left_num..left_num + right_num].copy_from_slice(&r_vals[..right_num]);

        left_leaf.header.num_keys += right_leaf.header.num_keys;
        left_leaf.header.next_page_id = right_leaf.header.next_page_id;

        drop(left_data);
        drop(right_data);

        let _ = self.buffer_pool.unpin_page(left_id, true);
        let _ = self.buffer_pool.unpin_page(right_id, false);

        let delete_idx = if is_left_sibling {
            leaf_idx
        } else {
            sibling_idx
        };
        let key_idx = delete_idx - 1;

        let (p_keys, p_children) = parent.keys_and_children_mut();
        p_keys.copy_within(key_idx + 1..p_keys_count, key_idx);
        p_children.copy_within(delete_idx + 1..p_keys_count + 1, delete_idx);
        parent.header.num_keys -= 1;

        drop(parent_data);
        let _ = self.buffer_pool.unpin_page(parent_id, true);

        Ok(())
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

        let mut curr_page_id = match self
            .find_leaf_page(start_key)
            .map_err(|_| crate::node::BTreeError::KeyNotFound)?
        {
            Some((_frame_id, pid)) => pid,
            None => return Ok(results),
        };

        loop {
            let frame_id = self
                .buffer_pool
                .fetch_page(curr_page_id)
                .map_err(|_| crate::node::BTreeError::KeyNotFound)?;
            let page_data = self.buffer_pool.read_page(frame_id);
            let leaf_node = unsafe { &*(page_data.data.as_ptr() as *const crate::node::LeafNode) };
            let num_keys = leaf_node.header.num_keys as usize;

            let mut stop = false;
            for i in 0..num_keys {
                let k = leaf_node.keys()[i];
                if k >= start_key && k <= end_key {
                    results.push(leaf_node.values()[i]);
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
        let root_id = *self.root_page_id.read();

        if let Ok(idx) = leaf.keys()[..num_keys].binary_search(&key) {
            let (keys, values) = leaf.keys_and_values_mut();
            keys.copy_within(idx + 1..num_keys, idx);
            values.copy_within(idx + 1..num_keys, idx);
            leaf.header.num_keys -= 1;

            let current_num = leaf.header.num_keys;
            let max_keys = leaf.header.max_keys;
            let parent_id = leaf.header.parent_page_id;
            let is_root = Some(leaf_page_id) == root_id;

            drop(page_data);
            let _ = self.buffer_pool.unpin_page(leaf_page_id, true);

            if !is_root && current_num < max_keys / 2 {
                let _ = self.merge_leaf(leaf_page_id, parent_id);
            }

            Ok(())
        } else {
            drop(page_data);
            let _ = self.buffer_pool.unpin_page(leaf_page_id, false);
            Err(crate::node::BTreeError::KeyNotFound)
        }
    }
}

#[allow(
    clippy::unwrap_used,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::expect_used
)]
#[cfg(test)]
pub mod tests {
    use super::*;
    use tempfile::tempdir;
    use wackdb_storage::disk_manager::BasicDiskManager;

    const TEST_PAGE_SIZE: usize = 8192;

    #[test]
    fn test_btree_insert_and_search() {
        let dir = tempdir().unwrap();
        let disk = BasicDiskManager::<TEST_PAGE_SIZE>::new(dir.path()).unwrap();
        let buffer = BufferPoolManager::new(10, disk);
        let mut btree = BTreeIndex::new(&buffer, None, 1);

        let ctid1 = wackdb_storage::CTID {
            page_id: wackdb_storage::PageId {
                file_id: 1,
                page_num: 1,
            },
            slot_idx: 1,
        };
        let ctid2 = wackdb_storage::CTID {
            page_id: wackdb_storage::PageId {
                file_id: 2,
                page_num: 2,
            },
            slot_idx: 2,
        };

        crate::traits::Index::insert(&mut btree, 10, ctid1).unwrap();
        crate::traits::Index::insert(&mut btree, 20, ctid2).unwrap();

        assert_eq!(crate::traits::Index::search(&btree, 10).unwrap(), ctid1);
        assert_eq!(crate::traits::Index::search(&btree, 20).unwrap(), ctid2);
    }

    #[test]
    fn test_btree_split() {
        let dir = tempdir().unwrap();
        let disk = BasicDiskManager::<TEST_PAGE_SIZE>::new(dir.path()).unwrap();
        let buffer = BufferPoolManager::new(50, disk);
        let mut btree = BTreeIndex::new(&buffer, None, 1);

        // For 8KB page, leaf_max_keys is ~680
        for i in 0u16..800 {
            let ctid = wackdb_storage::CTID {
                page_id: wackdb_storage::PageId {
                    file_id: i as u32,
                    page_num: i as u32,
                },
                slot_idx: i,
            };
            crate::traits::Index::insert(&mut btree, i as i32, ctid).unwrap();
        }

        let val_0 = crate::traits::Index::search(&btree, 0).unwrap();
        assert_eq!(val_0.page_id.file_id, 0);

        let val_799 = crate::traits::Index::search(&btree, 799).unwrap();
        assert_eq!(val_799.page_id.file_id, 799);
    }

    #[test]
    fn test_btree_internal_split() {
        let dir = tempdir().unwrap();
        let disk = BasicDiskManager::<TEST_PAGE_SIZE>::new(dir.path()).unwrap();
        let buffer = BufferPoolManager::new(2000, disk);
        let mut btree = BTreeIndex::new(&buffer, None, 1);

        let total_elements = 60_000;
        for i in 0..total_elements {
            let ctid = wackdb_storage::CTID {
                page_id: wackdb_storage::PageId {
                    file_id: i as u32,
                    page_num: i as u32,
                },
                slot_idx: (i % 100) as u16,
            };
            crate::traits::Index::insert(&mut btree, i, ctid).unwrap();
        }

        let val_0 = crate::traits::Index::search(&btree, 0).unwrap();
        assert_eq!(val_0.page_id.file_id, 0);

        let val_half = crate::traits::Index::search(&btree, 30_000).unwrap();
        assert_eq!(val_half.page_id.file_id, 30_000);

        let val_last = crate::traits::Index::search(&btree, 59_999).unwrap();
        assert_eq!(val_last.page_id.file_id, 59_999);
    }

    #[test]
    fn test_btree_delete() {
        let dir = tempdir().unwrap();
        let disk = BasicDiskManager::<TEST_PAGE_SIZE>::new(dir.path()).unwrap();
        let buffer = BufferPoolManager::new(10, disk);
        let mut btree = BTreeIndex::new(&buffer, None, 1);

        let ctid1 = wackdb_storage::CTID {
            page_id: wackdb_storage::PageId {
                file_id: 1,
                page_num: 1,
            },
            slot_idx: 1,
        };
        let ctid2 = wackdb_storage::CTID {
            page_id: wackdb_storage::PageId {
                file_id: 2,
                page_num: 2,
            },
            slot_idx: 2,
        };

        crate::traits::Index::insert(&mut btree, 10, ctid1).unwrap();
        crate::traits::Index::insert(&mut btree, 20, ctid2).unwrap();

        assert_eq!(crate::traits::Index::search(&btree, 10).unwrap(), ctid1);
        crate::traits::Index::delete(&mut btree, 10).unwrap();
        assert!(crate::traits::Index::search(&btree, 10).is_err());
        assert_eq!(crate::traits::Index::search(&btree, 20).unwrap(), ctid2);
    }

    #[test]
    fn test_btree_persistence() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let ctid1 = wackdb_storage::CTID {
            page_id: wackdb_storage::PageId {
                file_id: 1,
                page_num: 1,
            },
            slot_idx: 1,
        };

        let root_id;

        {
            let disk = BasicDiskManager::<TEST_PAGE_SIZE>::new(&db_path).unwrap();
            let buffer = BufferPoolManager::new(10, disk);
            let mut btree = BTreeIndex::new(&buffer, None, 1);

            crate::traits::Index::insert(&mut btree, 10, ctid1).unwrap();
            root_id = *btree.root_page_id.read();
            buffer.flush_all_pages().unwrap();
        }

        {
            let disk = BasicDiskManager::<TEST_PAGE_SIZE>::new(&db_path).unwrap();
            let buffer = BufferPoolManager::new(10, disk);
            let btree = BTreeIndex::new(&buffer, root_id, 1);

            let result = crate::traits::Index::search(&btree, 10).unwrap();
            assert_eq!(result, ctid1);
        }
    }
}
