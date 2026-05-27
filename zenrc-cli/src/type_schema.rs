//! CDR encode/decode helpers and TypeObject-based schema discovery.
//!
//! Provides:
//! - `TypeSchema` — field names (from TypeObject) + field types (from m_ops)
//! - `decode`     — CDR bytes → serde_json::Value
//! - `encode`     — serde_json::Value → CDR bytes
//! - Pure-Rust XTypes TypeObject discovery via zenrc_dds bindings

use std::ffi::CStr;

use serde_json::Value;
use zenrc_dds::{dds_entity_t, dds_return_t, dds_typeinfo_t};

// --------------------------------------------------------------------------
// CDR write — pure Rust via zenrc_dds bindings
// --------------------------------------------------------------------------

/// Write raw CDR bytes (including the 4-byte encapsulation header) to a DDS
/// writer by calling `from_ser_iov` through the entity's own sertype ops.
///
/// # Safety
/// `writer` must be a valid DDS writer entity.
pub unsafe fn write_cdr_bytes(writer: dds_entity_t, bytes: &[u8]) -> dds_return_t {
    use zenrc_dds::{
        DDS_RETCODE_ERROR, dds_get_entity_sertype, dds_writecdr,
        ddsi_serdata, ddsi_serdata_kind_SDK_DATA, ddsi_sertype, ddsrt_iovec_t,
    };

    // ddsi_serdata_unref is exported by libddsc but not re-exported by zenrc_dds.
    unsafe extern "C" {
        fn ddsi_serdata_unref(sd: *mut ddsi_serdata);
    }

    let mut sertype: *const ddsi_sertype = std::ptr::null();
    let rc = unsafe { dds_get_entity_sertype(writer, &mut sertype) };
    if rc != 0 || sertype.is_null() {
        return if rc < 0 { rc } else { DDS_RETCODE_ERROR };
    }

    let serdata_ops = unsafe { (*sertype).serdata_ops };
    if serdata_ops.is_null() {
        return DDS_RETCODE_ERROR;
    }

    let from_ser_iov = match unsafe { (*serdata_ops).from_ser_iov } {
        Some(f) => f,
        None => return DDS_RETCODE_ERROR,
    };

    let iov = ddsrt_iovec_t {
        iov_base: bytes.as_ptr() as *mut _,
        iov_len: bytes.len(),
    };

    let sd = unsafe { from_ser_iov(sertype, ddsi_serdata_kind_SDK_DATA, 1, &iov, bytes.len()) };
    if sd.is_null() {
        return DDS_RETCODE_ERROR;
    }

    let rc = unsafe { dds_writecdr(writer, sd) };
    unsafe { ddsi_serdata_unref(sd) };
    rc
}

// --------------------------------------------------------------------------
// m_ops constants
// --------------------------------------------------------------------------

const DDS_OP_MASK: u32      = 0xFF000000;
const DDS_OP_ADR: u32       = 0x01 << 24;
const DDS_OP_TYPE_MASK: u32 = 0x007F0000;
const DDS_OP_SUBTYPE_MASK: u32 = 0x0000FF00;
const DDS_OP_FLAGS_MASK: u32 = 0x000000FF;

// Type codes in bits 22-16 of ADR word
const OP_1BY:  u8 = 0x01;  // 1-byte scalar
const OP_2BY:  u8 = 0x02;  // 2-byte scalar
const OP_4BY:  u8 = 0x03;  // 4-byte scalar
const OP_8BY:  u8 = 0x04;  // 8-byte scalar
const OP_STR:  u8 = 0x05;  // unbounded string
const OP_BST:  u8 = 0x06;  // bounded string (extra word: max_len)
const OP_SEQ:  u8 = 0x07;  // sequence (subtype = element type code)
const OP_ARR:  u8 = 0x08;  // array (extra word: count)
const OP_STU:  u8 = 0x0a;  // nested struct reference (extra word: JSR)
const OP_ENU:  u8 = 0x0c;  // enum (serialised as u32)
const OP_BLN:  u8 = 0x0e;  // boolean (1 byte)
const OP_WSTR: u8 = 0x10;  // wide string

// Flag bits in bits 7-0 of ADR word
const FLAG_FP:  u8 = 1 << 1; // floating-point
const FLAG_SGN: u8 = 1 << 2; // signed integer

