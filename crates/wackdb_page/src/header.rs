type TransactionId = u64;

/// Represents the metadata at the beginning of every slotted page.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct SlottedPageHeader {
    /// The log sequence number associated with the latest change to this page.
    pub log_sequence_number: u64,
    /// The total number of slots present in the slot directory.
    pub total_slots: u16,
    /// The offset where the slot directory currently ends.
    pub free_space_lower: u16,
    /// The offset where the tuple data currently begins.
    pub free_space_upper: u16,
    /// Flags indicating page state (e.g. leaf/internal for b-trees).
    pub page_flags: u16,
}

/// The metadata prefixed to every individual tuple/record.
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct TupleHeader {
    /// The transaction ID that inserted this tuple.
    pub xmin: TransactionId,
    /// The transaction ID that deleted this tuple, or 0 if active.
    pub xmax: TransactionId,
}
