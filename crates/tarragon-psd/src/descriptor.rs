//! A small, checked reader for Adobe `ActionDescriptor` trees.
//!
//! Layer extra blocks such as `SoCo`, `GdFl` and `CgEd` hold an OSType-tagged
//! key/value tree. The compositor needs a few keys only, so this reader keeps a
//! small value set and rejects anything else. A malformed tree returns `None`,
//! and the caller marks the layer content unsupported.
//!
//! The binary layout is the documented Adobe `ActionDescriptor` format. This is
//! our own reader.

/// The deepest nested descriptor this reader accepts.
const MAX_DEPTH: usize = 32;

/// One parsed descriptor: a class id and its key/value items.
#[derive(Debug, Clone, PartialEq)]
pub struct Descriptor {
    /// The class id bytes.
    pub class_id: Vec<u8>,
    /// The items in stored order.
    pub items: Vec<(Vec<u8>, Value)>,
}

/// One descriptor value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A nested `Objc` descriptor.
    Descriptor(Descriptor),
    /// A `VlLs` list.
    List(Vec<Value>),
    /// A `doub` double.
    Double(f64),
    /// An `UntF` unit float.
    UnitFloat {
        /// The four-byte unit code, for example `#Prc`.
        unit: [u8; 4],
        /// The value.
        value: f64,
    },
    /// A `UnFl` unit float array.
    UnitFloats {
        /// The four-byte unit code.
        unit: [u8; 4],
        /// The values.
        values: Vec<f64>,
    },
    /// A `TEXT` unicode string.
    Text(String),
    /// An `enum` enumerated value.
    Enumerated {
        /// The enum type id.
        type_id: Vec<u8>,
        /// The enum value id.
        value: Vec<u8>,
    },
    /// A `long` integer.
    Integer(i32),
    /// A `comp` large integer.
    LargeInteger(i64),
    /// A `bool` boolean.
    Boolean(bool),
    /// An `obj ` reference list. The items are consumed but not kept.
    Reference,
    /// A `type` or `GlbC` class. The name and id are consumed but not kept.
    Class,
    /// An `alis`, `Pth ` or `tdta` raw byte block.
    Raw(Vec<u8>),
}

impl Descriptor {
    /// The first item with `key`.
    #[must_use]
    pub fn get(&self, key: &[u8]) -> Option<&Value> {
        self.items
            .iter()
            .find(|(item_key, _)| item_key == key)
            .map(|(_, value)| value)
    }

    /// The nested descriptor or global object at `key`.
    #[must_use]
    pub fn descriptor(&self, key: &[u8]) -> Option<&Descriptor> {
        match self.get(key)? {
            Value::Descriptor(inner) => Some(inner),
            _ => None,
        }
    }

    /// The number at `key`, from a double, an integer or a unit float.
    #[must_use]
    pub fn number(&self, key: &[u8]) -> Option<f64> {
        match self.get(key)? {
            Value::Double(value) | Value::UnitFloat { value, .. } => Some(*value),
            Value::Integer(value) => Some(f64::from(*value)),
            _ => None,
        }
    }

    /// The boolean at `key`.
    #[must_use]
    pub fn boolean(&self, key: &[u8]) -> Option<bool> {
        match self.get(key)? {
            Value::Boolean(value) => Some(*value),
            _ => None,
        }
    }

    /// The text at `key`.
    #[allow(dead_code, reason = "part of the reader; used by the descriptor tests")]
    #[must_use]
    pub fn text(&self, key: &[u8]) -> Option<&str> {
        match self.get(key)? {
            Value::Text(value) => Some(value.as_str()),
            _ => None,
        }
    }

    /// The enumerated value id at `key`.
    #[must_use]
    pub fn enumeration(&self, key: &[u8]) -> Option<&[u8]> {
        match self.get(key)? {
            Value::Enumerated { value, .. } => Some(value.as_slice()),
            _ => None,
        }
    }

    /// The list at `key`.
    #[must_use]
    pub fn list(&self, key: &[u8]) -> Option<&[Value]> {
        match self.get(key)? {
            Value::List(items) => Some(items.as_slice()),
            _ => None,
        }
    }
}

