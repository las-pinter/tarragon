//! SQLite record decoding.
//!
//! A record payload holds a header of serial types and a body of values. The
//! module decodes the columns the preview needs and reads the column position
//! from the `CREATE TABLE` text.

use super::sqlite::{DbError, read_varint};

/// A value from one record column.
#[derive(Debug)]
pub(super) enum Value<'a> {
    /// The SQL NULL value.
    Null,
    /// An integer value.
    Integer(i64),
    /// A real value.
    Real(f64),
    /// A text value.
    Text(&'a [u8]),
    /// A blob value.
    Blob(&'a [u8]),
}

/// One table found in the database schema.
pub(super) struct Table {
    /// The root page number of the table b-tree.
    pub(super) root_page: u32,
    /// The `CREATE TABLE` statement text.
    pub(super) sql: String,
}

/// The most columns a record may hold. This is the SQLite default limit.
const MAX_COLUMNS: usize = 2000;

/// Parse a record payload into its column values.
pub(super) fn parse_record(payload: &[u8]) -> Result<Vec<Value<'_>>, DbError> {
    let (header_len, mut cursor) = read_varint(payload, 0)?;
    let header_len = usize::try_from(header_len)
        .map_err(|_| DbError::Format("the record header is too long".to_owned()))?;
    if header_len == 0 || header_len > payload.len() {
        return Err(DbError::Format(
            "the record header leaves the payload".to_owned(),
        ));
    }
    let mut serials = Vec::new();
    while cursor < header_len {
        let (serial, used) = read_varint(payload, cursor)?;
        cursor += used;
        if serials.len() == MAX_COLUMNS {
            return Err(DbError::Format(
                "the record holds too many columns".to_owned(),
            ));
        }
        serials.push(serial);
    }
    if cursor != header_len {
        return Err(DbError::Format(
            "the record header does not match its length".to_owned(),
        ));
    }
    let mut body = header_len;
    let mut values = Vec::with_capacity(serials.len());
    for serial in serials {
        let length = serial_length(serial)?;
        let end = body
            .checked_add(length)
            .ok_or_else(|| DbError::Format("the record body overflows".to_owned()))?;
        let bytes = payload
            .get(body..end)
            .ok_or_else(|| DbError::Format("the record body is cut".to_owned()))?;
        values.push(value_from(serial, bytes)?);
        body = end;
    }
    Ok(values)
}

/// The body length of one serial type.
fn serial_length(serial: u64) -> Result<usize, DbError> {
    Ok(match serial {
        0 | 8 | 9 => 0,
        1 => 1,
        2 => 2,
        3 => 3,
        4 => 4,
        5 => 6,
        6 | 7 => 8,
        10 | 11 => {
            return Err(DbError::Format(
                "the record holds a reserved serial type".to_owned(),
            ));
        }
        other if other % 2 == 0 => usize::try_from((other - 12) / 2)
            .map_err(|_| DbError::Format("the record value is too long".to_owned()))?,
        other => usize::try_from((other - 13) / 2)
            .map_err(|_| DbError::Format("the record value is too long".to_owned()))?,
    })
}

/// Build one column value from its serial type and body bytes.
fn value_from(serial: u64, bytes: &[u8]) -> Result<Value<'_>, DbError> {
    Ok(match serial {
        0 => Value::Null,
        1..=6 => Value::Integer(signed_be(bytes)),
        7 => {
            let array: [u8; 8] = bytes
                .try_into()
                .map_err(|_| DbError::Format("the record real value is cut".to_owned()))?;
            Value::Real(f64::from_be_bytes(array))
        }
        8 => Value::Integer(0),
        9 => Value::Integer(1),
        other if other % 2 == 0 => Value::Blob(bytes),
        _ => Value::Text(bytes),
    })
}

/// Read a big-endian signed integer of one to eight bytes.
fn signed_be(bytes: &[u8]) -> i64 {
    let mut value = 0_i64;
    for byte in bytes {
        value = (value << 8) | i64::from(*byte);
    }
    let shift = 64 - 8 * bytes.len();
    (value << shift) >> shift
}

