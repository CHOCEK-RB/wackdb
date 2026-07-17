use colored::Colorize;
use wackdb_catalog::Catalog;
use wackdb_sql::Ast;

/// Executes DDL statements (CREATE TABLE, DROP TABLE)
///
/// # Errors
/// Returns an error if catalog operations fail or schema logic prevents execution.
pub fn execute_ddl(
    ast: Ast,
    catalog: &mut Catalog,
    quiet: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    match ast {
        Ast::CreateTable { table, columns, .. } => {
            if catalog.get_table(&table).is_ok() {
                if !quiet {
                    println!(
                        "{}",
                        format!("[OK] Table '{}' already exists.", table)
                            .bold()
                            .yellow()
                    );
                }
                return Ok(());
            }

            let mut schema_cols = Vec::new();
            for col in columns {
                let dt = match col.data_type.to_uppercase().as_str() {
                    "INTEGER" | "INT" => wackdb_tuple::value::DataType::Integer,
                    "BOOLEAN" | "BOOL" => wackdb_tuple::value::DataType::Boolean,
                    "VARCHAR" | "TEXT" | "STRING" => wackdb_tuple::value::DataType::Varchar,
                    _ => wackdb_tuple::value::DataType::Varchar, // Fallback
                };
                schema_cols.push(wackdb_tuple::Column::new(&col.name, dt, false));
            }

            catalog.create_table_with_schema(&table, wackdb_tuple::Schema::new(schema_cols))?;
            if !quiet {
                println!(
                    "{}",
                    format!("[OK] Table '{}' created.", table).bold().green()
                );
            }
            Ok(())
        }
        Ast::DropTable { table, .. } => {
            if catalog.get_table(&table).is_err() {
                if !quiet {
                    println!(
                        "{}",
                        format!("[OK] Table '{}' does not exist.", table)
                            .bold()
                            .yellow()
                    );
                }
                return Ok(());
            }
            catalog.drop_table(&table)?;
            if !quiet {
                println!(
                    "{}",
                    format!("[OK] Table '{}' dropped.", table).bold().green()
                );
            }
            Ok(())
        }
        _ => Err("Not a DDL statement.".into()),
    }
}