// --------------------------------------------------------------------------
// CdrField enum
// --------------------------------------------------------------------------

/// A single CDR field type, derived from m_ops.
#[derive(Debug, Clone, PartialEq)]
pub enum CdrField {
    Bool,
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    F32,
    F64,
    Str,
    SeqOf(u8, u8),      // (element type_code, element flags)
    ArrOf(u8, u8, u32), // (element type_code, element flags, count)
    Nested,             // STU — nested struct, not fully decoded
    Unknown(u8),        // unrecognised type code
}

fn cdr_field_from_code(type_code: u8, flags: u8) -> CdrField {
    match type_code {
        OP_BLN => CdrField::Bool,
        OP_1BY => {
            if flags & FLAG_SGN != 0 {
                CdrField::I8
            } else {
                CdrField::U8
            }
        }
        OP_2BY => {
            if flags & FLAG_SGN != 0 {
                CdrField::I16
            } else {
                CdrField::U16
            }
        }
        OP_4BY => {
            if flags & FLAG_FP != 0 {
                CdrField::F32
            } else if flags & FLAG_SGN != 0 {
                CdrField::I32
            } else {
                CdrField::U32
            }
        }
        OP_8BY => {
            if flags & FLAG_FP != 0 {
                CdrField::F64
            } else if flags & FLAG_SGN != 0 {
                CdrField::I64
            } else {
                CdrField::U64
            }
        }
        OP_STR | OP_BST | OP_WSTR => CdrField::Str,
        OP_ENU => CdrField::U32,
        OP_STU => CdrField::Nested,
        _ => CdrField::Unknown(type_code),
    }
}

// --------------------------------------------------------------------------
// TypeSchema
// --------------------------------------------------------------------------

/// Combined type schema: field names from TypeObject + field types from m_ops.
#[derive(Debug, Default)]
pub struct TypeSchema {
    pub idl: String,
    pub field_names: Vec<String>,
    pub fields: Vec<CdrField>,
}

impl TypeSchema {
    /// Parse field names from the JSON returned by `zenrc_typeobj_to_json`.
    ///
    /// JSON format: `{"idl":"...","fields":[{"name":"f","tid":N},...] }`
    pub fn from_json(json_str: &str) -> Option<Self> {
        let v: Value = serde_json::from_str(json_str).ok()?;
        let idl = v["idl"].as_str().unwrap_or("").to_owned();
        let fields_arr = v["fields"].as_array()?;
        let field_names: Vec<String> = fields_arr
            .iter()
            .filter_map(|f| f["name"].as_str().map(|s| s.to_owned()))
            .collect();
        Some(TypeSchema { idl, field_names, fields: Vec::new() })
    }

    /// Populate field types from m_ops (call after `from_json`).
    pub fn with_m_ops(mut self, m_ops: &[u32]) -> Self {
        self.fields = parse_m_ops_fields(m_ops);
        self
    }

    pub fn has_fields(&self) -> bool {
        !self.field_names.is_empty()
    }
}

/// Parse m_ops into a flat list of `CdrField` entries.
pub fn parse_m_ops_fields(ops: &[u32]) -> Vec<CdrField> {
    let mut fields = Vec::new();
    let mut i = 0;

    while i < ops.len() {
        let op = ops[i];
        let opcode = op & DDS_OP_MASK;

        // RTS (0x00…) or any zero word — end of instructions
        if opcode == 0 {
            break;
        }
        // Skip non-ADR instructions at top level (DLC=4, PLC=5, JSR=2, …)
        if opcode != DDS_OP_ADR {
            i += 1;
            continue;
        }

        let type_code = ((op & DDS_OP_TYPE_MASK) >> 16) as u8;
        let subtype   = ((op & DDS_OP_SUBTYPE_MASK) >> 8) as u8;
        let flags     = (op & DDS_OP_FLAGS_MASK) as u8;

        let field = match type_code {
            OP_SEQ => CdrField::SeqOf(subtype, 0),
            OP_ARR => {
                let count = if i + 2 < ops.len() { ops[i + 2] } else { 1 };
                CdrField::ArrOf(subtype, 0, count)
            }
            _ => cdr_field_from_code(type_code, flags),
        };

        fields.push(field);

        // Advance past this instruction's words:
        //   BST: ADR + offset + max_len  (3 words)
        //   ARR: ADR + offset + count    (3 words)
        //   STU: ADR + offset + JSR      (3 words)
        //   others:                      (2 words)
        let step = match type_code {
            OP_BST | OP_ARR | OP_STU => 3,
            _ => 2,
        };
        i += step;
    }

    fields
}