/// Return the named table from one `sqlite_master` row.
pub(super) fn table_from_schema(
    values: &[Value<'_>],
    name: &str,
) -> Result<Option<Table>, DbError> {
    let (Some(Value::Text(kind)), Some(Value::Text(table_name))) = (values.first(), values.get(1))
    else {
        return Ok(None);
    };
    if !kind.eq_ignore_ascii_case(b"table") || !table_name.eq_ignore_ascii_case(name.as_bytes()) {
        return Ok(None);
    }
    let root_page = match values.get(3) {
        Some(Value::Integer(value)) => u32::try_from(*value)
            .map_err(|_| DbError::Format(format!("table {name} has a bad root page {value}")))?,
        _ => {
            return Err(DbError::Format(format!("table {name} has no root page")));
        }
    };
    if root_page == 0 {
        return Err(DbError::Format(format!("table {name} is a virtual table")));
    }
    let sql = match values.get(4) {
        Some(Value::Text(text)) => String::from_utf8_lossy(text).into_owned(),
        _ => String::new(),
    };
    Ok(Some(Table { root_page, sql }))
}

/// Find the position of `column` in a `CREATE TABLE` statement.
#[must_use]
pub(super) fn column_index(create_sql: &str, column: &str) -> Option<usize> {
    let open = create_sql.find('(')?;
    let mut depth = 0_usize;
    let mut start = open + 1;
    let mut index = 0_usize;
    let mut quote: Option<u8> = None;
    for (position, byte) in create_sql.bytes().enumerate().skip(open + 1) {
        match quote {
            Some(end) => {
                if byte == end {
                    quote = None;
                }
            }
            None => match byte {
                b'\'' | b'"' | b'`' => quote = Some(byte),
                b'[' => quote = Some(b']'),
                b'(' => depth += 1,
                b')' if depth == 0 => {
                    return segment_index(create_sql.get(start..position)?, column, index);
                }
                b')' => depth -= 1,
                b',' if depth == 0 => {
                    if let Some(found) =
                        segment_index(create_sql.get(start..position)?, column, index)
                    {
                        return Some(found);
                    }
                    index += 1;
                    start = position + 1;
                }
                _ => {}
            },
        }
    }
    None
}

/// Compare one column definition with the wanted column name.
fn segment_index(segment: &str, column: &str, index: usize) -> Option<usize> {
    let name = first_identifier(segment)?;
    if ["primary", "unique", "check", "foreign", "constraint"]
        .iter()
        .any(|keyword| name.eq_ignore_ascii_case(keyword))
    {
        return None;
    }
    name.eq_ignore_ascii_case(column).then_some(index)
}

/// Return the first identifier of one column definition.
fn first_identifier(segment: &str) -> Option<&str> {
    let trimmed = segment.trim_start();
    let name = if let Some(rest) = trimmed.strip_prefix('"') {
        &rest[..rest.find('"')?]
    } else if let Some(rest) = trimmed.strip_prefix('`') {
        &rest[..rest.find('`')?]
    } else if let Some(rest) = trimmed.strip_prefix('[') {
        &rest[..rest.find(']')?]
    } else {
        let end = trimmed
            .find(|character: char| character.is_ascii_whitespace() || character == '(')
            .unwrap_or(trimmed.len());
        &trimmed[..end]
    };
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::{Value, column_index, parse_record};

    #[test]
    fn a_record_returns_its_columns() {
        // Header length 3, serial types integer 1 and text of length 2.
        let payload = [3, 1, 17, 7, b'o', b'k'];
        let values = parse_record(&payload).expect("the record parses");
        assert_eq!(values.len(), 2);
        assert!(matches!(values[0], Value::Integer(7)));
        assert!(matches!(values[1], Value::Text(text) if text == b"ok"));
    }

    #[test]
    fn a_cut_record_is_an_error() {
        // The header claims 16 text bytes, but the body holds only one.
        assert!(parse_record(&[3, 45, b'a']).is_err());
    }

    /// Build a record of `columns` NULL columns with a two-byte header length.
    fn null_record(columns: u8, extra: u8) -> Vec<u8> {
        // The columns are `extra * 100 + columns` one-byte serial types.
        let count = usize::from(extra) * 100 + usize::from(columns);
        let header_len = u8::try_from((2 + count) & 0x7f).unwrap();
        let high = u8::try_from((2 + count) >> 7).unwrap();
        let mut payload = vec![0x80 | high, header_len];
        payload.extend(std::iter::repeat_n(0_u8, count));
        payload
    }

    #[test]
    fn a_record_over_the_column_cap_is_an_error() {
        assert!(parse_record(&null_record(1, 20)).is_err());
    }

    #[test]
    fn a_record_at_the_column_cap_parses() {
        assert_eq!(
            parse_record(&null_record(0, 20)).expect("parses").len(),
            2000
        );
    }

    #[test]
    fn a_null_column_reads_as_null() {
        let values = parse_record(&[2, 0]).expect("the record parses");
        assert!(matches!(values[0], Value::Null));
    }

    #[test]
    fn the_column_index_reads_a_create_table_statement() {
        let sql = "CREATE TABLE CanvasPreview (ImageData BLOB)";
        assert_eq!(column_index(sql, "ImageData"), Some(0));
        assert_eq!(column_index(sql, "imagedata"), Some(0));
        assert_eq!(column_index(sql, "Missing"), None);
    }

    #[test]
    fn the_column_index_counts_earlier_columns() {
        let sql = "CREATE TABLE t (id INTEGER PRIMARY KEY, Name TEXT NOT NULL, ImageData BLOB)";
        assert_eq!(column_index(sql, "ImageData"), Some(2));
        assert_eq!(column_index(sql, "Name"), Some(1));
    }

    #[test]
    fn the_column_index_accepts_quoted_names_and_types() {
        let sql = "CREATE TABLE t (\"Image Data\" BLOB, [ImageData] BLOB)";
        assert_eq!(column_index(sql, "ImageData"), Some(1));
        assert_eq!(column_index(sql, "Image Data"), Some(0));
    }
}
