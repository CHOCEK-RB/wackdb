use crate::header::{SlottedPageHeader, TupleHeader};
use crate::slot::PageSlot;
use std::mem::size_of;
type TransactionId = u64;
const INVALID_TXN_ID: TransactionId = 0;

/// A wrapper to ensure 8-byte alignment for the page buffer.
/// This prevents SIGSEGV when casting the page data to types with strict alignment requirements (like `BTree` Nodes).
#[repr(C, align(8))]
pub struct Align8<const N: usize>(pub [u8; N]);

impl<const N: usize> std::ops::Deref for Align8<N> {
    type Target = [u8; N];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<const N: usize> std::ops::DerefMut for Align8<N> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Represents a physical memory page formatted with slotted architecture for variable-length records.
pub struct SlottedPage<const PAGE_SIZE: usize> {
    /// The raw byte array representing the physical page, safely aligned to 8 bytes.
    pub data: Box<Align8<PAGE_SIZE>>,
}

impl<const PAGE_SIZE: usize> Default for SlottedPage<PAGE_SIZE> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const PAGE_SIZE: usize> SlottedPage<PAGE_SIZE> {
    /// Creates a new, zeroed, and initialized `SlottedPage`.
    #[must_use]
    pub fn new() -> Self {
        let mut page = Self {
            data: Box::new(Align8([0; PAGE_SIZE])),
        };
        page.init();
        page
    }

    /// Initializes the page header and free space pointers for a brand new page.
    pub fn init(&mut self) {
        let header = SlottedPageHeader {
            log_sequence_number: 0,
            total_slots: 0,
            free_space_lower: size_of::<SlottedPageHeader>() as u16,
            free_space_upper: PAGE_SIZE as u16,
            page_flags: 0,
        };
        unsafe {
            let src = (&raw const header).cast::<u8>();
            let dst = self.data.as_mut_ptr();
            std::ptr::copy_nonoverlapping(src, dst, size_of::<SlottedPageHeader>());
        }
    }

    /// Retrieves the Log Sequence Number (LSN) from the page header.
    /// The LSN tracks the most recent log record that describes a modification to this page.
    #[must_use]
    pub fn get_lsn(&self) -> u64 {
        self.header().log_sequence_number
    }

    /// Updates the Log Sequence Number (LSN) safely in the page header.
    /// This must be called whenever the page is modified.
    pub fn set_lsn(&mut self, lsn: u64) {
        let mut header_copy = *self.header();
        header_copy.log_sequence_number = lsn;
        unsafe {
            let src = (&raw const header_copy).cast::<u8>();
            let dst = self.data.as_mut_ptr();
            std::ptr::copy_nonoverlapping(src, dst, size_of::<SlottedPageHeader>());
        }
    }

    /// Returns an immutable reference to the page header.
    #[must_use]
    pub fn header(&self) -> &SlottedPageHeader {
        unsafe { &*self.data.as_ptr().cast::<SlottedPageHeader>() }
    }

    /// Returns a mutable reference to the page header.
    pub fn header_mut(&mut self) -> &mut SlottedPageHeader {
        unsafe { &mut *self.data.as_mut_ptr().cast::<SlottedPageHeader>() }
    }

    /// Returns an immutable slice of the current slot directory.
    #[must_use]
    pub fn slots(&self) -> &[PageSlot] {
        let header = self.header();
        let start = size_of::<SlottedPageHeader>();
        unsafe {
            std::slice::from_raw_parts(
                self.data[start..].as_ptr().cast::<PageSlot>(),
                header.total_slots as usize,
            )
        }
    }

    /// Returns a mutable slice of the current slot directory.
    pub fn slots_mut(&mut self) -> &mut [PageSlot] {
        let header = self.header();
        let total_slots = header.total_slots;
        let start = size_of::<SlottedPageHeader>();
        unsafe {
            std::slice::from_raw_parts_mut(
                self.data[start..].as_mut_ptr().cast::<PageSlot>(),
                total_slots as usize,
            )
        }
    }

    /// Retrieves the tuple header and payload data for a given slot index.
    #[must_use]
    pub fn get_record(&self, slot_idx: usize) -> Option<(TupleHeader, &[u8])> {
        let slots = self.slots();
        if slot_idx >= slots.len() {
            return None;
        }
        let slot = &slots[slot_idx];
        if slot.length == 0 {
            return None;
        }

        let start = slot.offset as usize;
        let header_end = start + size_of::<TupleHeader>();
        let end = start + slot.length as usize;

        // Prevent panic if reading corrupted or incompatible page data
        if end > PAGE_SIZE || header_end > end {
            return None;
        }

        let tuple_header =
            unsafe { std::ptr::read_unaligned(self.data[start..].as_ptr().cast::<TupleHeader>()) };
        let record_data = &self.data[header_end..end];

        Some((tuple_header, record_data))
    }

