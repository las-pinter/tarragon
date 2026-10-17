//! Filter engine: compose the active filters into one thumbnail query.
//!
//! The semantics are tags AND, colors AND, folders OR, and a filename
//! substring. The filename text is used as-is, so a whitespace-only text is a
//! real filter. The result comes back in natural full-path order.
//! The query runs on the shared connection. The caller chooses the folder
//! scope, so a folder scope passes only the current folder in `folders`.

use std::path::Path;

use rusqlite::types::Value;
use tarragon_core::filter_state::FilterState;
use tarragon_core::sorting::natural_path_cmp;

use crate::like::{escape_like, folder_like};
use crate::repository::{THUMBNAIL_COLUMNS, Thumbnail, row_to_thumbnail};
use crate::{Database, DbError};

/// Push an "every one of these tag names is present" condition.
///
/// The `HAVING COUNT` clause gives AND semantics: a path must carry all
/// names, not just one. The names come from the user tags or the color
/// buckets, which are bare color names.
fn push_name_group(conditions: &mut Vec<String>, params: &mut Vec<Value>, names: &[&str]) {
    let placeholders = vec!["?"; names.len()].join(", ");
    conditions.push(format!(
        "path IN (SELECT ft.path FROM file_tags ft JOIN tags t ON t.id = ft.tag_id \
         WHERE t.name IN ({placeholders}) GROUP BY ft.path \
         HAVING COUNT(DISTINCT t.name) = ?)"
    ));
    for name in names {
        params.push(Value::Text((*name).to_owned()));
    }
    params.push(Value::Integer(
        i64::try_from(names.len()).unwrap_or(i64::MAX),
    ));
}

/// Build the filtered SELECT and its bound values.
///
/// The conditions join with AND. The folder terms join with OR inside one
/// condition. A whitespace-only text is not empty, so it adds one condition
/// and is not trimmed.
fn filter_sql(filters: &FilterState) -> (String, Vec<Value>) {
    let mut conditions: Vec<String> = Vec::new();
    let mut params: Vec<Value> = Vec::new();

    if !filters.folders.is_empty() {
        let mut terms: Vec<String> = Vec::new();
        for folder in &filters.folders {
            terms.push("path LIKE ? ESCAPE '\\'".to_owned());
            params.push(Value::Text(folder_like(&folder.to_string_lossy())));
        }
        let mut group = String::from("(");
        group.push_str(&terms.join(" OR "));
        group.push(')');
        conditions.push(group);
    }

    if !filters.text.is_empty() {
        conditions.push("path LIKE ? ESCAPE '\\'".to_owned());
        params.push(Value::Text(format!("%{}%", escape_like(&filters.text))));
    }

    if !filters.tags.is_empty() {
        let names: Vec<&str> = filters.tags.iter().map(String::as_str).collect();
        push_name_group(&mut conditions, &mut params, &names);
    }

    if !filters.colors.is_empty() {
        let mut names: Vec<&str> = filters
            .colors
            .iter()
            .map(|bucket| bucket.as_str())
            .collect();
        names.sort_unstable();
        push_name_group(&mut conditions, &mut params, &names);
    }

    let mut sql = format!("SELECT {THUMBNAIL_COLUMNS} FROM thumbnails");
    if !conditions.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&conditions.join(" AND "));
    }
    sql.push_str(" ORDER BY path");
    (sql, params)
}

impl Database {
    /// Return the thumbnail rows that match every active filter.
    ///
    /// Folders combine with OR. Tags and colors each require every selected
    /// name. The filename text is a substring of the stored path and is used
    /// as-is, with no trim. Rows come back in natural full-path order.
    ///
    /// # Errors
    ///
    /// Returns [`DbError::Sqlite`] when the read fails. Returns
    /// [`DbError::LockPoisoned`] after a panic poisoned the lock.
    pub fn filter_thumbnails(&self, filters: &FilterState) -> Result<Vec<Thumbnail>, DbError> {
        let (sql, params) = filter_sql(filters);
        let conn = self.lock()?;
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(params), row_to_thumbnail)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        out.sort_by(|left, right| natural_path_cmp(Path::new(&left.path), Path::new(&right.path)));
        Ok(out)
    }
}
