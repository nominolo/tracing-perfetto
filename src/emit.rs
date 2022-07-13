use std::{error::Error, fmt::Display};

/// A very simple API to write Protobuf messages (Protobuf v2, as that's what
/// the Perfetto spec uses)
///
/// # Example
///
/// If the proto file defines a field as:
///
/// ```text
/// optional uint32 counter_id = 1;
/// optional string description = 3;
/// ```
///
/// then you can write this out as:
///
/// ```ignore
/// # use tracing_perfetto::emit::ProtoEmitter;
/// let mut out = ProtoEmitter::new();
/// out.varint_field(1, 42);  // counter_id has field id 1
/// out.string_field(3, "example");  // description has field id 3
/// assert_eq!(out.as_bytes(), &[
///     8, // field 1, type varint
///     42, // 42 encoded as varint
///     26, // field 3, type string
///     7,  // length of string (in bytes)
///     101, 120, 97, 109, 112, 108, 101 // string
/// ]);
/// ```
///
/// Official docs on encoding: https://developers.google.com/protocol-buffers/docs/encoding
pub struct ProtoEmitter {
    data: Vec<u8>,
}

#[derive(Debug)]
pub enum ProtoEmitterError {
    NestedMessageTooLarge { actual_size: usize, max_size: usize },
}

impl Error for ProtoEmitterError {}

impl Display for ProtoEmitterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProtoEmitterError::NestedMessageTooLarge {
                actual_size,
                max_size,
            } => write!(
                f,
                "Nested protobuf message too large: {actual_size} (max: {max_size})"
            ),
        }
    }
}

impl ProtoEmitter {
    pub fn new() -> Self {
        ProtoEmitter { data: Vec::new() }
    }

    /// Emit a field as a varint.
    ///
    /// Use for protobuf types: int32, int64, uint32, uint64, sint32, sint64, bool, enum
    pub fn varint_field(&mut self, field_id: u32, data: u64) {
        Self::check_valid_field_id(field_id);
        self.push_varint((field_id << 3) as u64);
        self.push_varint(data);
    }

    /// Emit field of type `string`.
    pub fn string_field(&mut self, field_id: u32, data: &str) {
        Self::check_valid_field_id(field_id);
        self.push_varint(((field_id << 3) | LENGTH_DELIMITED) as u64);
        self.push_varint(data.len() as u64);
        self.data.extend(data.as_bytes());
    }

    /// Emit field of type `bytes`.
    #[allow(unused)]
    pub fn bytes_field(&mut self, field_id: u32, data: &[u8]) {
        Self::check_valid_field_id(field_id);
        self.push_varint(((field_id << 3) | LENGTH_DELIMITED) as u64);
        self.push_varint(data.len() as u64);
        self.data.extend(data);
    }

    /// Emit field of type `double`.
    pub fn double_field(&mut self, field_id: u32, data: f64) {
        Self::check_valid_field_id(field_id);
        self.push_varint(((field_id << 3) | FIXED_LENGTH_8) as u64);
        let bytes: [u8; 8] = data.to_le_bytes();
        self.data.extend(bytes);
    }

    /// Clear the output buffer.
    pub fn clear(&mut self) {
        self.data.clear();
    }

    /// Access the bytes in the output buffer.
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    #[inline]
    fn check_valid_field_id(field_id: u32) {
        debug_assert!(field_id < 1u32 << 29);
    }

    // Write value using varint encoding.
    //
    // TODO: Optimize via SIMD?
    fn push_varint(&mut self, mut val: u64) {
        //    dbg!(val);
        loop {
            let byte = (val & 0x7f) as u8;
            val >>= 7;
            if val > 0 {
                //dbg!(byte | 0x80);
                self.data.push(byte | 0x80);
            } else {
                //dbg!(byte);
                self.data.push(byte);
                return;
            }
        }
    }

    // Write a varint encoded `size` value using exactly 3 bytes at `offset` in
    // the output buffer.
    //
    // Replaces the existing 3 bytes.
    //
    // This will always use 3 bytes, even if the value could be encoded using
    // fewer bytes.
    //
    // # Panics
    //
    // Panics if `size` cannot be varint encoded using 3 bytes.
    //
    // Supported range for size: 0..2Mi-1
    fn write_size3(&mut self, offset: usize, size: u32) -> Result<(), ProtoEmitterError> {
        if size >= 1 << 21 {
            return Err(ProtoEmitterError::NestedMessageTooLarge {
                actual_size: size as usize,
                max_size: (1 << 21) - 1,
            });
        }
        self.data[offset] = ((size & 0x7f) as u8) | 0x80;
        self.data[offset + 1] = (((size >> 7) & 0x7f) as u8) | 0x80;
        self.data[offset + 2] = ((size >> 14) & 0x7f) as u8;
        Ok(())
    }

    // Like `write_size3` but using only two bytes.
    //
    // Supported range for size: 0..16Ki-1
    fn write_size2(&mut self, offset: usize, size: u32) -> Result<(), ProtoEmitterError> {
        if size >= 1 << 14 {
            return Err(ProtoEmitterError::NestedMessageTooLarge {
                actual_size: size as usize,
                max_size: (1 << 14) - 1,
            });
        }
        self.data[offset] = ((size & 0x7f) as u8) | 0x80;
        self.data[offset + 1] = ((size >> 7) & 0x7f) as u8;
        Ok(())
    }

    /// Write a nested message using `build` to emit the inner message.
    ///
    /// The total encoded size of the inner message must be less than 2MiB.
    ///
    /// # Panics
    ///
    /// Panics if the inner message is too large.
    pub fn nested<F>(&mut self, field_id: u32, mut build: F) -> Result<(), ProtoEmitterError>
    where
        F: FnMut(&mut ProtoEmitter) -> Result<(), ProtoEmitterError>,
    {
        Self::check_valid_field_id(field_id);
        self.push_varint(((field_id << 3) | LENGTH_DELIMITED) as u64);
        // Reserve 3 bytes, later overwritten by length.
        // Smaller values use non-minimal encoding. 3 bytes => max size 2MiB
        for _ in 0..3 {
            self.data.push(0);
        }
        // Get current write offset
        let ofs = self.data.len();
        build(self)?;
        let size = self.data.len() - ofs;
        self.write_size3(ofs - 3, size as u32)
    }

    /// Like `nested` but message must be less than 16KiB.
    pub fn nested_small<F>(&mut self, field_id: u32, mut build: F) -> Result<(), ProtoEmitterError>
    where
        F: FnMut(&mut ProtoEmitter) -> Result<(), ProtoEmitterError>,
    {
        Self::check_valid_field_id(field_id);
        self.push_varint(((field_id << 3) | LENGTH_DELIMITED) as u64);
        // Reserve 2 bytes, later overwritten by length.
        // Smaller values use non-minimal encoding. 2 bytes => max size 16KiB
        for _ in 0..2 {
            self.data.push(0);
        }
        // Get current write offset
        let ofs = self.data.len();
        build(self)?;
        let size = self.data.len() - ofs;
        self.write_size2(ofs - 2, size as u32)
    }
}

const LENGTH_DELIMITED: u32 = 2;
const FIXED_LENGTH_8: u32 = 1;