// --------------------------------------------------------------------------
// CDR helpers
// --------------------------------------------------------------------------

fn align_to(pos: usize, a: usize) -> usize {
    (pos + a - 1) & !(a - 1)
}

fn read_u16le(buf: &[u8], p: usize, le: bool) -> u16 {
    if p + 2 > buf.len() {
        return 0;
    }
    let b = [buf[p], buf[p + 1]];
    if le {
        u16::from_le_bytes(b)
    } else {
        u16::from_be_bytes(b)
    }
}

fn read_u32le(buf: &[u8], p: usize, le: bool) -> u32 {
    if p + 4 > buf.len() {
        return 0;
    }
    let b: [u8; 4] = buf[p..p + 4].try_into().unwrap_or([0u8; 4]);
    if le {
        u32::from_le_bytes(b)
    } else {
        u32::from_be_bytes(b)
    }
}

fn read_u64le(buf: &[u8], p: usize, le: bool) -> u64 {
    if p + 8 > buf.len() {
        return 0;
    }
    let b: [u8; 8] = buf[p..p + 8].try_into().unwrap_or([0u8; 8]);
    if le {
        u64::from_le_bytes(b)
    } else {
        u64::from_be_bytes(b)
    }
}

// --------------------------------------------------------------------------
// CDR decode
// --------------------------------------------------------------------------

/// Decode CDR bytes (including the 4-byte encapsulation header) into a JSON
/// object using `schema` for field names and types.
///
/// Falls back to a hex string if the schema has no fields.
pub fn decode(cdr: &[u8], schema: &TypeSchema) -> Value {
    if cdr.len() < 4 || schema.fields.is_empty() {
        return hex_value(cdr);
    }

    let enc   = cdr[1];
    let is_le = matches!(enc, 0x01 | 0x07); // CDR_LE or XCDR2_LE
    let xcdr2 = matches!(enc, 0x06 | 0x07); // XCDR2 — cap alignment at 4

    let mut pos = 4usize;
    let mut obj = serde_json::Map::new();

    let n = schema.fields.len().min(schema.field_names.len());
    for idx in 0..n {
        if pos >= cdr.len() {
            break;
        }
        let name = &schema.field_names[idx];
        let ftype = &schema.fields[idx];
        let val = decode_field(cdr, &mut pos, ftype, is_le, xcdr2);
        obj.insert(name.clone(), val);
    }

    Value::Object(obj)
}

