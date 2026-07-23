#[derive(Debug, PartialEq, Clone)]
pub enum Operator {
    Eq,
    Neq,
    Gt,
    Gte,
    Lt,
    Lte,
}

#[derive(Debug, PartialEq, Clone)]
pub enum LogicOp {
    And,
    Or,
}

#[derive(Debug, PartialEq, Clone)]
pub struct WhereCondition {
    pub left_col: String,
    pub operator: Operator,
    pub right_val: String,
    pub next_logic: Option<LogicOp>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct ColumnDef {
    pub name: String,
    pub data_type: String,
}

#[derive(Debug, PartialEq)]
pub enum Ast {
    Select {
        columns: Vec<String>, // e.g. ["A.x", "B.y"] or ["*"]
        table: String,
        table_alias: Option<String>,
        join: Vec<JoinClause>,
        where_clause: Vec<WhereCondition>,
        order_by: Option<String>,
    },
    Insert {
        table: String,
        values: Vec<String>,
    },
    CreateTable {
        table: String,
        columns: Vec<ColumnDef>,
        if_not_exists: bool,
    },
    DropTable {
        table: String,
        if_exists: bool,
    },
    Delete {
        table: String,
        where_clause: Vec<WhereCondition>,
    },
}

#[derive(Debug, PartialEq)]
pub struct JoinClause {
    pub table: String,
    pub alias: Option<String>,
    pub left_col: String,
    pub right_col: String,
}

fn normalize_sql(sql: &str) -> String {
    let mut normalized = String::with_capacity(sql.len());
    let mut in_quotes = false;
    let mut in_comment = false;
    let mut last_was_space = false;
    let chars: Vec<char> = sql.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let Some(&c) = chars.get(i) else {
            break;
        };

        if in_comment {
            if c == '\n' {
                in_comment = false;
                if !last_was_space {
                    normalized.push(' ');
                    last_was_space = true;
                }
            }
            i += 1;
            continue;
        }

        if !in_quotes && c == '-' && chars.get(i + 1) == Some(&'-') {
            in_comment = true;
            i += 2;
            continue;
        }

        if c == '\'' {
            in_quotes = !in_quotes;
            normalized.push(c);
            last_was_space = false;
        } else if !in_quotes && c.is_ascii_whitespace() {
            if !last_was_space {
                normalized.push(' ');
                last_was_space = true;
            }
        } else {
            normalized.push(c);
            last_was_space = false;
        }
        i += 1;
    }
    normalized.trim().to_string()
}

