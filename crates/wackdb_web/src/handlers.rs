//! HTTP Handlers for the web server API.

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use std::fs;
use std::io::{Read, Seek};
use std::path::PathBuf;
use wackdb_btree::node::{BTreePageHeader, InternalNode, LeafNode, NodeType};

use crate::dtos::{
    BTreeInternalDataDto, BTreeLeafDataDto, BTreeNodeDataDto, BTreePageDto, BTreePageHeaderDto,
    BufferPoolStateDto, CatalogDataDto, FrameDto, FrameDumpDto, PageDto, PageHeaderDto, PageIdDto,
    RecordDto, SlotDto, TableMetadataDto,
};
use crate::state::{AppState, PAGE_SIZE};

/// Returns the current state of the buffer pool.
pub async fn get_buffer_pool_state(
    State(state): State<AppState>,
) -> (StatusCode, Json<BufferPoolStateDto>) {
    let pool_guard = state.buffer_pool.read();

    let frames_meta = pool_guard.get_frames_metadata();

    let frames = frames_meta
        .into_iter()
        .map(|(frame_id, pid, pin_count, is_dirty)| FrameDto {
            frame_id,
            page_id: pid.map(|p| PageIdDto {
                file_id: p.file_id,
                page_num: p.page_num,
            }),
            pin_count,
            is_dirty,
        })
        .collect();

    let state = BufferPoolStateDto {
        hits: pool_guard.get_hits(),
        misses: pool_guard.get_misses(),
        hit_rate: pool_guard.get_hit_rate(),
        frames,
    };

    (StatusCode::OK, Json(state))
}

/// Returns the raw hex dump of a specific memory frame.
///
/// # Errors
///
/// Returns a status code if the frame cannot be accessed or read.
pub async fn get_frame_content(
    Path(frame_id): Path<usize>,
    State(state): State<AppState>,
) -> Result<Json<FrameDumpDto>, StatusCode> {
    let pool_guard = state.buffer_pool.read();

    // We don't fetch_page, we just read whatever is inside the frame right now
    let page_guard = pool_guard.read_page(frame_id);
    let hex_dump = hex::encode(&page_guard.data.0);

    Ok(Json(FrameDumpDto { frame_id, hex_dump }))
}

/// Returns the list of registered tables from the catalog.
///
/// # Errors
///
/// Returns `INTERNAL_SERVER_ERROR` if the catalog file cannot be read or parsed.
pub async fn get_tables(
    State(state): State<AppState>,
) -> Result<Json<Vec<TableMetadataDto>>, StatusCode> {
    let path = PathBuf::from(&state.data_dir).join("catalog.json");
    if !path.exists() {
        return Ok(Json(vec![]));
    }

    let file = fs::File::open(path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let catalog: CatalogDataDto =
        serde_json::from_reader(file).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut tables: Vec<_> = catalog.tables.into_values().collect();
    tables.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Json(tables))
}