fn decode_field(
    cdr: &[u8],
    pos: &mut usize,
    field: &CdrField,
    le: bool,
    xcdr2: bool,
) -> Value {
    if *pos >= cdr.len() {
        return Value::Null;
    }

    match field {
        CdrField::Bool => {
            let v = cdr.get(*pos).copied().unwrap_or(0);
            *pos += 1;
            Value::Bool(v != 0)
        }
        CdrField::U8 => {
            let v = cdr.get(*pos).copied().unwrap_or(0);
            *pos += 1;
            Value::Number(v.into())
        }
        CdrField::I8 => {
            let v = cdr.get(*pos).copied().unwrap_or(0) as i8;
            *pos += 1;
            Value::Number((v as i64).into())
        }
        CdrField::U16 => {
            *pos = align_to(*pos, 2);
            let v = read_u16le(cdr, *pos, le);
            *pos += 2;
            Value::Number(v.into())
        }
        CdrField::I16 => {
            *pos = align_to(*pos, 2);
            let v = read_u16le(cdr, *pos, le) as i16;
            *pos += 2;
            Value::Number((v as i64).into())
        }
        CdrField::U32 => {
            *pos = align_to(*pos, 4);
            let v = read_u32le(cdr, *pos, le);
            *pos += 4;
            Value::Number(v.into())
        }
        CdrField::I32 => {
            *pos = align_to(*pos, 4);
            let v = read_u32le(cdr, *pos, le) as i32;
            *pos += 4;
            Value::Number((v as i64).into())
        }
        CdrField::U64 => {
            let a = if xcdr2 { 4 } else { 8 };
            *pos = align_to(*pos, a);
            let v = read_u64le(cdr, *pos, le);
            *pos += 8;
            Value::Number(v.into())
        }
        CdrField::I64 => {
            let a = if xcdr2 { 4 } else { 8 };
            *pos = align_to(*pos, a);
            let v = read_u64le(cdr, *pos, le) as i64;
            *pos += 8;
            Value::Number(v.into())
        }
        CdrField::F32 => {
            *pos = align_to(*pos, 4);
            let v = f32::from_bits(read_u32le(cdr, *pos, le));
            *pos += 4;
            serde_json::json!(v)
        }
        CdrField::F64 => {
            let a = if xcdr2 { 4 } else { 8 };
            *pos = align_to(*pos, a);
            let v = f64::from_bits(read_u64le(cdr, *pos, le));
            *pos += 8;
            serde_json::json!(v)
        }
        CdrField::Str => {
            *pos = align_to(*pos, 4);
            let len = read_u32le(cdr, *pos, le) as usize;
            *pos += 4;
            if len == 0 || *pos + len > cdr.len() {
                Value::String(String::new())
            } else {
                let end = *pos + len;
                let bytes = &cdr[*pos..end];
                // Strip the null terminator CycloneDDS includes in the length
                let s_bytes = if bytes.last() == Some(&0) {
                    &bytes[..bytes.len() - 1]
                } else {
                    bytes
                };
                let s = String::from_utf8_lossy(s_bytes).into_owned();
                *pos = end;
                Value::String(s)
            }
        }
        CdrField::SeqOf(elem_code, elem_flags) => {
            *pos = align_to(*pos, 4);
            let count = read_u32le(cdr, *pos, le) as usize;
            *pos += 4;
            let ef = cdr_field_from_code(*elem_code, *elem_flags);
            let cap = count.min(65536);
            let mut arr = Vec::with_capacity(cap);
            for _ in 0..cap {
                if *pos >= cdr.len() {
                    break;
                }
                arr.push(decode_field(cdr, pos, &ef, le, xcdr2));
            }
            Value::Array(arr)
        }
        CdrField::ArrOf(elem_code, elem_flags, count) => {
            let ef = cdr_field_from_code(*elem_code, *elem_flags);
            let mut arr = Vec::with_capacity(*count as usize);
            for _ in 0..*count {
                if *pos >= cdr.len() {
                    break;
                }
                arr.push(decode_field(cdr, pos, &ef, le, xcdr2));
            }
            Value::Array(arr)
        }
        CdrField::Nested | CdrField::Unknown(_) => {
            let hex = cdr[*pos..]
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join("");
            *pos = cdr.len();
            Value::String(format!("<hex:{hex}>"))
        }
    }
}

fn hex_value(cdr: &[u8]) -> Value {
    Value::String(
        cdr.iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(""),
    )
}

// --------------------------------------------------------------------------
// CDR encode
// --------------------------------------------------------------------------

/// Encode a JSON object to CDR bytes (XCDR1 / CDR_LE format).
///
/// Returns a `Vec<u8>` that includes the 4-byte CDR_LE encapsulation header
/// (`00 01 00 00`).
pub fn encode(json: &Value, schema: &TypeSchema) -> Vec<u8> {
    let mut buf = vec![0x00u8, 0x01, 0x00, 0x00]; // CDR_LE header

    let obj = match json.as_object() {
        Some(o) => o,
        None => return buf,
    };

    let n = schema.fields.len().min(schema.field_names.len());
    for idx in 0..n {
        let fname = &schema.field_names[idx];
        let ftype = &schema.fields[idx];
        let val = obj.get(fname).unwrap_or(&Value::Null);
        encode_field(&mut buf, val, ftype);
    }

    buf
}

fn pad_to(buf: &mut Vec<u8>, align: usize) {
    let rem = buf.len() % align;
    if rem != 0 {
        let pad = align - rem;
        for _ in 0..pad {
            buf.push(0);
        }
    }
}