pub fn parse_sql(raw_sql: &str) -> Result<Ast, String> {
    let raw_sql = raw_sql.trim().trim_end_matches(';');
    let sql_owned = normalize_sql(raw_sql);
    let sql = sql_owned.as_str();
    let upper = sql.to_uppercase();

    if upper.starts_with("SELECT ") {
        // Find FROM
        let from_idx = upper.find(" FROM ").ok_or("Missing FROM clause")?;
        let columns_str = &sql[7..from_idx].trim();
        let columns: Vec<String> = columns_str
            .split(',')
            .map(|s| {
                let s = s.trim();
                s.split('.').next_back().unwrap_or(s).to_string()
            })
            .collect();

        let rest = &sql[from_idx + 6..].trim();
        let upper_rest = &upper[from_idx + 6..].trim();

        // Check for JOIN
        let join_idx = upper_rest.find(" JOIN ");
        let where_idx = upper_rest.find(" WHERE ");

        let order_by_idx = upper_rest.find(" ORDER BY ");
        let table_end = join_idx.unwrap_or(where_idx.unwrap_or(order_by_idx.unwrap_or(rest.len())));
        let table_part = &rest[..table_end].trim();

        let mut table_tokens = table_part.split_whitespace();
        let table = table_tokens.next().ok_or("Missing table name")?.to_string();
        let table_alias = table_tokens.next().map(|s| s.to_string());

        let mut joins = Vec::new();
        let mut where_clause = Vec::new();
        let mut order_by = None;

        let mut current_pos = table_end;
        while current_pos < rest.len() {
            let next_join = upper_rest[current_pos..].find(" JOIN ");
            let next_where = upper_rest[current_pos..].find(" WHERE ");
            let next_order = upper_rest[current_pos..].find(" ORDER BY ");

            let process_join = match (next_join, next_where, next_order) {
                (Some(j), Some(w), _) if j < w => true,
                (Some(j), None, Some(o)) if j < o => true,
                (Some(_), None, None) => true,
                _ => false,
            };

            if process_join {
                let j_rel_idx = next_join.unwrap();
                let j_idx = current_pos + j_rel_idx;
                let on_idx = upper_rest[j_idx..]
                    .find(" ON ")
                    .ok_or("Missing ON clause in JOIN")?
                    + j_idx;

                let join_table_part = &rest[j_idx + 6..on_idx].trim();
                let mut j_tokens = join_table_part.split_whitespace();
                let j_table = j_tokens
                    .next()
                    .ok_or("Missing join table name")?
                    .to_string();
                let j_alias = j_tokens.next().map(std::string::ToString::to_string);

                // Find end of ON condition
                let next_next_join = upper_rest[on_idx..].find(" JOIN ").map(|i| i + on_idx);
                let on_cond_end = next_next_join.unwrap_or(
                    next_where
                        .or(next_order)
                        .map_or(rest.len(), |i| i + current_pos),
                );

                let on_part = &rest[on_idx + 4..on_cond_end].trim();
                let mut on_tokens = on_part.split('=');
                let left_raw = on_tokens.next().ok_or("Missing left ON condition")?.trim();
                let right_raw = on_tokens.next().ok_or("Missing right ON condition")?.trim();

                let left_col = left_raw
                    .split('.')
                    .next_back()
                    .unwrap_or(left_raw)
                    .to_string();
                let right_col = right_raw
                    .split('.')
                    .next_back()
                    .unwrap_or(right_raw)
                    .to_string();

                joins.push(JoinClause {
                    table: j_table,
                    alias: j_alias,
                    left_col,
                    right_col,
                });

                current_pos = on_cond_end;
                continue;
            }

            if let Some(w_rel_idx) = next_where {
                #[allow(clippy::collapsible_if)]
                if next_order.is_none_or(|o| w_rel_idx < o) {
                    let w_idx = current_pos + w_rel_idx;
                    let w_end = next_order.map_or(rest.len(), |o| current_pos + o);
                    let w_part = &rest[w_idx + 7..w_end].trim();

                    let mut c_idx = 0;
                    let w_bytes = w_part.as_bytes();
                    let mut start_idx = 0;

                    let mut raw_conds = Vec::new();

                    while c_idx < w_bytes.len() {
                        if c_idx + 4 <= w_bytes.len()
                            && w_part[c_idx..c_idx + 4].eq_ignore_ascii_case(" and")
                            && c_idx + 5 <= w_bytes.len()
                            && w_part[c_idx..c_idx + 5].eq_ignore_ascii_case(" and ")
                        {
                            raw_conds.push((&w_part[start_idx..c_idx], Some(LogicOp::And)));
                            c_idx += 5;
                            start_idx = c_idx;
                            continue;
                        }
                        if c_idx + 3 <= w_bytes.len()
                            && w_part[c_idx..c_idx + 3].eq_ignore_ascii_case(" or")
                            && c_idx + 4 <= w_bytes.len()
                            && w_part[c_idx..c_idx + 4].eq_ignore_ascii_case(" or ")
                        {
                            raw_conds.push((&w_part[start_idx..c_idx], Some(LogicOp::Or)));
                            c_idx += 4;
                            start_idx = c_idx;
                            continue;
                        }
                        c_idx += 1;
                    }
                    raw_conds.push((&w_part[start_idx..], None));

                    for (raw_c, next_logic) in raw_conds {
                        let raw_c = raw_c.trim();
                        if raw_c.is_empty() {
                            continue;
                        }

                        #[allow(clippy::manual_map)]
                        let op_idx = if let Some(i) = raw_c.find(">=") {
                            Some((i, 2, Operator::Gte))
                        } else if let Some(i) = raw_c.find("<=") {
                            Some((i, 2, Operator::Lte))
                        } else if let Some(i) = raw_c.find("!=") {
                            Some((i, 2, Operator::Neq))
                        } else if let Some(i) = raw_c.find('>') {
                            Some((i, 1, Operator::Gt))
                        } else if let Some(i) = raw_c.find('<') {
                            Some((i, 1, Operator::Lt))
                        } else if let Some(i) = raw_c.find('=') {
                            Some((i, 1, Operator::Eq))
                        } else {
                            None
                        };

                        if let Some((i, len, op)) = op_idx {
                            let left_raw = raw_c[..i].trim();
                            let left_col = left_raw
                                .split('.')
                                .next_back()
                                .unwrap_or(left_raw)
                                .to_string();
                            let right_col = raw_c[i + len..].trim().trim_matches('\'').to_string();
                            where_clause.push(WhereCondition {
                                left_col,
                                operator: op,
                                right_val: right_col,
                                next_logic,
                            });
                        }
                    }
                    current_pos = w_end;
                    continue;
                }
            }

            if let Some(o_rel_idx) = next_order {
                let o_idx = current_pos + o_rel_idx;
                order_by = Some(rest[o_idx + 10..].trim().to_string());
                break;
            }
            break;
        }

        return Ok(Ast::Select {
            columns,
            table,
            table_alias,
            join: joins,
            where_clause,
            order_by,
        });
    }

    if upper.starts_with("INSERT INTO ") {
        let values_idx = upper.find(" VALUES ").ok_or("Missing VALUES clause")?;
        let table = sql[12..values_idx].trim().to_string();
        let values_str = sql[values_idx + 8..].trim();
        let values_str = values_str.trim_start_matches('(').trim_end_matches(')');
        let values = values_str
            .split(',')
            .map(|s| s.trim().trim_matches('\'').to_string())
            .collect();
        return Ok(Ast::Insert { table, values });
    }

    if let Some(stripped) = upper.strip_prefix("CREATE TABLE ") {
        let rest = sql[13..].trim();
        let upper_rest = stripped.trim();
        let if_not_exists = upper_rest.starts_with("IF NOT EXISTS ");
        let table_name_start = if if_not_exists { 14 } else { 0 };
        let table_part = &rest[table_name_start..].trim();

        let paren_start = table_part.find('(');
        let table = if let Some(idx) = paren_start {
            table_part[..idx].trim().to_string()
        } else {
            table_part
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim()
                .to_string()
        };

        let mut columns = Vec::new();
        if let Some(idx) = paren_start {
            let cols_str = table_part[idx + 1..].trim().trim_end_matches(')');
            for col_def in cols_str.split(',') {
                let parts: Vec<&str> = col_def.split_whitespace().collect();
                if parts.len() >= 2 {
                    columns.push(ColumnDef {
                        name: parts.first().unwrap_or(&"").to_string(),
                        data_type: parts.get(1).unwrap_or(&"").to_string(),
                    });
                }
            }
        }

        return Ok(Ast::CreateTable {
            table,
            columns,
            if_not_exists,
        });
    }

    if let Some(stripped) = upper.strip_prefix("DROP TABLE ") {
        let mut table = sql[11..].trim();
        let mut if_exists = false;
        if stripped.starts_with("IF EXISTS ") {
            table = table[10..].trim();
            if_exists = true;
        }
        return Ok(Ast::DropTable {
            table: table.to_string(),
            if_exists,
        });
    }

    if let Some(stripped) = upper.strip_prefix("DELETE FROM ") {
        let rest = sql[12..].trim();
        let upper_rest = stripped.trim();

        let where_idx = upper_rest.find(" WHERE ");
        let table_end = where_idx.unwrap_or(rest.len());
        let table = rest[..table_end].trim().to_string();

        let mut where_clause = Vec::new();
        if let Some(w_rel_idx) = where_idx {
            let w_idx = w_rel_idx;
            let w_part = &rest[w_idx + 7..].trim();

            let mut c_idx = 0;
            let w_bytes = w_part.as_bytes();
            let mut start_idx = 0;
            let mut raw_conds = Vec::new();

            while c_idx < w_bytes.len() {
                if c_idx + 4 <= w_bytes.len()
                    && w_part[c_idx..c_idx + 4].eq_ignore_ascii_case(" and")
                    && c_idx + 5 <= w_bytes.len()
                    && w_part[c_idx..c_idx + 5].eq_ignore_ascii_case(" and ")
                {
                    raw_conds.push((&w_part[start_idx..c_idx], Some(LogicOp::And)));
                    c_idx += 5;
                    start_idx = c_idx;
                    continue;
                }
                if c_idx + 3 <= w_bytes.len()
                    && w_part[c_idx..c_idx + 3].eq_ignore_ascii_case(" or")
                    && c_idx + 4 <= w_bytes.len()
                    && w_part[c_idx..c_idx + 4].eq_ignore_ascii_case(" or ")
                {
                    raw_conds.push((&w_part[start_idx..c_idx], Some(LogicOp::Or)));
                    c_idx += 4;
                    start_idx = c_idx;
                    continue;
                }
                c_idx += 1;
            }
            raw_conds.push((&w_part[start_idx..], None));

            for (raw_c, next_logic) in raw_conds {
                let raw_c = raw_c.trim();
                if raw_c.is_empty() {
                    continue;
                }

                #[allow(clippy::manual_map)]
                let op_idx = if let Some(i) = raw_c.find(">=") {
                    Some((i, 2, Operator::Gte))
                } else if let Some(i) = raw_c.find("<=") {
                    Some((i, 2, Operator::Lte))
                } else if let Some(i) = raw_c.find("!=") {
                    Some((i, 2, Operator::Neq))
                } else if let Some(i) = raw_c.find('>') {
                    Some((i, 1, Operator::Gt))
                } else if let Some(i) = raw_c.find('<') {
                    Some((i, 1, Operator::Lt))
                } else if let Some(i) = raw_c.find('=') {
                    Some((i, 1, Operator::Eq))
                } else {
                    None
                };

                if let Some((i, len, op)) = op_idx {
                    let left_col = raw_c[..i].trim().to_string();
                    let right_col = raw_c[i + len..].trim().trim_matches('\'').to_string();
                    where_clause.push(WhereCondition {
                        left_col,
                        operator: op,
                        right_val: right_col,
                        next_logic,
                    });
                }
            }
        }
        return Ok(Ast::Delete {
            table,
            where_clause,
        });
    }

    Err("Unsupported SQL statement".into())
}
