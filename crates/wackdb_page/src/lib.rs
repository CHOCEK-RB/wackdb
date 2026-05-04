use std::mem::size_of;
use wackdb_common::constants::PAGE_SIZE;
use wackdb_common::errors::DatabaseError;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

/// Header for a fixed-size database page
/// Total size: 24 bytes
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct PageHeader {
    /// Verification checksum
    pub checksum: u32,
    /// Offset to the end of the ItemId array (start of free space)
    pub lower: u16,
    /// Offset to the start of the data area (end of free space)
    pub upper: u16,
    /// Number of used slots in the ItemId array
    pub slot_count: u16,
    /// Page-level flags (e.g., dirty, special type)
    pub flags: u16,
    /// Reserved space for future use (e.g., LSN, transaction info)
    pub reserved: [u8; 12],
}

/// Pointer to a record within a page
/// Total size: 4 bytes
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct ItemId {
    /// Offset from the start of the page where the record begins
    pub offset: u16,
    /// Length of the record in bytes
    pub length: u16,
}

impl ItemId {
    /// Returns true if the slot is currently used
    pub fn is_used(&self) -> bool {
        self.length > 0
    }
}

/// A wrapper around a raw 8KB page buffer providing slotted page management
pub struct SlottedPage<'a> {
    buffer: &'a mut [u8; PAGE_SIZE],
}

impl<'a> SlottedPage<'a> {
    /// Initializes a new SlottedPage wrapper over an existing buffer
    pub fn new(buffer: &'a mut [u8; PAGE_SIZE]) -> Self {
        Self { buffer }
    }

    /// Initializes the page structure (header and bounds)
    pub fn init(&mut self) {
        let header = self.header_mut();
        header.checksum = 0;
        header.lower = size_of::<PageHeader>() as u16;
        header.upper = PAGE_SIZE as u16;
        header.slot_count = 0;
        header.flags = 0;
        header.reserved = [0u8; 12];
    }

    /// Returns an immutable reference to the page header
    pub fn header(&self) -> &PageHeader {
        PageHeader::ref_from_bytes(&self.buffer[..size_of::<PageHeader>()]).unwrap()
    }

    /// Returns a mutable reference to the page header
    fn header_mut(&mut self) -> &mut PageHeader {
        PageHeader::mut_from_bytes(&mut self.buffer[..size_of::<PageHeader>()]).unwrap()
    }

    /// Returns the number of bytes available for new data
    pub fn free_space(&self) -> usize {
        let header = self.header();
        if header.upper < header.lower {
            return 0;
        }
        (header.upper - header.lower) as usize
    }

    /// Returns an immutable reference to a specific slot
    pub fn get_slot(&self, index: u16) -> Option<&ItemId> {
        let header = self.header();
        if index >= header.slot_count {
            return None;
        }

        let offset = size_of::<PageHeader>() + (index as usize * size_of::<ItemId>());
        ItemId::ref_from_bytes(&self.buffer[offset..offset + size_of::<ItemId>()]).ok()
    }

    /// Returns a mutable reference to a specific slot
    fn get_slot_mut(&mut self, index: u16) -> Option<&mut ItemId> {
        let header = self.header();
        if index >= header.slot_count {
            return None;
        }

        let offset = size_of::<PageHeader>() + (index as usize * size_of::<ItemId>());
        ItemId::mut_from_bytes(&mut self.buffer[offset..offset + size_of::<ItemId>()]).ok()
    }

