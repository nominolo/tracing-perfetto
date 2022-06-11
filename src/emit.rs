pub struct ProtoEmitter {
    data: Vec<u8>,
}

impl ProtoEmitter {
    pub fn new() -> Self {
        ProtoEmitter { data: Vec::new() }
    }

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

    pub fn bytes_field(&mut self, field_id: u32, data: &[u8]) {
        Self::check_valid_field_id(field_id);
        self.push_varint(((field_id << 3) | LENGTH_DELIMITED) as u64);
        self.push_varint(data.len() as u64);
        self.data.extend(data);
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

    // TODO: Optimize via SIMD
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
}

const LENGTH_DELIMITED: u32 = 2;
