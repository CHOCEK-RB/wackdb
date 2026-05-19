use crate::header::{SlottedPageHeader, TupleHeader};
use crate::slot::PageSlot;
use std::mem::size_of;
type TransactionId = u64;
const INVALID_TXN_ID: TransactionId = 0;

/// Represents a physical memory page formatted with slotted architecture for variable-length records.
pub struct SlottedPage<const PAGE_SIZE: usize> {
    /// The raw byte array representing the physical page.
    pub data: Box<[u8; PAGE_SIZE]>,
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
            data: Box::new([0; PAGE_SIZE]),
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

    /// Prints a formatted ASCII table representing the internal structure of this `SlottedPage`.
    ///
    /// This includes the page header fields (LSN, next page ID, total slots, free space boundaries)
    /// and a slot directory table detailing the offset and length of each tuple currently stored.
    pub fn print_page_architecture(&self) {
        let header = self.header();
        println!("\n[ Page Header ]");
        println!("+-------------------------+-----------------+");
        println!("| Field                   | Value           |");
        println!("+-------------------------+-----------------+");
        println!(
            "| LSN (Log Sequence Num)  | {:<15} |",
            header.log_sequence_number
        );
        println!("| Total Slots             | {:<15} |", header.total_slots);
        println!(
            "| Free Space Lower        | {:<15} |",
            header.free_space_lower
        );
        println!(
            "| Free Space Upper        | {:<15} |",
            header.free_space_upper
        );
        println!("| Page Flags              | {:<15} |", header.page_flags);
        println!("+-------------------------+-----------------+");

        let slots = self.slots();
        println!("\n[ Slot Directory ]");
        println!("+---------+---------+---------+");
        println!("| Slot ID | Offset  | Length  |");
        println!("+---------+---------+---------+");
        for (i, slot) in slots.iter().enumerate().take(header.total_slots as usize) {
            println!(
                "| {:02}      | {:<7} | {:<7} |",
                i, slot.offset, slot.length
            );
        }
        println!("+---------+---------+---------+");
    }
}

#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
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
}
