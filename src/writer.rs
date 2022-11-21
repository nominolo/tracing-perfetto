use std::{
    fs::File,
    io::{BufWriter, Write},
    ops::Deref,
    path::PathBuf,
    sync::Arc,
};

use crossbeam_channel::Receiver;
use dashmap::DashMap;

use crate::{
    annotations::{FieldValue, SpanValue},
    emit::ProtoEmitter,
    intern::Interned,
    message::{Buffer, FinishedBuf, Message, Timestamp},
    packet::{
        self, DebugAnnotation, DebugAnnotationName, DebugValue, Emit, EventName, InternedData,
        PacketData, TracePacket, TracePacketDefaults, TrackDescriptor, TrackEvent,
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
    let mut interned = Interned::new();
    let trusted_uid = 42;

    for finished_buffer in finished_buffers {
        match finished_buffer {
            FinishedBuf::Buf(buffer) => {
                //println!("Got buffer");
                process_buffer(
                    &mut emitter,
                    &mut writer,
                    &mut interned,
                    trusted_uid,
                    buffer,
                )?
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
                        process_buffer(
                            &mut emitter,
                            &mut writer,
                            &mut interned,
                            trusted_uid,
                            buffer,
                        )?;
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
    interned: &mut Interned,
    trusted_uid: i32,
    buffer: Arc<Buffer>,
) -> std::io::Result<()> {
    let thread_id = buffer.thread_id;

    // Normally, we should be the only ones having access to this buffer. But
    // when the program exits we drain any remaining buffers. These buffers
    // might still get messages from their threads. If we just call `pop` until
    // we get `None` we may never finish if the producer is faster than us. So
    // we just take note of the buffer size at the beginning and then consume
    // that much. Any messages added after we started will thus simply get
    // ignored.
    let mut items = buffer.queue.len();

    //println!("Processing: {items:?} items");

    if items == 0 {
        return Ok(());
    }

    // Reset interned data
    let seq_id = start_sequence(emitter, writer, trusted_uid, thread_id)?;
    interned.reset();

    while let Some(msg) = buffer.queue.pop() {
        if items == 0 {
            break;
        }
        items -= 1;
        emitter.clear();

        match msg {
            Message::NewThread(_, name) => {
                new_thread(emitter, writer, trusted_uid, seq_id, thread_id, name)?;
            }
            Message::Enter { ts, label, args } => enter(
                emitter,
                writer,
                interned,
                trusted_uid,
                seq_id,
                ts,
                label,
                args,
            )?,
            Message::Exit { ts, label, args: _ } => {
                exit(emitter, writer, interned, trusted_uid, seq_id, ts, label)?
            }
            Message::Event { ts, label, args } => event(
                emitter,
                writer,
                interned,
                trusted_uid,
                seq_id,
                ts,
                label,
                args,
            )?,
        }
    }
    Ok(())
}

fn start_sequence<W: Write>(
    emitter: &mut ProtoEmitter,
    writer: &mut BufWriter<W>,
    trusted_uid: i32,
    thread_id: u64,
) -> std::io::Result<u32> {
    let trusted_packet_sequence_id = 1 + thread_id as u32;
    // This packet is needed so we can use string interning. It also
    // defines the default track uuid for this thread. Because we
    // use one trusted sequence id per thread, we should never have
    // to override the track uuid in a packet.
    let msg0 = TracePacket {
        timestamp: 1,
        data: PacketData::None,
        sequence_flags: SEQ_INCREMENTAL_STATE_CLEARED,
        trusted_uid,
        trusted_packet_sequence_id,
        interned_data: None,
        trace_packet_defaults: Some(TracePacketDefaults {
            timestamp_clock_id: 6, // BUILTIN_CLOCK_BOOTTIME
            track_event_defaults: Some(TrackEventDefaults {
                track_uuid: 8765 * (thread_id as u64 + 1),
            }),
        }),
    };

    emitter.nested(1, |out| msg0.emit(out));
    writer.write(emitter.as_bytes())?;
    Ok(trusted_packet_sequence_id)
}

fn new_thread<W: Write>(
    emitter: &mut ProtoEmitter,
    writer: &mut BufWriter<W>,
    trusted_uid: i32,
    trusted_packet_sequence_id: u32,
    thread_id: u64,
    thread_name: Box<String>,
) -> std::io::Result<()> {
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
        trusted_packet_sequence_id,
        interned_data: None,
        trace_packet_defaults: None,
    };

    // emitter.nested(1, |out| msg0.emit(out));
    emitter.nested(1, |out| msg1.emit(out));
    writer.write(emitter.as_bytes())?;
    Ok(())
}

fn intern_event_name(interned: &mut Interned, label: &'static str) -> (u64, Option<InternedData>) {
    let (name_iid, is_new) = interned.event_name(label);
    let interned_data = if is_new {
        Some(InternedData {
            event_names: vec![EventName {
                iid: name_iid,
                name: label.to_string(),
            }],
            ..Default::default()
        })
    } else {
        None
    };
    (name_iid, interned_data)
}

fn build_debug_annotations(
    interned: &mut Interned,
    args: Option<Arc<Vec<FieldValue>>>,
    interned_data: Option<InternedData>,
) -> (Vec<DebugAnnotation>, Option<InternedData>) {
    if let Some(args) = args {
        let mut res = Vec::with_capacity(args.len());
        let mut local_interned_data = interned_data.unwrap_or_default();

        for arg in &*args {
            let (name_iid, is_new) = interned.debug_annotation_name(arg.name);
            if is_new {
                local_interned_data
                    .debug_annotation_names
                    .push(DebugAnnotationName {
                        iid: name_iid,
                        name: arg.name.to_string(),
                    })
            }
            let value: DebugValue = match &arg.value {
                SpanValue::Bool(b) => DebugValue::Bool(*b),
                SpanValue::U64(n) => DebugValue::Uint(*n),
                SpanValue::I64(i) => DebugValue::Int(*i),
                SpanValue::Str(s) => DebugValue::String(s.to_string()),
                SpanValue::F64(f) => DebugValue::Double(*f),
            };
            res.push(DebugAnnotation {
                name: packet::IString::Interned(name_iid),
                value,
            })
        }

        (
            res,
            if local_interned_data.is_empty() {
                None
            } else {
                Some(local_interned_data)
            },
        )
    } else {
        (Vec::new(), interned_data)
    }
}

fn enter<W: Write>(
    emitter: &mut ProtoEmitter,
    writer: &mut BufWriter<W>,
    interned: &mut Interned,
    trusted_uid: i32,
    trusted_packet_sequence_id: u32,
    timestamp: Timestamp,
    label: &'static str,
    args: Option<Arc<Vec<FieldValue>>>,
) -> std::io::Result<()> {
    let (event_iid, interned_data) = intern_event_name(interned, label);

    let (debug_annotations, interned_data) = build_debug_annotations(interned, args, interned_data);

    let msg = TracePacket {
        timestamp,
        sequence_flags: SEQ_NEEDS_INCREMENTAL_STATE,
        data: PacketData::TrackEvent(TrackEvent {
            event_type: packet::EventType::SliceBegin,
            name: packet::IString::Interned(event_iid),
            debug_annotations,
        }),
        trusted_uid,
        trusted_packet_sequence_id,
        interned_data,
        trace_packet_defaults: None,
    };

    emitter.nested(1, |out| msg.emit(out));
    writer.write(emitter.as_bytes())?;

    Ok(())
}

fn exit<W: Write>(
    emitter: &mut ProtoEmitter,
    writer: &mut BufWriter<W>,
    interned: &mut Interned,
    trusted_uid: i32,
    trusted_packet_sequence_id: u32,
    timestamp: Timestamp,
    label: &'static str,
) -> std::io::Result<()> {
    let (event_iid, interned_data) = intern_event_name(interned, label);
    let msg = TracePacket {
        timestamp,
        sequence_flags: SEQ_NEEDS_INCREMENTAL_STATE,
        data: PacketData::TrackEvent(TrackEvent {
            event_type: packet::EventType::SliceEnd,
            name: packet::IString::Interned(event_iid),
            debug_annotations: Vec::new(),
        }),
        trusted_uid,
        trusted_packet_sequence_id,
        interned_data,
        trace_packet_defaults: None,
    };

    emitter.nested(1, |out| msg.emit(out));
    writer.write(emitter.as_bytes())?;

    Ok(())
}

fn event<W: Write>(
    emitter: &mut ProtoEmitter,
    writer: &mut BufWriter<W>,
    interned: &mut Interned,
    trusted_uid: i32,
    trusted_packet_sequence_id: u32,
    timestamp: Timestamp,
    label: &'static str,
    args: Option<Arc<Vec<FieldValue>>>,
) -> std::io::Result<()> {
    let (event_iid, interned_data) = intern_event_name(interned, label);
    let (debug_annotations, interned_data) = build_debug_annotations(interned, args, interned_data);
    let msg = TracePacket {
        timestamp,
        sequence_flags: SEQ_NEEDS_INCREMENTAL_STATE,
        data: PacketData::TrackEvent(TrackEvent {
            event_type: packet::EventType::Instant,
            name: packet::IString::Interned(event_iid),
            debug_annotations,
        }),
        trusted_uid,
        trusted_packet_sequence_id,
        interned_data,
        trace_packet_defaults: None,
    };

    emitter.nested(1, |out| msg.emit(out));
    writer.write(emitter.as_bytes())?;

    Ok(())
}
