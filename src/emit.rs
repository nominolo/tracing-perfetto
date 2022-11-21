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
/// then you can encode this to bytes via:
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
/// For more info see the [official docs on Protobuf
/// encoding](https://developers.google.com/protocol-buffers/docs/encoding).
pub struct ProtoEmitter {
    data: Vec<u8>,
}

impl ProtoEmitter {
    pub fn new() -> Self {
        ProtoEmitter { data: Vec::new() }
    }

    /// Emit a field as a varint.
    ///
    /// Use for protobuf types: int32, int64, uint32, uint64, sint32, sint64,
    /// bool, enum
    pub fn varint_field(&mut self, field_id: u32, data: u64) {
        Self::check_valid_field_id(field_id);
        self.push_varint((field_id << 3) as u64);
        self.push_varint(data);
    }

    pub fn string_field(&mut self, field_id: u32, data: &str) {
        Self::check_valid_field_id(field_id);
        self.push_varint(((field_id << 3) | LENGTH_DELIMITED) as u64);
        self.push_varint(data.len() as u64);
        self.data.extend(data.as_bytes());
    }

    #[allow(unused)]
    pub fn bytes_field(&mut self, field_id: u32, data: &[u8]) {
        Self::check_valid_field_id(field_id);
        self.push_varint(((field_id << 3) | LENGTH_DELIMITED) as u64);
        self.push_varint(data.len() as u64);
        self.data.extend(data);
    }

    pub fn double_field(&mut self, field_id: u32, data: f64) {
        Self::check_valid_field_id(field_id);
        self.push_varint(((field_id << 3) | FIXED_LENGTH_8) as u64);
        let bytes: [u8; 8] = data.to_le_bytes();
        self.data.extend(bytes);
    }

    pub fn clear(&mut self) {
        self.data.clear();
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    #[inline]
    fn check_valid_field_id(field_id: u32) {
        debug_assert!(field_id < 1u32 << 29);
    }

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

    /// Write a varint encoded `size` value using exactly 3 bytes at `offset` in
    /// the output buffer.
    ///
    /// Replaces the existing 3 bytes starting at `offset`.
    ///
    /// This will always use 3 bytes, even if the value could be encoded using
    /// fewer bytes.
    ///
    /// # Panics
    ///
    /// Panics if `size` cannot be varint encoded using 3 bytes.
    ///
    /// Supported range for size: 0..2Mi-1
    fn write_size3(&mut self, offset: usize, size: u32) {
        assert!(size < (1 << 21));
        self.data[offset] = ((size & 0x7f) as u8) | 0x80;
        self.data[offset + 1] = (((size >> 7) & 0x7f) as u8) | 0x80;
        self.data[offset + 2] = ((size >> 14) & 0x7f) as u8;
    }

    // Like `write_size3` but using only two bytes.
    //
    // Supported range for size: 0..16Ki-1
    fn write_size2(&mut self, offset: usize, size: u32) {
        assert!(size < (1 << 14));
        self.data[offset] = ((size & 0x7f) as u8) | 0x80;
        self.data[offset + 1] = ((size >> 7) & 0x7f) as u8;
    }

    pub fn nested<F>(&mut self, field_id: u32, mut build: F)
    where
        F: FnMut(&mut ProtoEmitter) -> (),
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
        build(self);
        let size = self.data.len() - ofs;
        self.write_size3(ofs - 3, size as u32);
    }

    pub fn nested_small<F>(&mut self, field_id: u32, mut build: F)
    where
        F: FnMut(&mut ProtoEmitter) -> (),
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
        build(self);
        let size = self.data.len() - ofs;
        self.write_size2(ofs - 2, size as u32);
    }
}

const LENGTH_DELIMITED: u32 = 2;
const FIXED_LENGTH_8: u32 = 1;
