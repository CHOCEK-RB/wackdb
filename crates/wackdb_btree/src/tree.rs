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

use crate::node::{
    BTreePageHeader, INVALID_PAGE_ID, InternalNode, KeyType, LeafNode, NodeType, ValueType,
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
        let mut root_lock = self.root_page_id.write();
        let root_id = *root_lock;
        if root_id.is_none() {
            // Create root leaf page
            let (frame_id, root_page_id) = self.buffer_pool.new_page(self.index_file_id)?;
            let mut page_data = self.buffer_pool.write_page(frame_id);
            let leaf = unsafe { &mut *(page_data.data.as_mut_ptr() as *mut LeafNode) };

            leaf.header.node_type = NodeType::Leaf as u8;
            leaf.header.num_keys = 0;
            leaf.header.max_keys = crate::node::MAX_KEYS as u16;
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

        self.split_leaf(leaf_page_id, key, value)?;
        Ok(())
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

        // Create right sibling
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

        // Extract all keys temporarily, including the new one
        let mut temp_keys = Vec::with_capacity(total_keys + 1);
        let mut temp_vals = Vec::with_capacity(total_keys + 1);
        for i in 0..total_keys {
            temp_keys.push(leaf.keys[i]);
            temp_vals.push(leaf.values[i]);
        }

        let insert_idx = match temp_keys.binary_search(&key) {
            Ok(_) => return Err(BTreeError::DuplicateKey),
            Err(pos) => pos,
        };
        temp_keys.insert(insert_idx, key);
        temp_vals.insert(insert_idx, value);

        // Distribute between left and right
        leaf.header.num_keys = mid as u16;
        for i in 0..mid {
            leaf.keys[i] = temp_keys[i];
            leaf.values[i] = temp_vals[i];
        }

        rs_leaf.header.num_keys = (temp_keys.len() - mid) as u16;
        for i in mid..temp_keys.len() {
            let rs_idx = i - mid;
            rs_leaf.keys[rs_idx] = temp_keys[i];
            rs_leaf.values[rs_idx] = temp_vals[i];
        }

        let promote_key = rs_leaf.keys[0];
        let parent_id = leaf.header.parent_page_id;

        drop(leaf_data);
        drop(rs_data);
        let _ = self.buffer_pool.unpin_page(leaf_page_id, true);
        let _ = self.buffer_pool.unpin_page(rs_page_id, true);

        self.insert_into_parent(leaf_page_id, promote_key, rs_page_id, parent_id)
    }

    #[allow(clippy::needless_range_loop, clippy::too_many_lines)]
    fn insert_into_parent(
        &self,
        old_node_id: PageId,
        key: KeyType,
        new_node_id: PageId,
        parent_id: PageId,
    ) -> Result<(), BTreeError> {
        if parent_id == INVALID_PAGE_ID {
            // Create a new root
            let (root_frame, new_root_id) = self.buffer_pool.new_page(old_node_id.file_id)?;
            let mut root_data = self.buffer_pool.write_page(root_frame);
            let root = unsafe { &mut *(root_data.data.as_mut_ptr() as *mut InternalNode) };

            root.header.node_type = NodeType::Internal as u8;
            root.header.max_keys = crate::node::MAX_KEYS as u16;
            root.header.num_keys = 1;
            root.header.parent_page_id = INVALID_PAGE_ID;
            root.keys[0] = key;
            root.children[0] = old_node_id;
            root.children[1] = new_node_id;

            // Update children's parent pointers
            let old_frame = self.buffer_pool.fetch_page(old_node_id)?;
            let mut old_data = self.buffer_pool.write_page(old_frame);
            let old_header = unsafe { &mut *(old_data.data.as_mut_ptr() as *mut BTreePageHeader) };
            old_header.parent_page_id = new_root_id;
            drop(old_data);
            let _ = self.buffer_pool.unpin_page(old_node_id, true);

            let new_frame = self.buffer_pool.fetch_page(new_node_id)?;
            let mut new_data = self.buffer_pool.write_page(new_frame);
            let new_header = unsafe { &mut *(new_data.data.as_mut_ptr() as *mut BTreePageHeader) };
            new_header.parent_page_id = new_root_id;
            drop(new_data);
            let _ = self.buffer_pool.unpin_page(new_node_id, true);

            drop(root_data);
            let _ = self.buffer_pool.unpin_page(new_root_id, true);

            let mut root_lock = self.root_page_id.write();
            *root_lock = Some(new_root_id);
            return Ok(());
        }

        let parent_frame = self.buffer_pool.fetch_page(parent_id)?;
        let mut parent_data = self.buffer_pool.write_page(parent_frame);
        let parent = unsafe { &mut *(parent_data.data.as_mut_ptr() as *mut InternalNode) };

        let num_keys = parent.header.num_keys as usize;

        if num_keys < parent.header.max_keys as usize {
            let mut insert_idx = num_keys;
            for i in 0..num_keys {
                if parent.keys[i] > key {
                    insert_idx = i;
                    break;
                }
            }

            for i in (insert_idx..num_keys).rev() {
                parent.keys[i + 1] = parent.keys[i];
                parent.children[i + 2] = parent.children[i + 1];
            }
            parent.keys[insert_idx] = key;
            parent.children[insert_idx + 1] = new_node_id;
            parent.header.num_keys += 1;

            drop(parent_data);
            let _ = self.buffer_pool.unpin_page(parent_id, true);
            return Ok(());
        }

        // Split internal node
        let (rs_frame, rs_page_id) = self.buffer_pool.new_page(parent_id.file_id)?;
        let mut rs_data = self.buffer_pool.write_page(rs_frame);
        let rs_internal = unsafe { &mut *(rs_data.data.as_mut_ptr() as *mut InternalNode) };

        rs_internal.header.node_type = NodeType::Internal as u8;
        rs_internal.header.max_keys = parent.header.max_keys;
        rs_internal.header.parent_page_id = parent.header.parent_page_id;

        // Temporarily store all keys and children
        let mut temp_keys = Vec::with_capacity(num_keys + 1);
        let mut temp_children = Vec::with_capacity(num_keys + 2);

        for i in 0..num_keys {
            temp_keys.push(parent.keys[i]);
            temp_children.push(parent.children[i]);
        }
        temp_children.push(parent.children[num_keys]);

        let mut insert_idx = num_keys;
        for i in 0..num_keys {
            if temp_keys[i] > key {
                insert_idx = i;
                break;
            }
        }
        temp_keys.insert(insert_idx, key);
        temp_children.insert(insert_idx + 1, new_node_id);

        let total_keys = temp_keys.len();
        let mid = total_keys / 2;

        parent.header.num_keys = mid as u16;
        for i in 0..mid {
            parent.keys[i] = temp_keys[i];
            parent.children[i] = temp_children[i];
        }
        parent.children[mid] = temp_children[mid];

        let promote_key = temp_keys[mid];

        let rs_num_keys = total_keys - mid - 1;
        rs_internal.header.num_keys = rs_num_keys as u16;
        for i in 0..rs_num_keys {
            rs_internal.keys[i] = temp_keys[mid + 1 + i];
            rs_internal.children[i] = temp_children[mid + 1 + i];
        }
        rs_internal.children[rs_num_keys] = temp_children[total_keys];

        let grand_parent_id = parent.header.parent_page_id;

        // Update children's parent pointer to the new right sibling
        for i in 0..=rs_num_keys {
            let child_id = rs_internal.children[i];
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

#[cfg(test)]
mod tests {
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

        // Insert
        crate::traits::Index::insert(&mut btree, 10, ctid1).unwrap();
        crate::traits::Index::insert(&mut btree, 20, ctid2).unwrap();

        // Search
        assert_eq!(crate::traits::Index::search(&btree, 10).unwrap(), ctid1);
        assert_eq!(crate::traits::Index::search(&btree, 20).unwrap(), ctid2);
    }

    #[test]
    fn test_btree_split() {
        let dir = tempdir().unwrap();
        let disk = BasicDiskManager::<TEST_PAGE_SIZE>::new(dir.path()).unwrap();
        let buffer = BufferPoolManager::new(50, disk);
        let mut btree = BTreeIndex::new(&buffer, None, 1);

        // Insert enough to trigger a split. Max keys is 340 for 8KB.
        for i in 0u16..400 {
            let ctid = wackdb_storage::CTID {
                page_id: wackdb_storage::PageId {
                    file_id: i as u32,
                    page_num: i as u32,
                },
                slot_idx: i,
            };
            crate::traits::Index::insert(&mut btree, i as i32, ctid).unwrap();
        }

        // Search elements across the split
        let val_0 = crate::traits::Index::search(&btree, 0).unwrap();
        assert_eq!(val_0.page_id.file_id, 0);

        let val_399 = crate::traits::Index::search(&btree, 399).unwrap();
        assert_eq!(val_399.page_id.file_id, 399);
    }

    #[test]
    fn test_btree_internal_split() {
        let dir = tempdir().unwrap();
        let disk = BasicDiskManager::<TEST_PAGE_SIZE>::new(dir.path()).unwrap();
        let buffer = BufferPoolManager::new(2000, disk); // Lots of frames so we don't evict too aggressively while keeping references, wait, we don't hold them all. 2000 is enough.
        let mut btree = BTreeIndex::new(&buffer, None, 1);

        // We need to insert enough elements to cause an internal node split.
        // MAX_KEYS is 340.
        // Each leaf split gives ~170 keys per leaf.
        // So 341 leaf splits * ~170 = ~57,970 elements.
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

        // Let's verify a few elements to ensure the tree is intact.
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

        // Search success
        assert_eq!(crate::traits::Index::search(&btree, 10).unwrap(), ctid1);

        // Delete
        crate::traits::Index::delete(&mut btree, 10).unwrap();

        // Search should fail
        assert!(crate::traits::Index::search(&btree, 10).is_err());

        // But 20 should still exist
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

        // Scope 1: Create, insert, and close
        {
            let disk = BasicDiskManager::<TEST_PAGE_SIZE>::new(&db_path).unwrap();
            let buffer = BufferPoolManager::new(10, disk);
            let mut btree = BTreeIndex::new(&buffer, None, 1);

            crate::traits::Index::insert(&mut btree, 10, ctid1).unwrap();
            root_id = *btree.root_page_id.read();

            // Force flush buffer pool
            buffer.flush_all_pages().unwrap();
        }

        // Scope 2: Reopen and search
        {
            let disk = BasicDiskManager::<TEST_PAGE_SIZE>::new(&db_path).unwrap();
            let buffer = BufferPoolManager::new(10, disk);
            let btree = BTreeIndex::new(&buffer, root_id, 1);

            let result = crate::traits::Index::search(&btree, 10).unwrap();
            assert_eq!(result, ctid1);
        }
    }
}
