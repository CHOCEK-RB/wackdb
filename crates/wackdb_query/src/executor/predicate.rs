use wackdb_sql::{LogicOp, Operator, WhereCondition};
use wackdb_tuple::{Schema, value::Value};

/// Evaluates a list of WHERE conditions against a set of values.
///
/// # Errors
/// Does not return errors; returns `false` on type mismatch or parsing failure.
#[must_use]
pub fn evaluate_where_clause(
    where_clause: &[WhereCondition],
    schema: &Schema,
    vals: &[Value],
) -> bool {
    if where_clause.is_empty() {
        return true;
    }

    let mut final_result = false;
    let mut current_and_group = true;

    for (i, cond) in where_clause.iter().enumerate() {
        let Some(col_idx) = schema.columns.iter().position(|c| c.name == cond.left_col) else {
            continue;
        };

        let val = &vals[col_idx];
        let r_val = &cond.right_val;

        let is_match = match val {
            Value::Null => false,
            Value::Integer(v) => evaluate_int_cond(*v, r_val, &cond.operator),
            Value::Boolean(v) => evaluate_bool_cond(*v, r_val, &cond.operator),
            Value::Varchar(v) => evaluate_str_cond(v, r_val, &cond.operator),
        };

        current_and_group = current_and_group && is_match;

        if let Some(LogicOp::Or) = cond.next_logic {
            final_result = final_result || current_and_group;
            current_and_group = true;
        } else if i == where_clause.len() - 1 {
            final_result = final_result || current_and_group;
        }
    }

    final_result
}

fn evaluate_int_cond(v: i32, r_val: &str, op: &Operator) -> bool {
    let Ok(rv) = r_val.parse::<i32>() else {
        return false;
    };
    match op {
        Operator::Eq => v == rv,
        Operator::Neq => v != rv,
        Operator::Gt => v > rv,
        Operator::Gte => v >= rv,
        Operator::Lt => v < rv,
        Operator::Lte => v <= rv,
    }
}

fn evaluate_bool_cond(v: bool, r_val: &str, op: &Operator) -> bool {
    let Ok(rv) = r_val.parse::<bool>() else {
        return false;
    };
    match op {
        Operator::Eq => v == rv,
        Operator::Neq => v != rv,
        _ => false,
    }
}

fn evaluate_str_cond(v: &str, r_val: &str, op: &Operator) -> bool {
    match op {
        Operator::Eq => v == r_val,
        Operator::Neq => v != r_val,
        Operator::Gt => v > r_val,
        Operator::Gte => v >= r_val,
        Operator::Lt => v < r_val,
        Operator::Lte => v <= r_val,
    }
}
