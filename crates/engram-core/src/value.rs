//! Value types: numeric scalars for scanning and richer data types for read/write.

use crate::{Error, Result};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NumType {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
}

impl NumType {
    pub const ALL: [NumType; 10] =
        [Self::I8, Self::I16, Self::I32, Self::I64, Self::U8, Self::U16, Self::U32, Self::U64, Self::F32, Self::F64];

    pub fn size(self) -> usize {
        match self {
            Self::I8 | Self::U8 => 1,
            Self::I16 | Self::U16 => 2,
            Self::I32 | Self::U32 | Self::F32 => 4,
            Self::I64 | Self::U64 | Self::F64 => 8,
        }
    }

    pub fn is_float(self) -> bool {
        matches!(self, Self::F32 | Self::F64)
    }

    pub fn is_signed(self) -> bool {
        matches!(self, Self::I8 | Self::I16 | Self::I32 | Self::I64)
    }

    /// Alignment used by "aligned" scans. 8-byte values are often only 4-aligned
    /// inside packed game structs, so cap at 4 (Cheat Engine's fast-scan default).
    pub fn align(self) -> usize {
        self.size().min(4)
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
            Self::F32 => "f32",
            Self::F64 => "f64",
        }
    }

    pub fn parse_name(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "i8" | "int8" | "sbyte" => Self::I8,
            "i16" | "int16" | "short" => Self::I16,
            "i32" | "int32" | "int" | "dword" | "4 bytes" => Self::I32,
            "i64" | "int64" | "long" | "qword" | "8 bytes" => Self::I64,
            "u8" | "uint8" | "byte" | "1 byte" => Self::U8,
            "u16" | "uint16" | "ushort" | "word" | "2 bytes" => Self::U16,
            "u32" | "uint32" | "uint" => Self::U32,
            "u64" | "uint64" | "ulong" => Self::U64,
            "f32" | "float" | "single" => Self::F32,
            "f64" | "double" => Self::F64,
            _ => return None,
        })
    }

    /// Decode little-endian bytes (at least `size()` long) into raw bits.
    pub fn bits(self, b: &[u8]) -> u64 {
        let mut raw = [0u8; 8];
        raw[..self.size()].copy_from_slice(&b[..self.size()]);
        u64::from_le_bytes(raw)
    }

    pub fn format(self, bits: u64) -> String {
        match self {
            Self::I8 => (bits as u8 as i8).to_string(),
            Self::I16 => (bits as u16 as i16).to_string(),
            Self::I32 => (bits as u32 as i32).to_string(),
            Self::I64 => (bits as i64).to_string(),
            Self::U8 => (bits as u8).to_string(),
            Self::U16 => (bits as u16).to_string(),
            Self::U32 => (bits as u32).to_string(),
            Self::U64 => bits.to_string(),
            Self::F32 => fmt_float(f32::from_bits(bits as u32) as f64, true),
            Self::F64 => fmt_float(f64::from_bits(bits), false),
        }
    }

    pub fn as_f64(self, bits: u64) -> f64 {
        match self {
            Self::I8 => bits as u8 as i8 as f64,
            Self::I16 => bits as u16 as i16 as f64,
            Self::I32 => bits as u32 as i32 as f64,
            Self::I64 => bits as i64 as f64,
            Self::U8 => bits as u8 as f64,
            Self::U16 => bits as u16 as f64,
            Self::U32 => bits as u32 as f64,
            Self::U64 => bits as f64,
            Self::F32 => f32::from_bits(bits as u32) as f64,
            Self::F64 => f64::from_bits(bits),
        }
    }

    /// Text → little-endian bytes. Integers accept decimal, `0x` hex and negative
    /// values for unsigned types (two's complement), with range checks.
    pub fn encode(self, text: &str) -> Result<Vec<u8>> {
        let t = text.trim();
        let size = self.size();
        if self.is_float() {
            let v: f64 = t.parse().map_err(|_| Error::invalid(format!("'{t}' is not a number")))?;
            return Ok(if self == Self::F32 { (v as f32).to_le_bytes().to_vec() } else { v.to_le_bytes().to_vec() });
        }
        let v = parse_int(t)?;
        let bits = size as u32 * 8;
        let (smin, umax) = (-(1i128 << (bits - 1)), (1i128 << bits) - 1);
        let ok = if self.is_signed() { v >= smin && v < (1i128 << (bits - 1)) } else { v >= smin && v <= umax };
        if !ok {
            return Err(Error::new("out_of_range", format!("{t} does not fit in {}", self.name())));
        }
        Ok((v as u64).to_le_bytes()[..size].to_vec())
    }
}

pub fn parse_int(t: &str) -> Result<i128> {
    let t = t.trim().replace('_', "");
    let (neg, body) = match t.strip_prefix('-') {
        Some(b) => (true, b.to_string()),
        None => (false, t.clone()),
    };
    let v = if let Some(h) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X")) {
        i128::from_str_radix(h, 16)
    } else {
        body.parse::<i128>()
    }
    .map_err(|_| Error::invalid(format!("'{t}' is not an integer")))?;
    Ok(if neg { -v } else { v })
}

fn fmt_float(v: f64, single: bool) -> String {
    if v.is_nan() || v.is_infinite() {
        return v.to_string();
    }
    let s = if single { (v as f32).to_string() } else { v.to_string() };
    if s.len() > 24 {
        format!("{v:.6e}")
    } else {
        s
    }
}

