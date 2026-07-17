#![allow(clippy::all)]
use crate::{Executor, QueryError};
use std::io::{Read, Seek, SeekFrom, Write};
use tempfile::tempfile;
use wackdb_tuple::{Schema, Tuple, value::Value};

/// Performs an external merge sort on the child executor's output.
pub struct ExternalMergeSort<E: Executor> {
    child: E,
    runs: Vec<std::fs::File>,
    active_runs: Vec<(std::fs::File, Option<Tuple>)>,
    in_memory: Option<std::vec::IntoIter<Tuple>>,
    initialized: bool,
    sort_col_idx: usize,
    chunk_size: usize,
}

impl<E: Executor> ExternalMergeSort<E> {
    /// Initializes a new sorting executor.
    pub fn new(child: E, sort_col_idx: usize, chunk_size: usize) -> Self {
        Self {
            child,
            runs: Vec::new(),
            active_runs: Vec::new(),
            in_memory: None,
            initialized: false,
            sort_col_idx,
            chunk_size,
        }
    }

    fn init(&mut self) -> Result<(), QueryError> {
        let mut buffer = Vec::new();
        let schema = self.child.schema().clone();

        while let Some(t) = self.child.next()? {
            buffer.push(t);
            if buffer.len() >= self.chunk_size {
                self.sort_buffer(&mut buffer, &schema);
                self.spill_to_disk(&mut buffer)?;
            }
        }

        if self.runs.is_empty() {
            // Fits entirely in memory
            self.sort_buffer(&mut buffer, &schema);
            self.in_memory = Some(buffer.into_iter());
        } else {
            // Spill the remaining tuples
            if !buffer.is_empty() {
                self.sort_buffer(&mut buffer, &schema);
                self.spill_to_disk(&mut buffer)?;
            }

            // Initialize active runs
            for mut file in std::mem::take(&mut self.runs) {
                file.seek(SeekFrom::Start(0))
                    .map_err(|_| QueryError::Execution("Seek failed".into()))?;
                let tuple = Self::read_tuple(&mut file)?;
                self.active_runs.push((file, tuple));
            }
        }

        self.initialized = true;
        Ok(())
    }

    fn sort_buffer(&self, buffer: &mut [Tuple], schema: &Schema) {
        let idx = self.sort_col_idx;
        buffer.sort_by(|a, b| {
            if let (Ok(va), Ok(vb)) = (a.to_values(schema), b.to_values(schema)) {
                if va.len() > idx && vb.len() > idx {
                    match (&va[idx], &vb[idx]) {
                        (Value::Integer(na), Value::Integer(nb)) => na.cmp(nb),
                        (Value::Varchar(na), Value::Varchar(nb)) => na.cmp(nb),
                        (Value::Boolean(na), Value::Boolean(nb)) => na.cmp(nb),
                        _ => std::cmp::Ordering::Equal,
                    }
                } else {
                    std::cmp::Ordering::Equal
                }
            } else {
                std::cmp::Ordering::Equal
            }
        });
    }

    fn spill_to_disk(&mut self, buffer: &mut Vec<Tuple>) -> Result<(), QueryError> {
        let mut file =
            tempfile().map_err(|_| QueryError::Execution("Tempfile creation failed".into()))?;
        for t in buffer.drain(..) {
            let len = t.data.len() as u32;
            file.write_all(&len.to_le_bytes())
                .map_err(|_| QueryError::Execution("Write failed".into()))?;
            file.write_all(&t.data)
                .map_err(|_| QueryError::Execution("Write failed".into()))?;
        }
        self.runs.push(file);
        Ok(())
    }

    fn read_tuple(file: &mut std::fs::File) -> Result<Option<Tuple>, QueryError> {
        let mut len_buf = [0u8; 4];
        match file.read_exact(&mut len_buf) {
            Ok(()) => {
                let len = u32::from_le_bytes(len_buf) as usize;
                let mut data = vec![0u8; len];
                file.read_exact(&mut data)
                    .map_err(|_| QueryError::Execution("Read failed".into()))?;
                Ok(Some(Tuple { data }))
            }
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Ok(None),
            Err(_) => Err(QueryError::Execution("Read failed".into())),
        }
    }
}

impl<E: Executor> Executor for ExternalMergeSort<E> {
    fn next(&mut self) -> Result<Option<Tuple>, QueryError> {
        if !self.initialized {
            self.init()?;
        }

        if let Some(in_mem) = &mut self.in_memory {
            return Ok(in_mem.next());
        }

        let schema = self.child.schema();
        let idx = self.sort_col_idx;

        let mut best_idx: Option<usize> = None;
        for (i, (_, tuple_opt)) in self.active_runs.iter().enumerate() {
            if let Some(t) = tuple_opt {
                if let Some(b_idx) = best_idx {
                    let Some(best_tuple) = self.active_runs[b_idx].1.as_ref() else {
                        unreachable!("Best tuple must exist")
                    };
                    if let (Ok(vt), Ok(vb)) = (t.to_values(schema), best_tuple.to_values(schema)) {
                        if vt.len() > idx && vb.len() > idx {
                            let cmp = match (&vt[idx], &vb[idx]) {
                                (Value::Integer(na), Value::Integer(nb)) => na.cmp(nb),
                                (Value::Varchar(na), Value::Varchar(nb)) => na.cmp(nb),
                                (Value::Boolean(na), Value::Boolean(nb)) => na.cmp(nb),
                                _ => std::cmp::Ordering::Equal,
                            };
                            if cmp == std::cmp::Ordering::Less {
                                best_idx = Some(i);
                            }
                        }
                    }
                } else {
                    best_idx = Some(i);
                }
            }
        }

        if let Some(b_idx) = best_idx {
            let (file, tuple_opt) = &mut self.active_runs[b_idx];
            let ret = tuple_opt.take();
            *tuple_opt = Self::read_tuple(file)?;
            Ok(ret)
        } else {
            Ok(None)
        }
    }

    fn schema(&self) -> &Schema {
        self.child.schema()
    }
}
