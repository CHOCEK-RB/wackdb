pub mod ddl;
pub mod dml;
pub mod meta;
pub mod query;

use wackdb_buffer::buffer_pool::BufferPoolManager;
use wackdb_catalog::Catalog;
use wackdb_sql::{Ast, parse_sql};
use wackdb_storage::DiskManager;

/// Main entrypoint to process an interactive command or SQL query.
///
/// # Errors
/// Returns an error if parsing or command execution fails.
pub fn process_command<const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>>(
    cmd: &str,
    catalog: &mut Catalog,
    bpm: &mut BufferPoolManager<PAGE_SIZE, D>,
    sort_chunk_size: usize,
    quiet: bool,
    is_recovery: bool,
    verbose: bool,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    if cmd.starts_with('.') {
        meta::execute_meta_command(cmd, catalog, bpm)
    } else {
        let ast = parse_sql(cmd)?;
        match ast {
            Ast::CreateTable { .. } | Ast::DropTable { .. } => {
                ddl::execute_ddl(ast, catalog, quiet)?;
                Ok(None)
            }
            Ast::Insert { .. } | Ast::Delete { .. } => {
                let hits_before = bpm.get_hits();
                let misses_before = bpm.get_misses();

                let is_insert = matches!(ast, Ast::Insert { .. });
                let table_name = match &ast {
                    Ast::Insert { table, .. } => table.clone(),
                    Ast::Delete { table, .. } => table.clone(),
                    _ => unreachable!(),
                };

                dml::execute_dml(ast, catalog, bpm, quiet, is_recovery)?;

                let hits_after = bpm.get_hits();
                let misses_after = bpm.get_misses();
                let delta_hits = hits_after - hits_before;
                let delta_misses = misses_after - misses_before;
                let total = delta_hits + delta_misses;
                let hr = if total > 0 {
                    (delta_hits as f64 / total as f64) * 100.0
                } else {
                    0.0
                };

                let plan = if is_insert {
                    format!("IndexInsert({})", table_name)
                } else {
                    format!("SeqScan({}) -> DeleteExecutor", table_name)
                };

                let telemetry = format!(
                    "\n- Buffer Cache Hits: {}\n- Cache Misses (Disk I/O): {} (Hit Rate: {:.2}%)\n- Execution Pipeline: {}\n",
                    delta_hits, delta_misses, hr, plan
                );

                if verbose {
                    Ok(Some(telemetry))
                } else {
                    Ok(None)
                }
            }
            Ast::Select { .. } => {
                let hits_before = bpm.get_hits();
                let misses_before = bpm.get_misses();

                let plan = query::execute_query(ast, catalog, bpm, sort_chunk_size)?;

                let hits_after = bpm.get_hits();
                let misses_after = bpm.get_misses();
                let delta_hits = hits_after - hits_before;
                let delta_misses = misses_after - misses_before;
                let total = delta_hits + delta_misses;
                let hr = if total > 0 {
                    (delta_hits as f64 / total as f64) * 100.0
                } else {
                    0.0
                };

                let telemetry = format!(
                    "\n[STATISTICS]\n- Buffer Cache Hits: {}\n- Cache Misses (Disk I/O): {} (Hit Rate: {:.2}%)\n- Execution Pipeline: {}\n",
                    delta_hits, delta_misses, hr, plan
                );

                if verbose {
                    Ok(Some(telemetry))
                } else {
                    Ok(None)
                }
            }
        }
    }
}
