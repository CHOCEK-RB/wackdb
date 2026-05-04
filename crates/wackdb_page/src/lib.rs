use std::mem::size_of;
use wackdb_common::constants::PAGE_SIZE;
use wackdb_common::errors::DatabaseError;
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct PageHeader {
    pub checksum: u32,
    pub lower: u16,
    pub upper: u16,
    pub slot_count: u16,
    pub flags: u16,
    pub reserved: [u8; 12],
}

#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, Immutable, KnownLayout)]
#[repr(C)]
pub struct ItemId {
    pub offset: u16,
    pub length: u16,
}

pub struct SlottedPage<'a> {
    pub buffer: &'a mut [u8; PAGE_SIZE],
}

impl<'a> SlottedPage<'a> {
    pub fn new(buffer: &'a mut [u8; PAGE_SIZE]) -> Self {
        Self { buffer }
    }

    pub fn init(&mut self) {
        let h = self.header_mut();
        h.lower = size_of::<PageHeader>() as u16;
        h.upper = PAGE_SIZE as u16;
        h.slot_count = 0;
    }

    pub fn insert_tuple(&mut self, data: &[u8]) -> Result<u16, DatabaseError> {
        let size = data.len();
        let free = self.free_space();
        if free < size + size_of::<ItemId>() {
            return Err(DatabaseError::Storage("Page is full".to_string()));
        }

        let slot_idx = self.header().slot_count;
        let upper = self.header().upper - size as u16;
        let lower = self.header().lower + size_of::<ItemId>() as u16;

        let slot_offset = size_of::<PageHeader>() + (slot_idx as usize * size_of::<ItemId>());
        let item = ItemId::mut_from_bytes(
            &mut self.buffer[slot_offset..slot_offset + size_of::<ItemId>()],
        )
        .unwrap();
        item.offset = upper;
        item.length = size as u16;

        self.buffer[upper as usize..upper as usize + size].copy_from_slice(data);

        let h = self.header_mut();
        h.upper = upper;
        h.lower = lower;
        h.slot_count += 1;

        Ok(slot_idx)
    }

    pub fn get_tuple(&self, idx: u16) -> Option<&[u8]> {
        if idx >= self.header().slot_count {
            return None;
        }
        let offset = size_of::<PageHeader>() + (idx as usize * size_of::<ItemId>());
        let item =
            ItemId::ref_from_bytes(&self.buffer[offset..offset + size_of::<ItemId>()]).ok()?;
        Some(&self.buffer[item.offset as usize..item.offset as usize + item.length as usize])
    }

    fn free_space(&self) -> usize {
        (self.header().upper - self.header().lower) as usize
    }

    fn header(&self) -> &PageHeader {
        PageHeader::ref_from_bytes(&self.buffer[..size_of::<PageHeader>()]).unwrap()
    }

    fn header_mut(&mut self) -> &mut PageHeader {
        PageHeader::mut_from_bytes(&mut self.buffer[..size_of::<PageHeader>()]).unwrap()
    }
}