fn encode_field(buf: &mut Vec<u8>, val: &Value, field: &CdrField) {
    match field {
        CdrField::Bool => {
            buf.push(if val.as_bool().unwrap_or(false) { 1 } else { 0 });
        }
        CdrField::U8 => {
            buf.push(val.as_u64().unwrap_or(0) as u8);
        }
        CdrField::I8 => {
            buf.push(val.as_i64().unwrap_or(0) as i8 as u8);
        }
        CdrField::I16 | CdrField::U16 => {
            pad_to(buf, 2);
            buf.extend_from_slice(&(val.as_i64().unwrap_or(0) as i16).to_le_bytes());
        }
        CdrField::I32 => {
            pad_to(buf, 4);
            buf.extend_from_slice(&(val.as_i64().unwrap_or(0) as i32).to_le_bytes());
        }
        CdrField::U32 => {
            pad_to(buf, 4);
            buf.extend_from_slice(&(val.as_u64().unwrap_or(0) as u32).to_le_bytes());
        }
        CdrField::I64 => {
            pad_to(buf, 8);
            buf.extend_from_slice(&val.as_i64().unwrap_or(0).to_le_bytes());
        }
        CdrField::U64 => {
            pad_to(buf, 8);
            buf.extend_from_slice(&val.as_u64().unwrap_or(0).to_le_bytes());
        }
        CdrField::F32 => {
            pad_to(buf, 4);
            let v = val.as_f64().unwrap_or(0.0) as f32;
            buf.extend_from_slice(&v.to_bits().to_le_bytes());
        }
        CdrField::F64 => {
            pad_to(buf, 8);
            let v = val.as_f64().unwrap_or(0.0);
            buf.extend_from_slice(&v.to_bits().to_le_bytes());
        }
        CdrField::Str => {
            pad_to(buf, 4);
            let s = val.as_str().unwrap_or("");
            let len = (s.len() as u32) + 1; // includes null terminator
            buf.extend_from_slice(&len.to_le_bytes());
            buf.extend_from_slice(s.as_bytes());
            buf.push(0); // null terminator
        }
        CdrField::SeqOf(ec, ef) => {
            pad_to(buf, 4);
            let arr = val.as_array().map(|a| a.as_slice()).unwrap_or(&[]);
            buf.extend_from_slice(&(arr.len() as u32).to_le_bytes());
            let elem = cdr_field_from_code(*ec, *ef);
            for item in arr {
                encode_field(buf, item, &elem);
            }
        }
        CdrField::ArrOf(ec, ef, count) => {
            let elem = cdr_field_from_code(*ec, *ef);
            let arr = val.as_array().map(|a| a.as_slice()).unwrap_or(&[]);
            for i in 0..*count as usize {
                let item = arr.get(i).unwrap_or(&Value::Null);
                encode_field(buf, item, &elem);
            }
        }
        CdrField::Nested | CdrField::Unknown(_) => {
            // Accept a hex string and insert raw bytes
            if let Some(hex_str) = val.as_str() {
                let clean: String = hex_str
                    .chars()
                    .filter(|c| c.is_ascii_hexdigit())
                    .collect();
                if clean.len() % 2 == 0 {
                    let bytes: Vec<u8> = (0..clean.len())
                        .step_by(2)
                        .filter_map(|i| u8::from_str_radix(&clean[i..i + 2], 16).ok())
                        .collect();
                    buf.extend_from_slice(&bytes);
                }
            }
        }
    }
}

// --------------------------------------------------------------------------
// TypeObject query — pure Rust using zenrc_dds XTypes bindings
// --------------------------------------------------------------------------

