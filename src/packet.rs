use crate::emit::ProtoEmitter;


pub struct TracePacket {
    pub timestamp: u64,
    pub data: PacketData,
    pub trusted_uid: i32,
    pub trusted_packet_sequence_id: u32,
    pub interned_data: Option<InternedData>,
}

type InternedData = ();

pub enum PacketData {
    TrackEvent(TrackEvent),
}

pub struct TrackEvent {
    pub event_type: EventType,
    pub name: String,
}

pub enum EventType {
    Instant,
    SliceBegin,
    SliceEnd,
}

impl EventType {
    fn id(&self) -> u64 {
        match self {
            EventType::Instant => 3,
            EventType::SliceBegin => 1,
            EventType::SliceEnd => 2,
        }
    }
}
pub trait Emit {
    fn emit(&self, out: &mut ProtoEmitter);
}

impl Emit for TrackEvent {
    fn emit(&self, out: &mut ProtoEmitter) {
        out.varint_field(9, self.event_type.id());
        out.string_field(23, &self.name);
    }
}

impl Emit for TracePacket {
    fn emit(&self, out: &mut ProtoEmitter) {
        out.varint_field(8, self.timestamp);
        out.varint_field(3, zigzag_i32(self.trusted_uid));
        out.varint_field(10, self.trusted_packet_sequence_id as u64);
        match &self.data {
            PacketData::TrackEvent(ev) => {
                let mut em = ProtoEmitter::new();
                ev.emit(&mut em);
                out.bytes_field(11, em.as_bytes());
            },
        }
    }
}

fn zigzag_encode_i32(val: i32) -> u32 {
    ((val as u32).wrapping_add(val as u32)) ^ ((val >> 31) as u32)
}

// fn zigzag_decode_i32(val: u32) -> i32 {
//     ((val >> 1) ^ (0_u32.wrapping_sub(val & 1))) as i32
// }

fn zigzag_i32(val: i32) -> u64 {
    zigzag_encode_i32(val) as u64
}