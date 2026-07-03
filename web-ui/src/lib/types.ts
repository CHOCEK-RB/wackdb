// Data structure matching the Rust backend DTO
export type PageIdDto = { file_id: number; page_num: number };
export type FrameDto = { frame_id: number; page_id: PageIdDto | null; pin_count: number; is_dirty: boolean };
export type BufferPoolStateDto = { hits: number; misses: number; hit_rate: number; frames: FrameDto[] };

// Page Inspector DTOs
export type PageHeaderDto = { lsn: number; total_slots: number; free_space_lower: number; free_space_upper: number; page_flags: number; };
export type SlotDto = { slot_idx: number; offset: number; length: number; };
export type RecordDto = { slot_idx: number; xmin: number; xmax: number; data_hex: string; };
export type PageDto = { header: PageHeaderDto; slots: SlotDto[]; records: RecordDto[]; };

// Catalog DTOs
export type TableMetadataDto = { name: string; heap_relation_id: number; index_relation_id: number; root_page_num: number | null };

// B-Tree DTOs
export type BTreePageHeaderDto = { node_type: string; num_keys: number; max_keys: number; parent_page_num: number | null; next_page_num: number | null; };
export type BTreeNodeDataDto = 
    | { type: "Leaf", data: { keys: number[]; values: string[] } }
    | { type: "Internal", data: { keys: number[]; children: number[] } };
export type BTreePageDto = { header: BTreePageHeaderDto; node_data: BTreeNodeDataDto; };

export type FrameDumpDto = { frame_id: number; hex_dump: string; };