    /// Inserts a new record into the page if enough free space exists.
    pub fn insert_record(&mut self, record: &[u8], xmin: TransactionId) -> Option<usize> {
        let header_size = size_of::<TupleHeader>();
        let record_len = record.len();
        let total_len = header_size + record_len;
        let required_space = total_len + size_of::<PageSlot>();

        let mut header_copy = *self.header();
        let free_space = header_copy.free_space_upper - header_copy.free_space_lower;

        if free_space < required_space as u16 {
            return None;
        }

        let slot_idx = header_copy.total_slots as usize;
        let new_offset = header_copy.free_space_upper - total_len as u16;

        let tuple_header = TupleHeader {
            xmin,
            xmax: INVALID_TXN_ID,
        };

        // Write TupleHeader
        unsafe {
            std::ptr::write_unaligned(
                self.data[new_offset as usize..]
                    .as_mut_ptr()
                    .cast::<TupleHeader>(),
                tuple_header,
            );
        }

        // Write record data
        self.data[(new_offset as usize + header_size)..(new_offset as usize + total_len)]
            .copy_from_slice(record);

        // Update page header
        header_copy.total_slots += 1;
        header_copy.free_space_upper = new_offset;
        header_copy.free_space_lower += size_of::<PageSlot>() as u16;

        unsafe {
            let src = (&raw const header_copy).cast::<u8>();
            let dst = self.data.as_mut_ptr();
            std::ptr::copy_nonoverlapping(src, dst, size_of::<SlottedPageHeader>());
        }

        // Update slots array
        let slots = self.slots_mut();
        slots[slot_idx].offset = new_offset;
        slots[slot_idx].length = total_len as u16;

        Some(slot_idx)
    }

    /// Marks a record as logically deleted by setting its `xmax` value.
    pub fn mark_deleted(&mut self, slot_idx: usize, xmax: TransactionId) -> bool {
        let slots = self.slots();
        if slot_idx >= slots.len() {
            return false;
        }
        let slot = &slots[slot_idx];
        if slot.length == 0 {
            return false;
        }

        let start = slot.offset as usize;
        let mut tuple_header =
            unsafe { std::ptr::read_unaligned(self.data[start..].as_ptr().cast::<TupleHeader>()) };
        tuple_header.xmax = xmax;
        unsafe {
            std::ptr::write_unaligned(
                self.data[start..].as_mut_ptr().cast::<TupleHeader>(),
                tuple_header,
            );
        };
        true
    }
}

#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
#[cfg(test)]
mod tests {
    use super::*;

    const TEST_PAGE_SIZE: usize = 8192;

    #[test]
    fn insert_should_return_slot_index() {
        let mut page = SlottedPage::<TEST_PAGE_SIZE>::new();
        let slot = page.insert_record(b"data", 100).unwrap();
        assert_eq!(slot, 0);
    }

    #[test]
    fn get_record_should_return_inserted_data() {
        let mut page = SlottedPage::<TEST_PAGE_SIZE>::new();
        let record = b"Hello, World!";
        let slot = page.insert_record(record, 100).unwrap();

        let (header, retrieved) = page.get_record(slot).unwrap();
        assert_eq!(retrieved, record);
        assert_eq!(header.xmin, 100);
        assert_eq!(header.xmax, INVALID_TXN_ID);
    }

    #[test]
    fn insert_should_update_header_slots() {
        let mut page = SlottedPage::<TEST_PAGE_SIZE>::new();
        page.insert_record(b"A", 100).unwrap();
        page.insert_record(b"B", 100).unwrap();

        assert_eq!(page.header().total_slots, 2);
    }

    #[test]
    fn test_slot_directory_insert_and_read() {
        let mut page = SlottedPage::<TEST_PAGE_SIZE>::new();
        let payload1 = b"variable_length_record_1";
        let payload2 = b"more_data_here";

        let slot1 = page
            .insert_record(payload1, 100)
            .expect("failed to insert variable-length record 1");
        let slot2 = page
            .insert_record(payload2, 101)
            .expect("failed to insert variable-length record 2");

        assert_eq!(slot1, 0);
        assert_eq!(slot2, 1);

        let (_, rec1) = page.get_record(slot1).expect("failed to read record 1");
        assert_eq!(rec1, payload1);
        let (_, rec2) = page.get_record(slot2).expect("failed to read record 2");
        assert_eq!(rec2, payload2);

        let header = page.header();
        assert_eq!(header.total_slots, 2);
        assert!(header.free_space_upper < TEST_PAGE_SIZE as u16);
    }

    #[test]
    fn test_page_lsn_storage() {
        let mut page = SlottedPage::<TEST_PAGE_SIZE>::new();
        page.set_lsn(42);

        let lsn = page.get_lsn();
        assert_eq!(lsn, 42, "the LSN must be preserved in the page structure");
    }
}