impl fmt::Display for NumType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A type for read/write/table entries. Text form: `i32`, `f32`, `ptr`,
/// `bytes:16`, `str:32` (UTF-8), `str16:32` (UTF-16LE, count in characters).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataType {
    Num(NumType),
    Ptr,
    Bytes(usize),
    Str(usize),
    Str16(usize),
}

pub const MAX_BLOB: usize = 4096;

impl DataType {
    pub fn size(self, ptr_size: usize) -> usize {
        match self {
            Self::Num(n) => n.size(),
            Self::Ptr => ptr_size,
            Self::Bytes(n) | Self::Str(n) => n,
            Self::Str16(n) => n * 2,
        }
    }

    pub fn format(self, b: &[u8]) -> String {
        match self {
            Self::Num(n) => n.format(n.bits(b)),
            Self::Ptr => {
                let mut raw = [0u8; 8];
                raw[..b.len().min(8)].copy_from_slice(&b[..b.len().min(8)]);
                crate::hex(u64::from_le_bytes(raw))
            }
            Self::Bytes(_) => hex_bytes(b),
            Self::Str(_) => {
                let end = b.iter().position(|c| *c == 0).unwrap_or(b.len());
                String::from_utf8_lossy(&b[..end]).into_owned()
            }
            Self::Str16(_) => {
                let units: Vec<u16> = b.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).take_while(|u| *u != 0).collect();
                String::from_utf16_lossy(&units)
            }
        }
    }

    pub fn encode(self, text: &str, ptr_size: usize) -> Result<Vec<u8>> {
        match self {
            Self::Num(n) => n.encode(text),
            Self::Ptr => {
                let v = crate::parse_addr(text).ok_or_else(|| Error::invalid(format!("'{text}' is not an address")))?;
                Ok(v.to_le_bytes()[..ptr_size].to_vec())
            }
            Self::Bytes(_) => parse_hex_bytes(text),
            Self::Str(_) => Ok(text.as_bytes().to_vec()),
            Self::Str16(_) => Ok(text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()),
        }
    }
}

impl FromStr for DataType {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        let s = s.trim().to_ascii_lowercase();
        let (head, n) = match s.split_once(':') {
            Some((h, n)) => (h.to_string(), Some(n.parse::<usize>().map_err(|_| Error::invalid(format!("bad length in '{s}'")))?)),
            None => (s.clone(), None),
        };
        let n_or = |d: usize| n.unwrap_or(d).clamp(1, MAX_BLOB);
        Ok(match head.as_str() {
            "ptr" | "pointer" => Self::Ptr,
            "bytes" | "aob" | "hex" => Self::Bytes(n_or(16)),
            "str" | "string" | "utf8" | "text" => Self::Str(n_or(64)),
            "str16" | "string16" | "utf16" | "wstring" => Self::Str16(n_or(64)),
            other => Self::Num(NumType::parse_name(other).ok_or_else(|| {
                Error::invalid(format!("unknown type '{other}' (use i8..i64, u8..u64, f32, f64, ptr, bytes:N, str:N, str16:N)"))
            })?),
        })
    }
}

impl fmt::Display for DataType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Num(n) => f.write_str(n.name()),
            Self::Ptr => f.write_str("ptr"),
            Self::Bytes(n) => write!(f, "bytes:{n}"),
            Self::Str(n) => write!(f, "str:{n}"),
            Self::Str16(n) => write!(f, "str16:{n}"),
        }
    }
}

impl Serialize for DataType {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for DataType {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

pub fn hex_bytes(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" ")
}

pub fn parse_hex_bytes(text: &str) -> Result<Vec<u8>> {
    let clean: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if !clean.len().is_multiple_of(2) {
        return Err(Error::invalid("hex byte string must have an even number of digits"));
    }
    (0..clean.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&clean[i..i + 2], 16).map_err(|_| Error::invalid(format!("bad hex in '{text}'"))))
        .collect()
}

/// Parse an array-of-bytes pattern: "48 8B ?? 05 *". Returns (bytes, mask) with
/// mask 0xFF for fixed bytes and 0x00 for wildcards.
pub fn parse_pattern(text: &str) -> Result<(Vec<u8>, Vec<u8>)> {
    let mut bytes = Vec::new();
    let mut mask = Vec::new();
    for tok in text.split_whitespace() {
        if tok.chars().all(|c| c == '?' || c == '*') {
            bytes.push(0);
            mask.push(0);
        } else {
            let b = u8::from_str_radix(tok, 16).map_err(|_| Error::invalid(format!("bad pattern byte '{tok}'")))?;
            bytes.push(b);
            mask.push(0xFF);
        }
    }
    if bytes.is_empty() || mask.iter().all(|m| *m == 0) {
        return Err(Error::invalid("pattern needs at least one fixed byte"));
    }
    Ok((bytes, mask))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_ranges() {
        assert_eq!(NumType::U8.encode("-1").unwrap(), vec![0xff]);
        assert!(NumType::I8.encode("200").is_err());
        assert_eq!(NumType::I32.encode("0x10").unwrap(), vec![16, 0, 0, 0]);
        assert_eq!(NumType::F32.format(NumType::F32.bits(&100f32.to_le_bytes())), "100");
    }

    #[test]
    fn datatype_roundtrip() {
        for s in ["i32", "f64", "ptr", "bytes:8", "str:12", "str16:4"] {
            assert_eq!(s.parse::<DataType>().unwrap().to_string(), s);
        }
        assert_eq!(DataType::Str16(4).format(&DataType::Str16(4).encode("Hi", 8).unwrap()), "Hi");
        let (b, m) = parse_pattern("48 8b ?? 05").unwrap();
        assert_eq!((b.len(), m[2]), (4, 0));
    }
}