/// Returns the number of heap pages for a given table.
///
/// # Errors
///
/// Returns `NOT_FOUND` if the table is not in the catalog, or `INTERNAL_SERVER_ERROR` on file issues.
pub async fn get_table_pages(
    Path(name): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<u32>, StatusCode> {
    let path = PathBuf::from(&state.data_dir).join("catalog.json");
    if !path.exists() {
        return Err(StatusCode::NOT_FOUND);
    }

    let file = fs::File::open(path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let catalog: CatalogDataDto =
        serde_json::from_reader(file).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let table = catalog.tables.get(&name).ok_or(StatusCode::NOT_FOUND)?;

    let db_path = PathBuf::from(&state.data_dir).join(table.heap_relation_id.to_string());
    if !db_path.exists() {
        return Ok(Json(0));
    }

    let metadata = fs::metadata(db_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let num_pages = u32::try_from(metadata.len() / (PAGE_SIZE as u64)).unwrap_or(0);

    Ok(Json(num_pages))
}

/// Returns the number of index pages (B-Tree nodes) for a given table.
///
/// # Errors
///
/// Returns `NOT_FOUND` if the table is not in the catalog, or `INTERNAL_SERVER_ERROR` on file issues.
pub async fn get_table_index_pages(
    Path(name): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<u32>, StatusCode> {
    let path = PathBuf::from(&state.data_dir).join("catalog.json");
    if !path.exists() {
        return Err(StatusCode::NOT_FOUND);
    }

    let file = fs::File::open(path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let catalog: CatalogDataDto =
        serde_json::from_reader(file).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let table = catalog.tables.get(&name).ok_or(StatusCode::NOT_FOUND)?;

    let db_path = PathBuf::from(&state.data_dir).join(table.index_relation_id.to_string());
    if !db_path.exists() {
        return Ok(Json(0));
    }

    let metadata = fs::metadata(db_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let num_pages = u32::try_from(metadata.len() / (PAGE_SIZE as u64)).unwrap_or(0);

    Ok(Json(num_pages))
}

/// Fetches a heap page from disk and parses its slotted structure.
///
/// # Errors
///
/// Returns `NOT_FOUND` if the file doesn't exist, or `INTERNAL_SERVER_ERROR` on I/O issues.
pub async fn fetch_and_get_page(
    Path((file_id, page_num)): Path<(u32, u32)>,
    State(state): State<AppState>,
) -> Result<Json<PageDto>, StatusCode> {
    let db_path = PathBuf::from(&state.data_dir).join(file_id.to_string());
    if !db_path.exists() {
        return Err(StatusCode::NOT_FOUND);
    }

    let mut file = fs::File::open(db_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    file.seek(std::io::SeekFrom::Start(u64::from(page_num) * 8192))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Use an aligned buffer to avoid panics when dereferencing raw pointers
    let mut aligned_buffer = Box::new(wackdb_page::page::Align8([0u8; 8192]));
    file.read_exact(&mut aligned_buffer.0)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let page_guard = wackdb_page::page::SlottedPage {
        data: aligned_buffer,
    };

    let header = page_guard.header();
    let header_dto = PageHeaderDto {
        lsn: header.log_sequence_number,
        total_slots: header.total_slots,
        free_space_lower: header.free_space_lower,
        free_space_upper: header.free_space_upper,
        page_flags: header.page_flags,
    };

    let mut slots_dto = Vec::new();
    let mut records_dto = Vec::new();

    for (i, slot) in page_guard
        .slots()
        .iter()
        .enumerate()
        .take(header.total_slots as usize)
    {
        slots_dto.push(SlotDto {
            slot_idx: i,
            offset: slot.offset,
            length: slot.length,
        });

        if let Some((tuple_header, data)) = page_guard.get_record(i) {
            records_dto.push(RecordDto {
                slot_idx: i,
                xmin: tuple_header.xmin,
                xmax: tuple_header.xmax,
                data_hex: hex::encode(data),
            });
        }
    }

    Ok(Json(PageDto {
        header: header_dto,
        slots: slots_dto,
        records: records_dto,
    }))
}

/// Fetches a B-Tree page from disk and parses its node structure.
///
/// # Errors
///
/// Returns `NOT_FOUND` if the file doesn't exist, or `INTERNAL_SERVER_ERROR` on I/O issues.
pub async fn fetch_and_get_btree_page(
    Path((file_id, page_num)): Path<(u32, u32)>,
    State(state): State<AppState>,
) -> Result<Json<BTreePageDto>, StatusCode> {
    let db_path = PathBuf::from(&state.data_dir).join(file_id.to_string());
    if !db_path.exists() {
        return Err(StatusCode::NOT_FOUND);
    }

    let mut file = fs::File::open(db_path).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    file.seek(std::io::SeekFrom::Start(u64::from(page_num) * 8192))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Use an aligned buffer to avoid panics when dereferencing raw pointers
    let mut aligned_buffer = Box::new(wackdb_page::page::Align8([0u8; 8192]));
    file.read_exact(&mut aligned_buffer.0)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let btree_dto = {
        let header_ptr = aligned_buffer.0.as_ptr().cast::<BTreePageHeader>();
        let header = unsafe { &*header_ptr };

        let node_type = if header.node_type == NodeType::Leaf as u8 {
            "Leaf"
        } else {
            "Internal"
        };

        let header_dto = BTreePageHeaderDto {
            node_type: node_type.to_string(),
            num_keys: header.num_keys,
            max_keys: header.max_keys,
            parent_page_num: if header.parent_page_id.page_num == u32::MAX {
                None
            } else {
                Some(header.parent_page_id.page_num)
            },
            next_page_num: if header.next_page_id.page_num == u32::MAX {
                None
            } else {
                Some(header.next_page_id.page_num)
            },
        };

        let num_keys = header.num_keys as usize;

        let node_data = if header.node_type == NodeType::Leaf as u8 {
            let leaf = unsafe { &*aligned_buffer.0.as_ptr().cast::<LeafNode>() };
            let mut keys = Vec::with_capacity(num_keys);
            let mut values = Vec::with_capacity(num_keys);
            for i in 0..num_keys {
                keys.push(leaf.keys().get(i).copied().unwrap_or_default());
                let val_page = leaf
                    .values()
                    .get(i)
                    .map(|v| v.page_id.page_num)
                    .unwrap_or_default();
                let val_slot = leaf.values().get(i).map(|v| v.slot_idx).unwrap_or_default();
                values.push(format!("p{val_page}:s{val_slot}"));
            }
            BTreeNodeDataDto::Leaf(BTreeLeafDataDto { keys, values })
        } else {
            let internal = unsafe { &*aligned_buffer.0.as_ptr().cast::<InternalNode>() };
            let mut keys = Vec::with_capacity(num_keys);
            let mut children = Vec::with_capacity(num_keys + 1);
            for i in 0..num_keys {
                keys.push(internal.keys().get(i).copied().unwrap_or_default());
            }
            for i in 0..=num_keys {
                children.push(
                    internal
                        .children()
                        .get(i)
                        .map(|v| v.page_num)
                        .unwrap_or_default(),
                );
            }
            BTreeNodeDataDto::Internal(BTreeInternalDataDto { keys, children })
        };

        BTreePageDto {
            header: header_dto,
            node_data,
        }
    };

    Ok(Json(btree_dto))
}