/// Parse a versioned descriptor (`u32` version then the descriptor). Returns
/// `None` on a bad version or any malformed byte.
#[must_use]
pub fn parse_versioned(data: &[u8]) -> Option<(Descriptor, usize)> {
    let mut reader = Reader::new(data);
    if reader.u32()? != 16 {
        return None;
    }
    let descriptor = read_descriptor(&mut reader, 0)?;
    Some((descriptor, reader.pos))
}

/// A checked big-endian cursor over a descriptor byte slice.
struct Reader<'a> {
    /// The buffer.
    data: &'a [u8],
    /// The read position.
    pos: usize,
}

impl<'a> Reader<'a> {
    /// Wrap a slice.
    const fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// The bytes left.
    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    /// Take `count` bytes, or `None`.
    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(count)?;
        let slice = self.data.get(self.pos..end)?;
        self.pos = end;
        Some(slice)
    }

    /// Read a big-endian `u8`.
    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }

    /// Read a big-endian `u16`.
    #[allow(dead_code, reason = "part of the reader; kept for completeness")]
    fn u16(&mut self) -> Option<u16> {
        let bytes = self.take(2)?;
        Some(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    /// Read a big-endian `u32`.
    fn u32(&mut self) -> Option<u32> {
        let bytes = self.take(4)?;
        Some(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// Read a big-endian `i32`.
    fn i32(&mut self) -> Option<i32> {
        let bytes = self.take(4)?;
        Some(i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    /// Read a big-endian `i64`.
    fn i64(&mut self) -> Option<i64> {
        let bytes = self.take(8)?;
        Some(i64::from_be_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]))
    }

    /// Read a big-endian `f64`.
    fn f64(&mut self) -> Option<f64> {
        let bytes = self.take(8)?;
        let mut array = [0u8; 8];
        array.copy_from_slice(bytes);
        Some(f64::from_be_bytes(array))
    }

    /// Read a four-byte code.
    fn code(&mut self) -> Option<[u8; 4]> {
        let bytes = self.take(4)?;
        let mut array = [0u8; 4];
        array.copy_from_slice(bytes);
        Some(array)
    }

    /// Read a length-prefixed id: a zero length means a four-byte code follows.
    fn id(&mut self) -> Option<Vec<u8>> {
        let length = self.u32()?;
        if length == 0 {
            return Some(self.code()?.to_vec());
        }
        let length = usize::try_from(length).ok()?;
        Some(self.take(length)?.to_vec())
    }

    /// Read a `TEXT` unicode string.
    fn unicode(&mut self) -> Option<String> {
        let units = self.u32()?;
        let bytes = usize::try_from(units).ok()?.checked_mul(2)?;
        let raw = self.take(bytes)?;
        let units: Vec<u16> = raw
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect();
        Some(String::from_utf16_lossy(&units))
    }

    /// Read a class (a display name and a class id), discarding the name.
    fn class(&mut self) -> Option<()> {
        self.unicode()?;
        self.id()?;
        Some(())
    }
}

/// Read a descriptor body.
fn read_descriptor(reader: &mut Reader<'_>, depth: usize) -> Option<Descriptor> {
    if depth > MAX_DEPTH {
        return None;
    }
    reader.unicode()?;
    let class_id = reader.id()?;
    let count = reader.u32()?;
    // Every item needs at least eight bytes (four for the key, four for the type).
    let count = usize::try_from(count).ok()?;
    if count.checked_mul(8)? > reader.remaining() {
        return None;
    }
    let mut items = Vec::with_capacity(count);
    for _ in 0..count {
        let key = reader.id()?;
        let value = read_value(reader, depth + 1)?;
        items.push((key, value));
    }
    Some(Descriptor { class_id, items })
}

/// Read one typed value.
fn read_value(reader: &mut Reader<'_>, depth: usize) -> Option<Value> {
    if depth > MAX_DEPTH {
        return None;
    }
    let kind = reader.code()?;
    Some(match &kind {
        b"obj " => {
            let count = usize::try_from(reader.u32()?).ok()?;
            // Every reference item needs at least eight bytes.
            if count.checked_mul(8)? > reader.remaining() {
                return None;
            }
            for _ in 0..count {
                read_reference_item(reader)?;
            }
            Value::Reference
        }
        b"Objc" | b"GlbO" => Value::Descriptor(read_descriptor(reader, depth + 1)?),
        b"VlLs" => {
            let count = usize::try_from(reader.u32()?).ok()?;
            if count.checked_mul(4)? > reader.remaining() {
                return None;
            }
            let mut items = Vec::with_capacity(count);
            for _ in 0..count {
                items.push(read_value(reader, depth + 1)?);
            }
            Value::List(items)
        }
        b"doub" => Value::Double(reader.f64()?),
        b"UntF" => Value::UnitFloat {
            unit: reader.code()?,
            value: reader.f64()?,
        },
        b"UnFl" => {
            let unit = reader.code()?;
            let count = usize::try_from(reader.u32()?).ok()?;
            if count.checked_mul(8)? > reader.remaining() {
                return None;
            }
            let mut values = Vec::with_capacity(count);
            for _ in 0..count {
                values.push(reader.f64()?);
            }
            Value::UnitFloats { unit, values }
        }
        b"TEXT" => Value::Text(reader.unicode()?),
        b"enum" => Value::Enumerated {
            type_id: reader.id()?,
            value: reader.id()?,
        },
        b"long" => Value::Integer(reader.i32()?),
        b"comp" => Value::LargeInteger(reader.i64()?),
        b"bool" => Value::Boolean(reader.u8()? != 0),
        b"type" | b"GlbC" => {
            reader.class()?;
            Value::Class
        }
        b"alis" | b"Pth " | b"tdta" => {
            let length = usize::try_from(reader.u32()?).ok()?;
            Value::Raw(reader.take(length)?.to_vec())
        }
        b"ObAr" => {
            reader.u32()?;
            Value::Descriptor(read_descriptor(reader, depth + 1)?)
        }
        _ => return None,
    })
}

/// Read and discard one reference item.
fn read_reference_item(reader: &mut Reader<'_>) -> Option<()> {
    match &reader.code()? {
        b"prop" => {
            reader.class()?;
            reader.id()?;
        }
        b"Clss" => reader.class()?,
        b"Enmr" => {
            reader.class()?;
            reader.id()?;
            reader.id()?;
        }
        b"rele" => {
            reader.class()?;
            reader.i32()?;
        }
        b"Idnt" | b"indx" => {
            reader.u32()?;
        }
        b"name" => {
            reader.class()?;
            reader.unicode()?;
        }
        _ => return None,
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::{Value, parse_versioned};

    /// Push a four-byte code or a length-prefixed id.
    fn push_id(data: &mut Vec<u8>, id: &[u8]) {
        if id.len() == 4 {
            data.extend_from_slice(&0u32.to_be_bytes());
            data.extend_from_slice(id);
        } else {
            data.extend_from_slice(&u32::try_from(id.len()).expect("len").to_be_bytes());
            data.extend_from_slice(id);
        }
    }

    /// Push a unicode string as a count then UTF-16BE units.
    fn push_unicode(data: &mut Vec<u8>, value: &str) {
        let units: Vec<u16> = value.encode_utf16().collect();
        data.extend_from_slice(&u32::try_from(units.len()).expect("len").to_be_bytes());
        for unit in units {
            data.extend_from_slice(&unit.to_be_bytes());
        }
    }

    /// Encode a descriptor body with one `doub` item.
    fn simple_descriptor() -> Vec<u8> {
        let mut data = Vec::new();
        push_unicode(&mut data, "");
        push_id(&mut data, b"null");
        data.extend_from_slice(&1u32.to_be_bytes());
        push_id(&mut data, b"Opct");
        data.extend_from_slice(b"doub");
        data.extend_from_slice(&75.5f64.to_be_bytes());
        data
    }

    #[test]
    fn a_versioned_descriptor_reads_its_items() {
        let mut data = 16u32.to_be_bytes().to_vec();
        data.extend_from_slice(&simple_descriptor());
        let (descriptor, used) = parse_versioned(&data).expect("descriptor parses");
        assert_eq!(used, data.len());
        assert_eq!(descriptor.class_id, b"null");
        assert_eq!(descriptor.number(b"Opct"), Some(75.5));
        assert!(descriptor.get(b"nope").is_none());
    }

    #[test]
    fn a_non_sixteen_version_is_rejected() {
        let data = 15u32.to_be_bytes();
        assert!(parse_versioned(&data).is_none());
    }

    #[test]
    fn every_prefix_is_rejected_without_panic() {
        let mut data = 16u32.to_be_bytes().to_vec();
        // A text item, then an enumerated value and a boolean.
        push_unicode(&mut data, "");
        push_id(&mut data, b"null");
        data.extend_from_slice(&3u32.to_be_bytes());
        push_id(&mut data, b"Nm  ");
        data.extend_from_slice(b"TEXT");
        push_unicode(&mut data, "hello");
        push_id(&mut data, b"Md  ");
        data.extend_from_slice(b"enum");
        push_id(&mut data, b"BlnM");
        push_id(&mut data, b"Mltp");
        push_id(&mut data, b"enab");
        data.extend_from_slice(b"bool");
        data.push(1);
        let full = parse_versioned(&data).expect("full parses");
        assert_eq!(full.0.text(b"Nm  "), Some("hello"));
        assert_eq!(full.0.enumeration(b"Md  "), Some(b"Mltp".as_slice()));
        assert_eq!(full.0.boolean(b"enab"), Some(true));
        for cut in 0..data.len() {
            assert!(parse_versioned(&data[..cut]).is_none(), "cut {cut}");
        }
    }

    #[test]
    fn nested_descriptors_and_lists_parse() {
        let mut data = 16u32.to_be_bytes().to_vec();
        push_unicode(&mut data, "");
        push_id(&mut data, b"null");
        data.extend_from_slice(&2u32.to_be_bytes());
        push_id(&mut data, b"Clr ");
        data.extend_from_slice(b"Objc");
        data.extend_from_slice(&simple_descriptor());
        push_id(&mut data, b"Clrs");
        data.extend_from_slice(b"VlLs");
        data.extend_from_slice(&2u32.to_be_bytes());
        data.extend_from_slice(b"long");
        data.extend_from_slice(&7i32.to_be_bytes());
        data.extend_from_slice(b"bool");
        data.push(0);
        let (descriptor, _) = parse_versioned(&data).expect("nested parses");
        let color = descriptor.descriptor(b"Clr ").expect("nested color");
        assert_eq!(color.number(b"Opct"), Some(75.5));
        let list = descriptor.list(b"Clrs").expect("list");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0], Value::Integer(7));
        assert_eq!(list[1], Value::Boolean(false));
    }

    #[test]
    fn an_unknown_ostype_is_rejected() {
        let mut data = 16u32.to_be_bytes().to_vec();
        push_unicode(&mut data, "");
        push_id(&mut data, b"null");
        data.extend_from_slice(&1u32.to_be_bytes());
        push_id(&mut data, b"key ");
        data.extend_from_slice(b"????");
        assert!(parse_versioned(&data).is_none());
    }

    #[test]
    fn a_huge_item_count_is_rejected_without_alloc() {
        let mut data = 16u32.to_be_bytes().to_vec();
        push_unicode(&mut data, "");
        push_id(&mut data, b"null");
        data.extend_from_slice(&u32::MAX.to_be_bytes());
        assert!(parse_versioned(&data).is_none());
    }

    #[test]
    fn a_hand_built_descriptor_matches_its_values() {
        let mut data = 16u32.to_be_bytes().to_vec();
        push_unicode(&mut data, "");
        push_id(&mut data, b"null");
        data.extend_from_slice(&1u32.to_be_bytes());
        push_id(&mut data, b"Hrzn");
        data.extend_from_slice(b"UntF");
        data.extend_from_slice(b"#Prc");
        data.extend_from_slice(&(-25.0f64).to_be_bytes());
        let (descriptor, _) = parse_versioned(&data).expect("parses");
        assert_eq!(
            descriptor.get(b"Hrzn"),
            Some(&Value::UnitFloat {
                unit: *b"#Prc",
                value: -25.0
            })
        );
        assert_eq!(descriptor.number(b"Hrzn"), Some(-25.0));
    }
}
