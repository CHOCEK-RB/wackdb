use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use wackdb_buffer::{BufferError, LogManager};

/// A physical Write-Ahead Log implementation with memory buffering.
pub struct DiskLogManager {
    file: Mutex<File>,
    flushed_lsn: AtomicU64,
    next_lsn: AtomicU64,
    log_buffer: Mutex<Vec<(u64, Vec<u8>)>>,
}

impl DiskLogManager {
    /// Creates a new DiskLogManager
    pub fn new<P: AsRef<Path>>(data_dir: P) -> std::io::Result<Self> {
        let wal_path = data_dir.as_ref().join("wackdb.wal");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&wal_path)?;

        Ok(Self {
            file: Mutex::new(file),
            flushed_lsn: AtomicU64::new(0),
            next_lsn: AtomicU64::new(1),
            log_buffer: Mutex::new(Vec::new()),
        })
    }

    /// Read all logs for crash recovery
    pub fn read_all_logs(&self) -> std::io::Result<Vec<Vec<u8>>> {
        let mut file_guard = self.file.lock().unwrap();
        file_guard.seek(SeekFrom::Start(0))?;
        let mut records = Vec::new();

        loop {
            let mut lsn_buf = [0u8; 8];
            if file_guard.read_exact(&mut lsn_buf).is_err() {
                break;
            }
            let _lsn = u64::from_le_bytes(lsn_buf);

            let mut len_buf = [0u8; 4];
            if file_guard.read_exact(&mut len_buf).is_err() {
                break;
            }
            let len = u32::from_le_bytes(len_buf) as usize;

            let mut payload = vec![0u8; len];
            if file_guard.read_exact(&mut payload).is_err() {
                break;
            }
            records.push(payload);
        }

        Ok(records)
    }
}

impl LogManager for DiskLogManager {
    fn flush_up_to(&self, lsn: u64) -> Result<(), BufferError> {
        let current_flushed = self.flushed_lsn.load(Ordering::SeqCst);
        if lsn > current_flushed {
            let mut buf_guard = self
                .log_buffer
                .lock()
                .map_err(|_| BufferError::PageNotFound)?;
            let mut file_guard = self.file.lock().map_err(|_| BufferError::PageNotFound)?;

            let mut new_flushed = current_flushed;
            let mut bytes_to_write = Vec::new();

            // Extract records up to lsn
            let mut retained = Vec::new();
            for (rec_lsn, payload) in buf_guard.drain(..) {
                if rec_lsn <= lsn {
                    bytes_to_write.extend_from_slice(&rec_lsn.to_le_bytes());
                    let len = payload.len() as u32;
                    bytes_to_write.extend_from_slice(&len.to_le_bytes());
                    bytes_to_write.extend_from_slice(&payload);

                    if rec_lsn > new_flushed {
                        new_flushed = rec_lsn;
                    }
                } else {
                    retained.push((rec_lsn, payload));
                }
            }
            *buf_guard = retained;

            if !bytes_to_write.is_empty() {
                file_guard
                    .seek(SeekFrom::End(0))
                    .map_err(|_| BufferError::PageNotFound)?;
                file_guard
                    .write_all(&bytes_to_write)
                    .map_err(|_| BufferError::PageNotFound)?;
                file_guard
                    .sync_all()
                    .map_err(|_| BufferError::PageNotFound)?;
                self.flushed_lsn.store(new_flushed, Ordering::SeqCst);
                println!(
                    "Telemetry: [WAL] Log records flushed up to LSN {}",
                    new_flushed
                );
            }
        }
        Ok(())
    }

    fn append_record(&self, payload: &[u8]) -> Result<u64, BufferError> {
        let lsn = self.next_lsn.fetch_add(1, Ordering::SeqCst);
        let mut buf_guard = self
            .log_buffer
            .lock()
            .map_err(|_| BufferError::PageNotFound)?;
        buf_guard.push((lsn, payload.to_vec()));
        Ok(lsn)
    }

    fn checkpoint(&self) -> Result<(), BufferError> {
        let mut file_guard = self.file.lock().map_err(|_| BufferError::PageNotFound)?;
        file_guard
            .set_len(0)
            .map_err(|_| BufferError::PageNotFound)?;
        file_guard
            .seek(SeekFrom::Start(0))
            .map_err(|_| BufferError::PageNotFound)?;
        Ok(())
    }

    fn log_size(&self) -> u64 {
        let disk_size = match self.file.lock() {
            Ok(file_guard) => file_guard.metadata().map(|m| m.len()).unwrap_or(0),
            Err(_) => 0,
        };

        let buffer_size = match self.log_buffer.lock() {
            Ok(buf_guard) => {
                buf_guard.iter().map(|(_, p)| p.len() as u64 + 12).sum() // 8 for LSN + 4 for len
            }
            Err(_) => 0,
        };

        disk_size + buffer_size
    }
}