/// Map a primitive DDS XTypes type code to its IDL type name.
#[allow(non_upper_case_globals)]
fn typeid_d_to_idl_simple(d: u8) -> &'static str {
    use zenrc_dds::{
        DDS_XTypes_EK_COMPLETE, DDS_XTypes_EK_MINIMAL, DDS_XTypes_TI_STRING8_LARGE,
        DDS_XTypes_TI_STRING8_SMALL, DDS_XTypes_TK_ARRAY, DDS_XTypes_TK_BOOLEAN,
        DDS_XTypes_TK_BYTE, DDS_XTypes_TK_CHAR8, DDS_XTypes_TK_CHAR16,
        DDS_XTypes_TK_FLOAT32, DDS_XTypes_TK_FLOAT64, DDS_XTypes_TK_FLOAT128,
        DDS_XTypes_TK_INT8, DDS_XTypes_TK_INT16, DDS_XTypes_TK_INT32,
        DDS_XTypes_TK_INT64, DDS_XTypes_TK_SEQUENCE, DDS_XTypes_TK_STRING8,
        DDS_XTypes_TK_STRUCTURE, DDS_XTypes_TK_UINT8, DDS_XTypes_TK_UINT16,
        DDS_XTypes_TK_UINT32, DDS_XTypes_TK_UINT64,
    };
    match d as u32 {
        DDS_XTypes_TK_BOOLEAN => "boolean",
        DDS_XTypes_TK_BYTE => "octet",
        DDS_XTypes_TK_INT8 => "int8",
        DDS_XTypes_TK_INT16 => "short",
        DDS_XTypes_TK_INT32 => "long",
        DDS_XTypes_TK_INT64 => "long long",
        DDS_XTypes_TK_UINT8 => "uint8",
        DDS_XTypes_TK_UINT16 => "unsigned short",
        DDS_XTypes_TK_UINT32 => "unsigned long",
        DDS_XTypes_TK_UINT64 => "unsigned long long",
        DDS_XTypes_TK_FLOAT32 => "float",
        DDS_XTypes_TK_FLOAT64 => "double",
        DDS_XTypes_TK_FLOAT128 => "long double",
        DDS_XTypes_TK_CHAR8 => "char",
        DDS_XTypes_TK_CHAR16 => "wchar",
        DDS_XTypes_TK_STRING8 => "string",
        DDS_XTypes_TI_STRING8_SMALL | DDS_XTypes_TI_STRING8_LARGE => "string",
        DDS_XTypes_TK_STRUCTURE => "<struct>",
        DDS_XTypes_TK_SEQUENCE => "sequence",
        DDS_XTypes_TK_ARRAY => "array",
        DDS_XTypes_EK_MINIMAL | DDS_XTypes_EK_COMPLETE => "<ref>",
        _ => "unknown",
    }
}

/// Format a `DDS_XTypes_TypeIdentifier` as an IDL type string.
///
/// # Safety
/// `tid` must point to a valid, initialized `DDS_XTypes_TypeIdentifier`.
#[allow(non_upper_case_globals)]
unsafe fn format_member_type(tid: &zenrc_dds::DDS_XTypes_TypeIdentifier) -> String {
    use zenrc_dds::{
        DDS_XTypes_TI_PLAIN_ARRAY_LARGE, DDS_XTypes_TI_PLAIN_ARRAY_SMALL,
        DDS_XTypes_TI_PLAIN_SEQUENCE_LARGE, DDS_XTypes_TI_PLAIN_SEQUENCE_SMALL,
        DDS_XTypes_TI_STRING8_LARGE, DDS_XTypes_TI_STRING8_SMALL,
    };
    match tid._d as u32 {
        DDS_XTypes_TI_STRING8_SMALL => {
            let bound = unsafe { tid._u.string_sdefn.bound } as u32;
            if bound == 0 {
                "string".to_string()
            } else {
                format!("string<{}>", bound)
            }
        }
        DDS_XTypes_TI_STRING8_LARGE => "string".to_string(),
        DDS_XTypes_TI_PLAIN_SEQUENCE_SMALL => {
            let elem = unsafe { tid._u.seq_sdefn.element_identifier };
            if !elem.is_null() {
                format!("sequence<{}>", typeid_d_to_idl_simple(unsafe { (*elem)._d }))
            } else {
                "sequence<unknown>".to_string()
            }
        }
        DDS_XTypes_TI_PLAIN_SEQUENCE_LARGE => {
            let elem = unsafe { tid._u.seq_ldefn.element_identifier };
            if !elem.is_null() {
                format!("sequence<{}>", typeid_d_to_idl_simple(unsafe { (*elem)._d }))
            } else {
                "sequence<unknown>".to_string()
            }
        }
        DDS_XTypes_TI_PLAIN_ARRAY_SMALL => {
            let sdefn = unsafe { &tid._u.array_sdefn };
            if !sdefn.element_identifier.is_null() && sdefn.array_bound_seq._length > 0 {
                let count = unsafe { *sdefn.array_bound_seq._buffer } as u32;
                let elem_t = typeid_d_to_idl_simple(unsafe { (*sdefn.element_identifier)._d });
                format!("{}[{}]", elem_t, count)
            } else {
                "array".to_string()
            }
        }
        DDS_XTypes_TI_PLAIN_ARRAY_LARGE => {
            let ldefn = unsafe { &tid._u.array_ldefn };
            if !ldefn.element_identifier.is_null() && ldefn.array_bound_seq._length > 0 {
                let count = unsafe { *ldefn.array_bound_seq._buffer } as u32;
                let elem_t = typeid_d_to_idl_simple(unsafe { (*ldefn.element_identifier)._d });
                format!("{}[{}]", elem_t, count)
            } else {
                "array".to_string()
            }
        }
        _ => typeid_d_to_idl_simple(tid._d).to_string(),
    }
}