    /// Inserts a tuple into the page
    /// Returns the slot index (SlotId) if successful
    pub fn insert_tuple(&mut self, data: &[u8]) -> Result<u16, DatabaseError> {
        let tuple_len = data.len();
        let required_space = size_of::<ItemId>() + tuple_len;

        if self.free_space() < required_space {
            return Err(DatabaseError::Storage("Page is full".to_string()));
        }

        let header = self.header_mut();
        let slot_idx = header.slot_count;

        // Records grow backward from the end of the page
        header.upper -= tuple_len as u16;
        header.lower += size_of::<ItemId>() as u16;
        header.slot_count += 1;

        let tuple_offset = header.upper;
        let slot_offset = size_of::<PageHeader>() + (slot_idx as usize * size_of::<ItemId>());

        // Update the ItemId
        let item_id = ItemId::mut_from_bytes(
            &mut self.buffer[slot_offset..slot_offset + size_of::<ItemId>()],
        )
        .unwrap();
        item_id.offset = tuple_offset;
        item_id.length = tuple_len as u16;

        // Write data into the data area
        self.buffer[tuple_offset as usize..tuple_offset as usize + tuple_len].copy_from_slice(data);

        Ok(slot_idx)
    }

    /// Returns the data associated with a specific slot
    pub fn get_tuple(&self, slot_idx: u16) -> Option<&[u8]> {
        let item = self.get_slot(slot_idx)?;
        if !item.is_used() {
            return None;
        }

        let start = item.offset as usize;
        let end = start + item.length as usize;
        Some(&self.buffer[start..end])
    }

    /// Defragment the page by compacting active tuples and reclaiming space from deleted ones
    pub fn compact(&mut self) -> Result<(), DatabaseError> {
        let header = self.header();
        let mut active_slots: Vec<(u16, ItemId)> = Vec::with_capacity(header.slot_count as usize);

        for i in 0..header.slot_count {
            let item = *self.get_slot(i).unwrap();
            if item.is_used() {
                active_slots.push((i, item));
            }
        }

        // Sort active slots by their physical offset in reverse order (closest to the end first)
        // to simplify the backward growth relocation
        active_slots.sort_by_key(|&(_, item)| std::cmp::Reverse(item.offset));

        let mut current_upper = PAGE_SIZE as u16;

        for (idx, _) in active_slots {
            // We need to re-read the item because we update it in the loop
            let item = *self.get_slot(idx).unwrap();
            let data_len = item.length;
            let new_offset = current_upper - data_len;

            // Move data only if the offset changed
            if new_offset != item.offset {
                // Use copy_within to handle potential overlaps safely
                self.buffer.copy_within(
                    item.offset as usize..item.offset as usize + data_len as usize,
                    new_offset as usize,
                );

                // Update the slot in the buffer
                let slot_ptr = self.get_slot_mut(idx).unwrap();
                slot_ptr.offset = new_offset;
            }

            current_upper = new_offset;
        }

        // Update upper bound
        let header = self.header_mut();
        header.upper = current_upper;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slotted_page_compaction() -> Result<(), DatabaseError> {
        let mut buffer = [0u8; PAGE_SIZE];
        let mut page = SlottedPage::new(&mut buffer);
        page.init();

        let slot0 = page.insert_tuple(b"first")?;
        let slot1 = page.insert_tuple(b"second")?;
        let slot2 = page.insert_tuple(b"third")?;

        // Simulate deletion of slot1 by setting length to 0
        {
            let item = page.get_slot_mut(slot1).unwrap();
            item.length = 0;
        }

        assert_eq!(page.get_tuple(slot0).unwrap(), b"first");
        assert!(page.get_tuple(slot1).is_none());
        assert_eq!(page.get_tuple(slot2).unwrap(), b"third");

        // Before compaction, free space doesn't include the "hole" from slot1
        // because we haven't updated lower/upper beyond just ignoring the length.
        let space_before = page.free_space();

        page.compact()?;

        // After compaction, the space from slot1 should be reclaimed.
        // Wait, current compact() only moves data and updates 'upper'.
        // It doesn't update 'lower' or remove the ItemId slot itself.
        // This is consistent with many DBMS where SlotIds are preserved.
        assert_eq!(page.get_tuple(slot0).unwrap(), b"first");
        assert_eq!(page.get_tuple(slot2).unwrap(), b"third");
        assert!(page.free_space() > space_before);

        Ok(())
    }
}
