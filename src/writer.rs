use std::{
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
    sync::Arc,
};

use crossbeam_channel::Receiver;
use dashmap::DashMap;

use crate::{
    emit::ProtoEmitter,
    message::{Buffer, FinishedBuf, Message, Timestamp},
    packet::{
        self, Emit, PacketData, TracePacket, TracePacketDefaults, TrackDescriptor, TrackEvent,
        TrackEventDefaults, SEQ_INCREMENTAL_STATE_CLEARED, SEQ_NEEDS_INCREMENTAL_STATE,
    },
};

pub fn writer_thread(
    finished_buffers: Receiver<FinishedBuf>,
    path: Option<PathBuf>,
    pending_buffers: Arc<DashMap<u64, Arc<Buffer>>>,
) -> std::io::Result<()> {
    //println!("Writer thread started");
    // Use default filename if none was provided
    let filename = if let Some(path) = path {
        path
    } else {
        PathBuf::from(format!(
            "trace-{}.pftrace",
            std::time::SystemTime::UNIX_EPOCH
                .elapsed()
                .unwrap()
                .as_millis()
        ))
    };

    let file = File::create(filename).unwrap();
    let mut writer = BufWriter::with_capacity(64 * 1024, file);

    let mut emitter = ProtoEmitter::new();
    let trusted_uid = 42;

    for finished_buffer in finished_buffers {
        match finished_buffer {
            FinishedBuf::Buf(buffer) => {
                //println!("Got buffer");
                process_buffer(&mut emitter, &mut writer, trusted_uid, buffer)?
            }
            FinishedBuf::StopWriterThread => {
                //println!("Worker finishing");

                // We are supposed to stop now. Finish up all the existing
                // buffers.
                //
                // `DashMap` doesn't seem to have a `drain` method so we'll just
                // collect the thread Ids and then remove them one by one. We
                // can't do both at the same time, because the iterator holds a
                // lock, so we'd risk deadlock.
                let ids: Vec<u64> = pending_buffers.iter().map(|m| *m.key()).collect::<Vec<_>>();
                //println!("  {:?}", ids);

                for id in ids {
                    if let Some((_id, buffer)) = pending_buffers.remove(&id) {
                        process_buffer(&mut emitter, &mut writer, trusted_uid, buffer)?;
                    }
                }

                break;
            }
        }
    }

    writer.flush()?;

    Ok(())
}

fn process_buffer<W: Write>(
    emitter: &mut ProtoEmitter,
    writer: &mut BufWriter<W>,
    trusted_uid: i32,
    buffer: Arc<Buffer>,
) -> std::io::Result<()> {
    let thread_id = buffer.thread_id;

    // Normally, we should be the only ones having access to this buffer. But
    // when exiting we're draining any remaining buffers. These buffers might
    // still get messages from their threads. If we just call `pop` until we get
    // `None` we may never finish if the producer is faster than us. So we just
    // take note of the buffer size at the beginning and then consume that much.
    // Any messages added after we started will thus simply get ignored.
    let mut items = buffer.queue.len();

    //println!("Processing: {items:?} items");

    while let Some(msg) = buffer.queue.pop() {
        if items == 0 {
            break;
        }
        items -= 1;
        emitter.clear();

        match msg {
            Message::NewThread(_, name) => {
                new_thread(emitter, writer, trusted_uid, thread_id, name)?;
            }
            Message::Enter { ts, label } => {
                enter(emitter, writer, trusted_uid, thread_id, ts, label)?
            }
            Message::Exit { ts, label } => {
                exit(emitter, writer, trusted_uid, thread_id, ts, label)?
            }
            Message::Event { ts, label } => (),
        }
    }
    Ok(())
}

fn new_thread<W: Write>(
    emitter: &mut ProtoEmitter,
    writer: &mut BufWriter<W>,
    trusted_uid: i32,
    thread_id: u64,
    thread_name: Box<String>,
) -> std::io::Result<()> {
    // This packet is needed so we can use string interning. It also
    // defines the default track uuid for this thread. Because we
    // use one trusted sequence id per thread, we should never have
    // to override the track uuid in a packet.
    let msg0 = TracePacket {
        timestamp: 1,
        data: PacketData::None,
        sequence_flags: SEQ_INCREMENTAL_STATE_CLEARED,
        trusted_uid,
        trusted_packet_sequence_id: 1 + thread_id as u32,
        interned_data: None,
        trace_packet_defaults: Some(TracePacketDefaults {
            timestamp_clock_id: 6, // boottime?
            track_event_defaults: Some(TrackEventDefaults {
                track_uuid: 8765 * (thread_id as u64 + 1),
            }),
        }),
    };

    // thread track descriptor. defines track uuid and track name
    // (= thread name)
    let msg1 = TracePacket {
        timestamp: 1,
        data: PacketData::TrackDescriptor(TrackDescriptor {
            uuid: 8765 * (thread_id as u64 + 1),
            name: *thread_name,
        }),
        sequence_flags: SEQ_NEEDS_INCREMENTAL_STATE,
        trusted_uid,
        trusted_packet_sequence_id: 1 + thread_id as u32,
        interned_data: None,
        trace_packet_defaults: None,
    };

    emitter.nested(1, |out| msg0.emit(out));
    emitter.nested(1, |out| msg1.emit(out));
    writer.write(emitter.as_bytes())?;
    Ok(())
}

fn enter<W: Write>(
    emitter: &mut ProtoEmitter,
    writer: &mut BufWriter<W>,
    trusted_uid: i32,
    thread_id: u64,
    timestamp: Timestamp,
    label: &'static str,
) -> std::io::Result<()> {
    let msg = TracePacket {
        timestamp,
        sequence_flags: SEQ_NEEDS_INCREMENTAL_STATE,
        data: PacketData::TrackEvent(TrackEvent {
            event_type: packet::EventType::SliceBegin,
            name: packet::IString::Plain(label.to_owned()),
            debug_annotations: Vec::new(),
        }),
        trusted_uid,
        trusted_packet_sequence_id: 1 + thread_id as u32,
        interned_data: None,
        trace_packet_defaults: None,
    };

    emitter.nested(1, |out| msg.emit(out));
    writer.write(emitter.as_bytes())?;

    Ok(())
}

fn exit<W: Write>(
    emitter: &mut ProtoEmitter,
    writer: &mut BufWriter<W>,
    trusted_uid: i32,
    thread_id: u64,
    timestamp: Timestamp,
    label: &'static str,
) -> std::io::Result<()> {
    let msg = TracePacket {
        timestamp,
        sequence_flags: SEQ_NEEDS_INCREMENTAL_STATE,
        data: PacketData::TrackEvent(TrackEvent {
            event_type: packet::EventType::SliceEnd,
            name: packet::IString::Plain(label.to_owned()),
            debug_annotations: Vec::new(),
        }),
        trusted_uid,
        trusted_packet_sequence_id: 1 + thread_id as u32,
        interned_data: None,
        trace_packet_defaults: None,
    };

    emitter.nested(1, |out| msg.emit(out));
    writer.write(emitter.as_bytes())?;

    Ok(())
}
