mod emit;
mod packet;

use std::fs::{File, OpenOptions};
use std::io::Write;

use packet::*;

use crate::emit::ProtoEmitter;

fn main() {
    let mut f = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open("dummy.perfetto-trace")
        .unwrap();

    // TODO: Intern track event name, interning should be possible within the 
    // same TracePacket

    let packets = vec![TracePacket {
        trusted_uid: 9999,
        trusted_packet_sequence_id: 1,
        timestamp: 1142,
        data: PacketData::TrackEvent(TrackEvent {
            event_type: EventType::SliceBegin,
            name: "foo".to_string(),
        }),
        interned_data: None,
    },
    TracePacket {
        trusted_uid: 9999,
        trusted_packet_sequence_id: 1,
        timestamp: 1242,
        data: PacketData::TrackEvent(TrackEvent {
            event_type: EventType::SliceBegin,
            name: "mee".to_string(),
        }),
        interned_data: None,
    },
    TracePacket {
        trusted_uid: 9999,
        trusted_packet_sequence_id: 1,
        timestamp: 1642,
        data: PacketData::TrackEvent(TrackEvent {
            event_type: EventType::SliceEnd,
            name: "mee".to_string(),
        }),
        interned_data: None,
    },
    TracePacket {
        trusted_uid: 9999,
        trusted_packet_sequence_id: 1,
        timestamp: 2142,
        data: PacketData::TrackEvent(TrackEvent {
            event_type: EventType::SliceEnd,
            name: "foo".to_string(),
        }),
        interned_data: None,
    },
    TracePacket {
        trusted_uid: 9999,
        trusted_packet_sequence_id: 1,
        timestamp: 1542,
        data: PacketData::TrackEvent(TrackEvent {
            event_type: EventType::Instant,
            name: "blah".to_string(),
        }),
        interned_data: None,
    }
    ];

    let mut buf = ProtoEmitter::new();
    let mut em = ProtoEmitter::new();
    for packet in &packets {
        buf.clear();
        packet.emit(&mut buf);
        em.bytes_field(1, buf.as_bytes());

        // emit_varint(len_delim(1), &mut message);
        // packet.emit(&mut buf);
        // println!("buf: {:?}", buf);
        // emit_bytes(&buf, &mut message);
    }

    /*
    let inner: Vec<u8> = vec![

    ];

    let mut message: Vec<u8> = vec![
        len_delim(1), // TracePacket
    ];

    let mut trace_packet: Vec<u8> = vec![
        varint_id(8), 42, // timestamp: 42
        len_delim(11), // TrackEvent
    ];

    let track_event: Vec<u8> = vec![
        varint_id(9), 3, // type = INSTANT
        len_delim(23), 1, 3, b'f', b'o', b'o', // (non-interned) name = "foo"
    ];

    trace_packet.push(track_event.len() as u8);
    trace_packet.extend(track_event);

    message.push(trace_packet.len() as u8);
    message.extend(trace_packet);
     */

    println!("{:?}", em.as_bytes());

    f.write(em.as_bytes()).unwrap();

}


/*

fn len_delim(id: u8) -> u64 {
    ((id as u64) << 3) | (LENGTH_DELIMITED as u64)
}

fn varint_id(id: u8) -> u64 {
    (id as u64) << 3
}

const LENGTH_DELIMITED: u8 = 2;

pub struct TracePacket {
    timestamp: u64,
    data: PacketData,
    trusted_uid: i32,
    trusted_packet_sequence_id: u32,
    interned_data: Option<InternedData>,
}

pub enum PacketData {
    TrackEvent(TrackEvent),
}



impl TracePacket {
    pub fn emit(&self, out: &mut Vec<u8>) {
        emit_varint(varint_id(8), out);
        emit_varint(self.timestamp, out);

        emit_varint(varint_id(3), out);
        emit_varint(self.trusted_uid as u64, out);

        emit_varint(varint_id(10), out);
        emit_varint(self.trusted_packet_sequence_id as u64, out);

        match &self.data {
            PacketData::TrackEvent(ev) => {
                emit_varint(len_delim(11), out);
                let mut msg: Vec<u8> = Vec::new();
                ev.emit(&mut msg);
                emit_bytes(&msg, out);
            }
        }
    }
}

pub struct InternedData {
    event_name: Vec<EventName>,
}

pub struct EventName {
    iid: u64,
    name: String,
}

impl EventName {
    pub fn emit(&self, out: &mut Vec<u8>) {
        emit_varint(varint_id(1), out);
        emit_varint(self.iid, out);
        emit_varint(len_delim(2), out);
        emit_string(&self.name, out);
    }
}

pub struct TrackEvent {
    event_type: EventType,
    name: String,
}

pub enum EventType {
    Instant,
    SliceBegin,
    SliceEnd,
}

impl TrackEvent {
    pub fn emit(&self, out: &mut Vec<u8>) {
        emit_varint(varint_id(9), out);
        self.event_type.emit(out);
        emit_varint(len_delim(23), out);
        emit_string(&self.name, out);
    }
}

impl EventType {
    pub fn emit(&self, out: &mut Vec<u8>) {
        match self {
            EventType::Instant => out.push(3),
            EventType::SliceBegin => out.push(1),
            EventType::SliceEnd => out.push(2),
        }
    }
}

fn emit_varint(mut val: u64, out: &mut Vec<u8>) {
//    dbg!(val);
    loop {
        let byte = (val & 0x7f) as u8;
        val >>= 7;
        if val > 0 {
            //dbg!(byte | 0x80);
            out.push(byte | 0x80);
        } else {
            //dbg!(byte);
            out.push(byte);
            return;
        }
    }
}

fn emit_string(s: &str, out: &mut Vec<u8>) {
    let num_bytes = s.len();
    emit_varint(num_bytes as u64, out);
    out.extend(s.as_bytes());
}

fn emit_bytes(b: &[u8], out: &mut Vec<u8>) {
    let num_bytes = b.len();
    emit_varint(num_bytes as u64, out);
    out.extend(b);
}
*/