/// Parse a `DDS_XTypes_TypeObject` and return a JSON string with IDL and field info.
///
/// # Safety
/// `to` must point to a valid, initialized `DDS_XTypes_TypeObject`.
unsafe fn parse_typeobj(to: *const zenrc_dds::DDS_XTypes_TypeObject) -> Option<String> {
    use zenrc_dds::{DDS_XTypes_EK_COMPLETE, DDS_XTypes_TK_STRUCTURE};
    let to = unsafe { &*to };
    if to._d as u32 != DDS_XTypes_EK_COMPLETE {
        return None;
    }
    let complete = unsafe { &to._u.complete };
    if complete._d as u32 != DDS_XTypes_TK_STRUCTURE {
        return None;
    }
    let st = unsafe { &complete._u.struct_type };
    let type_name =
        unsafe { CStr::from_ptr(st.header.detail.type_name.as_ptr()) }.to_string_lossy();
    let n = st.member_seq._length as usize;
    let mut idl = format!("struct {} {{\n", type_name);
    let mut fields = Vec::with_capacity(n);
    for i in 0..n {
        let m = unsafe { &*st.member_seq._buffer.add(i) };
        let name =
            unsafe { CStr::from_ptr(m.detail.name.as_ptr()) }.to_string_lossy();
        let type_str = unsafe { format_member_type(&m.common.member_type_id) };
        let tid = m.common.member_type_id._d as u32;
        idl.push_str(&format!("  {} {};\n", type_str, name));
        fields.push(format!("{{\"name\":\"{}\",\"tid\":{}}}", name, tid));
    }
    idl.push_str("};");
    let idl_json = idl.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n");
    Some(format!(
        "{{\"idl\":\"{}\",\"fields\":[{}]}}",
        idl_json,
        fields.join(",")
    ))
}

/// Fetch the TypeObject via DDS XTypes and return IDL + field info as a JSON string.
///
/// Returns `None` if type discovery fails or the topic type is not a structure.
///
/// # Safety
/// `dp` must be a valid DDS participant handle; `ti` must be a valid, owned
/// `dds_typeinfo_t*` obtained from `dds_get_typeinfo`.
pub unsafe fn query_typeobj_json(
    dp: dds_entity_t,
    ti: *mut dds_typeinfo_t,
    timeout_ns: i64,
) -> Option<String> {
    use zenrc_dds::{
        DDS_XTypes_TypeObject, dds_free_typeobj, dds_get_typeobj, dds_typeobj_t,
        ddsi_typeinfo_complete_typeid, ddsi_typeinfo_minimal_typeid, ddsi_typeid_is_none,
    };
    if ti.is_null() {
        return None;
    }
    // Prefer complete type ID, fall back to minimal.
    let type_id = unsafe { ddsi_typeinfo_complete_typeid(ti as *const _) };
    let type_id = if type_id.is_null() || unsafe { ddsi_typeid_is_none(type_id) } {
        let min = unsafe { ddsi_typeinfo_minimal_typeid(ti as *const _) };
        if min.is_null() || unsafe { ddsi_typeid_is_none(min) } {
            return None;
        }
        min
    } else {
        type_id
    };
    let mut typeobj: *mut dds_typeobj_t = std::ptr::null_mut();
    let ret = unsafe { dds_get_typeobj(dp, type_id, timeout_ns, &mut typeobj) };
    if ret != 0 || typeobj.is_null() {
        return None;
    }
    let result = unsafe { parse_typeobj(typeobj as *const DDS_XTypes_TypeObject) };
    unsafe { dds_free_typeobj(typeobj) };
    result
}
