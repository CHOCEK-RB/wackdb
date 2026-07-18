use comfy_table::{
    Attribute, Cell, Color, Table, modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL,
};
use wackdb_btree::tree::BTreeIndex;
use wackdb_buffer::buffer_pool::BufferPoolManager;
use wackdb_catalog::Catalog;
use wackdb_query::executor::predicate::evaluate_where_clause;
use wackdb_query::{Executor, ExternalMergeSort, NestedLoopJoin, Project, Select};
use wackdb_sql::{Ast, JoinClause, WhereCondition};
use wackdb_storage::DiskManager;
use wackdb_tuple::{Schema, value::Value};

/// Executes SELECT queries via the Volcano execution pipeline.
///
/// # Errors
/// Returns an error if the query pipeline encounters an error or relations are not found.
pub fn execute_query<const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>>(
    ast: Ast,
    catalog: &mut Catalog,
    bpm: &mut BufferPoolManager<PAGE_SIZE, D>,
    sort_chunk_size: usize,
) -> Result<String, Box<dyn std::error::Error>> {
    let Ast::Select {
        columns,
        table,
        where_clause,
        join,
        order_by,
        ..
    } = ast
    else {
        return Err("Not a SELECT statement.".into());
    };

    let mut plan = String::new();
    let mut pipeline = build_base_pipeline(&table, &where_clause, catalog, &*bpm, &mut plan)?;

    if !join.is_empty() {
        pipeline = build_join_pipeline(pipeline, &join, catalog, &*bpm, &mut plan)?;
    }

    if !where_clause.is_empty() {
        pipeline = build_filter_pipeline(pipeline, where_clause, &mut plan)?;
    }

    let output_schema = build_projection_schema(&columns, pipeline.schema());
    if columns.len() != 1 || columns[0] != "*" {
        pipeline = build_projection_pipeline(pipeline, &columns, &output_schema, &mut plan)?;
    }

    if let Some(order_col) = order_by {
        pipeline = build_sort_pipeline(pipeline, &order_col, sort_chunk_size, &mut plan)?;
    }

    print_results(&mut pipeline, &output_schema)?;

    Ok(plan)
}

fn build_sort_pipeline<'a>(
    pipeline: Box<dyn Executor + 'a>,
    order_col: &str,
    sort_chunk_size: usize,
    plan: &mut String,
) -> Result<Box<dyn Executor + 'a>, Box<dyn std::error::Error>> {
    let schema = pipeline.schema();
    let sort_idx = schema
        .columns
        .iter()
        .position(|c| c.name == order_col)
        .ok_or(format!(
            "ORDER BY column '{}' not found in output schema",
            order_col
        ))?;

    plan.push_str(&format!(" -> ExternalMergeSort(col: {})", order_col));
    let exec = ExternalMergeSort::new(pipeline, sort_idx, sort_chunk_size);
    Ok(Box::new(exec))
}

fn build_base_pipeline<'a, const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>>(
    table: &str,
    where_clause: &[WhereCondition],
    catalog: &mut Catalog,
    bpm: &'a BufferPoolManager<PAGE_SIZE, D>,
    plan: &mut String,
) -> Result<Box<dyn Executor + 'a>, Box<dyn std::error::Error>> {
    let meta = catalog.get_table(table)?;
    let schema = catalog.get_schema(table).map_err(|_| "Schema not found")?;

    let index_opt = if let Some(rpn) = meta.root_page_num {
        let index = BTreeIndex::new(
            bpm,
            Some(wackdb_storage::PageId {
                file_id: meta.index_relation_id,
                page_num: rpn,
            }),
            meta.index_relation_id,
        );
        let mut bounds = None;
        if where_clause.len() == 1 {
            let cond = &where_clause[0];
            if cond.left_col == "id" && cond.operator == wackdb_sql::Operator::Eq {
                if let Ok(v) = cond.right_val.parse::<i32>() {
                    bounds = Some((index, v, v));
                }
            }
        }
        bounds
    } else {
        None
    };

    let total_pages = bpm.get_total_pages(meta.heap_relation_id)?;
    let max_pages = if total_pages > 0 { total_pages - 1 } else { 0 };

    let pipeline: Box<dyn Executor> = if let Some((index, start, end)) = index_opt {
        plan.push_str(&format!("IndexScan({})", table));
        wackdb_query::Optimizer::optimize(
            bpm,
            meta.heap_relation_id,
            schema.clone(),
            max_pages,
            Some((&index, start, end)),
        )?
    } else {
        plan.push_str(&format!("SeqScan({})", table));
        wackdb_query::Optimizer::optimize(
            bpm,
            meta.heap_relation_id,
            schema.clone(),
            max_pages,
            None,
        )?
    };

    Ok(pipeline)
}

