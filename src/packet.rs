use crate::emit::ProtoEmitter;

pub struct TracePacket {
    pub timestamp: u64,
    pub data: PacketData,
    pub sequence_flags: u32,
    pub trusted_uid: i32,
    pub trusted_packet_sequence_id: u32,
    pub interned_data: Option<InternedData>,
    pub trace_packet_defaults: Option<TracePacketDefaults>, // = 59
}

pub const SEQ_INCREMENTAL_STATE_CLEARED: u32 = 1;
pub const SEQ_NEEDS_INCREMENTAL_STATE: u32 = 2;

pub enum PacketData {
    TrackEvent(TrackEvent),           // 11
    TrackDescriptor(TrackDescriptor), // 60
    None,
}

#[derive(Debug, Clone)]
pub enum IString {
    Plain(String),
    Interned(u64),
}

pub struct TrackEvent {
    pub event_type: EventType,
    pub name: IString,
    pub debug_annotations: Vec<DebugAnnotation>,
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

pub struct TracePacketDefaults {
    pub timestamp_clock_id: u32,                          // 58
    pub track_event_defaults: Option<TrackEventDefaults>, // 11
}

impl Emit for TracePacketDefaults {
    fn emit(&self, out: &mut ProtoEmitter) {
        out.varint_field(58, self.timestamp_clock_id as u64);
        if let Some(defaults) = self.track_event_defaults.as_ref() {
            out.nested(11, |out| defaults.emit(out));
        }
    }
}

pub struct TrackEventDefaults {
    pub track_uuid: u64, // 11
}

impl Emit for TrackEventDefaults {
    fn emit(&self, out: &mut ProtoEmitter) {
        out.varint_field(11, self.track_uuid)
    }
}

pub struct TrackDescriptor {
    pub uuid: u64,
    pub name: String,
}

impl Emit for TrackDescriptor {
    fn emit(&self, out: &mut ProtoEmitter) {
        out.varint_field(1, self.uuid);
        out.string_field(2, &self.name);
    }
}

pub struct InternedData {
    pub event_names: Vec<EventName>,
}

pub struct EventName {
    pub iid: u64,
    pub name: String,
}

impl Emit for EventName {
    fn emit(&self, out: &mut ProtoEmitter) {
        out.varint_field(1, self.iid);
        out.string_field(2, &self.name);
    }
}

impl Emit for InternedData {
    fn emit(&self, out: &mut ProtoEmitter) {
        for event_name in &self.event_names {
            out.nested_small(2, |out| {
                event_name.emit(out);
            });
        }
    }
}

// impl EventName {
//     pub fn emit(&self, out: &mut Vec<u8>) {
//         emit_varint(varint_id(1), out);
//         emit_varint(self.iid, out);
//         emit_varint(len_delim(2), out);
//         emit_string(&self.name, out);
//     }
// }

pub trait Emit {
    fn emit(&self, out: &mut ProtoEmitter);
}

impl Emit for TrackEvent {
    fn emit(&self, out: &mut ProtoEmitter) {
        out.varint_field(9, self.event_type.id());
        match &self.name {
            IString::Plain(s) => out.string_field(23, s),
            IString::Interned(iid) => out.varint_field(10, *iid),
        }
        for debug_ann in &self.debug_annotations {
            out.nested(4, |out| debug_ann.emit(out));
        }
    }
}

impl Emit for TracePacket {
    fn emit(&self, out: &mut ProtoEmitter) {
        //let mut buf = ProtoEmitter::new();
        out.varint_field(8, self.timestamp);
        out.varint_field(3, self.trusted_uid as u32 as u64); // not sint32, so no zigzag
        out.varint_field(13, self.sequence_flags as u64);
        out.varint_field(10, self.trusted_packet_sequence_id as u64);
        match &self.data {
            PacketData::None => (),
            PacketData::TrackEvent(ev) => {
                out.nested(11, |out| ev.emit(out));
                // ev.emit(&mut buf);
                // out.bytes_field(11, buf.as_bytes());
            }
            PacketData::TrackDescriptor(ev) => {
                out.nested(60, |out| ev.emit(out));
                // ev.emit(&mut buf);
                // out.bytes_field(60, buf.as_bytes());
            }
        }
        if let Some(interned_data) = self.interned_data.as_ref() {
            out.nested(12, |out| interned_data.emit(out));
            // buf.clear();
            // interned_data.emit(&mut buf);
            // out.bytes_field(12, buf.as_bytes());
        }
        if let Some(defaults) = self.trace_packet_defaults.as_ref() {
            out.nested(59, |out| defaults.emit(out))
            // buf.clear();
            // defaults.emit(&mut buf);
            // out.bytes_field(59, buf.as_bytes());
        }
    }
}

// fn zigzag_encode_i32(val: i32) -> u32 {
//     ((val as u32).wrapping_add(val as u32)) ^ ((val >> 31) as u32)
// }

// fn zigzag_decode_i32(val: u32) -> i32 {
//     ((val >> 1) ^ (0_u32.wrapping_sub(val & 1))) as i32
// }

// fn zigzag_i32(val: i32) -> u64 {
//     zigzag_encode_i32(val) as u64
// }

#[derive(Debug, Clone)]

pub struct DebugAnnotation {
    pub name: IString,
    pub value: DebugValue,
}

pub struct DebugAnnotationName {
    pub iid: u64,
    pub name: String,
}

impl Emit for DebugAnnotationName {
    fn emit(&self, out: &mut ProtoEmitter) {
        out.varint_field(1, self.iid);
        out.string_field(2, &self.name);
    }
}

#[derive(Debug, Clone)]
pub enum DebugValue {
    Bool(bool),
    Uint(u64),
    Int(i64),
    Double(f64),
    String(String),
    Dict(Vec<DebugAnnotation>),
    Array(Vec<DebugValue>),
}

impl Emit for DebugAnnotation {
    fn emit(&self, out: &mut ProtoEmitter) {
        match &self.name {
            IString::Plain(s) => out.string_field(10, s),
            IString::Interned(n) => out.varint_field(1, *n),
        }
        emit_value(&self.value, out);
    }
}

fn emit_value(value: &DebugValue, out: &mut ProtoEmitter) {
    match value {
        DebugValue::Bool(b) => out.varint_field(2, *b as u64),
        DebugValue::Uint(n) => out.varint_field(3, *n),
        DebugValue::Int(n) => out.varint_field(4, *n as u64),
        DebugValue::Double(d) => out.double_field(5, *d),
        DebugValue::String(s) => out.string_field(6, s),
        DebugValue::Dict(anns) => {
            for ann in anns {
                out.nested_small(11, |out| ann.emit(out));
            }
        }
        DebugValue::Array(vals) => {
            for val in vals {
                out.nested_small(12, |out| emit_value(val, out));
            }
        }
    }
}