fn build_join_pipeline<'a, const PAGE_SIZE: usize, D: DiskManager<PAGE_SIZE>>(
    mut left_pipeline: Box<dyn Executor + 'a>,
    joins: &[JoinClause],
    catalog: &mut Catalog,
    bpm: &'a BufferPoolManager<PAGE_SIZE, D>,
    plan: &mut String,
) -> Result<Box<dyn Executor + 'a>, Box<dyn std::error::Error>> {
    for j in joins {
        let meta2 = catalog.get_table(&j.table)?;
        let schema2 = catalog
            .get_schema(&j.table)
            .map_err(|_| "Schema not found for join table")?;
        let total_pages2 = bpm.get_total_pages(meta2.heap_relation_id)?;
        let max_pages2 = if total_pages2 > 0 {
            total_pages2 - 1
        } else {
            0
        };

        plan.push_str(&format!(" -> NestedLoopJoin({})", j.table));
        let right_exec = wackdb_query::Optimizer::optimize(
            bpm,
            meta2.heap_relation_id,
            schema2.clone(),
            max_pages2,
            None,
        )?;

        let left_col_name = j.left_col.clone();
        let right_col_name = j.right_col.clone();

        let pred = Box::new(
            move |lt: &wackdb_tuple::Tuple,
                  ls: &Schema,
                  rt: &wackdb_tuple::Tuple,
                  rs: &Schema|
                  -> bool {
                let Ok(l_vals) = lt.to_values(ls) else {
                    return false;
                };
                let Ok(r_vals) = rt.to_values(rs) else {
                    return false;
                };

                let Some(l_idx) = ls.columns.iter().position(|c| c.name == left_col_name) else {
                    return false;
                };
                let Some(r_idx) = rs.columns.iter().position(|c| c.name == right_col_name) else {
                    return false;
                };

                match (&l_vals[l_idx], &r_vals[r_idx]) {
                    (Value::Integer(a), Value::Integer(b)) => a == b,
                    (Value::Varchar(a), Value::Varchar(b)) => a == b,
                    (Value::Boolean(a), Value::Boolean(b)) => a == b,
                    _ => false,
                }
            },
        );

        left_pipeline = Box::new(NestedLoopJoin::new(left_pipeline, right_exec, pred));
    }

    Ok(left_pipeline)
}

fn build_filter_pipeline<'a>(
    pipeline: Box<dyn Executor + 'a>,
    where_clause: Vec<WhereCondition>,
    plan: &mut String,
) -> Result<Box<dyn Executor + 'a>, Box<dyn std::error::Error>> {
    plan.push_str(" -> Filter");
    let exec = Select::new(
        pipeline,
        Box::new(move |t, s| {
            let Ok(vals) = t.to_values(s) else {
                return false;
            };
            evaluate_where_clause(&where_clause, s, &vals)
        }),
    );
    Ok(Box::new(exec))
}

fn build_projection_schema(columns: &[String], input_schema: &Schema) -> Schema {
    if columns.len() == 1 && columns[0] == "*" {
        return input_schema.clone();
    }

    let mut p_cols = Vec::new();
    for c_name in columns {
        if let Some(col) = input_schema.columns.iter().find(|c| &c.name == c_name) {
            p_cols.push(col.clone());
        }
    }
    Schema::new(p_cols)
}

fn build_projection_pipeline<'a>(
    pipeline: Box<dyn Executor + 'a>,
    columns: &[String],
    output_schema: &Schema,
    plan: &mut String,
) -> Result<Box<dyn Executor + 'a>, Box<dyn std::error::Error>> {
    let mut p_idx = Vec::new();
    for c_name in columns {
        if let Some(idx) = pipeline
            .schema()
            .columns
            .iter()
            .position(|c| &c.name == c_name)
        {
            p_idx.push(idx);
        }
    }

    plan.push_str(" -> Project");
    let exec = Project::new(pipeline, output_schema.clone(), p_idx);
    Ok(Box::new(exec))
}

fn print_results(
    pipeline: &mut Box<dyn Executor + '_>,
    output_schema: &Schema,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS);

    let header_cells: Vec<Cell> = output_schema
        .columns
        .iter()
        .map(|c| {
            Cell::new(&c.name)
                .fg(Color::Blue)
                .add_attribute(Attribute::Bold)
        })
        .collect();
    table.set_header(header_cells);

    let mut row_count = 0;
    while let Some(tuple) = pipeline.next()? {
        let vals = tuple.to_values(output_schema)?;
        let row_cells: Vec<Cell> = vals
            .into_iter()
            .map(|val| match val {
                Value::Integer(i) => Cell::new(i).fg(Color::Yellow),
                Value::Varchar(s) => Cell::new(s).fg(Color::Green),
                Value::Boolean(b) => Cell::new(b).fg(Color::Magenta),
                Value::Null => Cell::new("NULL").fg(Color::DarkGrey),
            })
            .collect();
        table.add_row(row_cells);
        row_count += 1;
    }

    if row_count > 0 {
        println!("{table}");
    } else {
        println!("(Empty set)");
    }
    println!("{} row(s) returned.", row_count);
    Ok(())
}
