// use std::{
//     cell::RefCell,
//     fs::File,
//     io::{BufWriter, Write},
//     marker::PhantomData,
//     ops::Deref,
//     path::{Path, PathBuf},
//     sync::{
//         atomic::{AtomicU32, Ordering},
//         Arc,
//     },
//     thread::JoinHandle,
//     time::Instant,
// };

// use crossbeam_channel::{Receiver, Sender};
// use intern::Interned;
// use packet::{
//     DebugAnnotation, TracePacketDefaults, TrackDescriptor, TrackEventDefaults,
//     SEQ_INCREMENTAL_STATE_CLEARED,
// };
// use tracing::{field::Visit, span, Subscriber};
// use tracing_subscriber::{layer::Context, registry::LookupSpan, Layer};

// use crate::{
//     emit::ProtoEmitter,
//     packet::{
//         Emit, EventName, InternedData, PacketData, TracePacket, TrackEvent,
//         SEQ_NEEDS_INCREMENTAL_STATE,
//     },
// };

mod annotations;
pub mod buffer;
mod emit;
mod intern;
mod layer;
mod message;
mod packet;
mod writer;
// mod thread_local;

pub use layer::{FlushGuard, PerfettoLayer, PerfettoLayerBuilder};

/*
thread_local! {
        //static OUT: RefCell<Option<Sender<Message>>> = RefCell::new(None);
    static THREAD_ID: RefCell<Option<u32>>  = RefCell::new(None);
}

pub struct PerfettoLayer<S> {
    sender: crossbeam_channel::Sender<Message>,
    start: Instant,
    next_thread_id: AtomicU32,
    include_args: bool,
    _marker: PhantomData<S>,
}

pub struct PerfettoLayerBuilder<S> {
    output_file: Option<PathBuf>,
    include_args: bool,
    _marker: PhantomData<S>,
}

impl<S> PerfettoLayerBuilder<S> {
    pub fn new() -> Self {
        PerfettoLayerBuilder {
            output_file: None,
            include_args: false,
            _marker: PhantomData,
        }
    }

    /// Set the path of the output trace file.
    ///
    /// Defaults to `trace-<unixepoch>.perfetto-trace`.
    pub fn file<P: AsRef<Path>>(mut self, path: P) -> Self {
        let path = PathBuf::from(path.as_ref());
        self.output_file = Some(path);
        self
    }

    pub fn include_args(mut self, include: bool) -> Self {
        self.include_args = include;
        self
    }

    pub fn build(self) -> (PerfettoLayer<S>, FlushGuard) {
        PerfettoLayer::new(self)
    }
}

type ThreadId = u32;
type Timestamp = u64;

#[derive(Debug)]
pub enum Message {
    NewThread(ThreadId, String),
    Enter(
        Timestamp,
        &'static str,
        Option<Arc<Vec<DebugAnnotation>>>,
        ThreadId,
    ),
    Exit(Timestamp, &'static str, ThreadId),
    Event(
        Timestamp,
        &'static str,
        Option<Arc<Vec<DebugAnnotation>>>,
        ThreadId,
    ),
    Drop,
}

impl<S> PerfettoLayer<S> {
    fn new(builder: PerfettoLayerBuilder<S>) -> (Self, FlushGuard) {
        let (tx, rx) = crossbeam_channel::unbounded();
        let worker = std::thread::spawn(move || writer_thread(rx, builder.output_file));

        let start = Instant::now();

        (
            PerfettoLayer {
                sender: tx.clone(),
                start,
                next_thread_id: AtomicU32::new(0),
                include_args: builder.include_args,
                _marker: PhantomData,
            },
            FlushGuard {
                handle: Some(worker),
                sender: tx,
            },
        )
    }

    fn send_message(&self, msg: Message) {
        let _ignore_send_err = self.sender.send(msg);
    }

    fn get_timestamp(&self) -> u64 {
        self.start.elapsed().as_nanos() as u64
    }

    fn get_thread_id(&self) -> (ThreadId, Option<String>) {
        THREAD_ID.with(|value| {
            let thread_id = *value.borrow();
            match thread_id {
                Some(thread_id) => (thread_id, None),
                None => {
                    let id = self.next_thread_id.fetch_add(1, Ordering::SeqCst);
                    value.replace(Some(id));
                    let thread_name = if let Some(name) = std::thread::current().name() {
                        format!("{} {}", name, id)
                    } else {
                        format!("thread {}", id)
                    };
                    (id, Some(thread_name))
                }
            }
        })
    }

    fn init_thread(&self, id: ThreadId, name: String) {
        self.send_message(Message::NewThread(id, name));
    }
}

impl<S> Drop for PerfettoLayer<S> {
    fn drop(&mut self) {
        println!("Dropping layer, TODO: flush buffers")
    }
}

impl<S> Layer<S> for PerfettoLayer<S>
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    fn on_new_span(&self, attrs: &span::Attributes<'_>, id: &span::Id, ctx: Context<'_, S>) {
        if self.include_args {
            let mut v = DebugAnnotationVisitor { infos: Vec::new() };
            attrs.record(&mut v);
            //println!("{:?}", &v.infos);
            ctx.span(id).unwrap().extensions_mut().insert(DebugInfoExt {
                info: Arc::new(v.infos),
            });
        }
    }

    // for handling `Span::record` events
    // fn on_record(&self, _span: &span::Id, _values: &span::Record<'_>, _ctx: Context<'_, S>) {

    // }

    fn on_enter(&self, id: &span::Id, ctx: Context<'_, S>) {
        let span = ctx.span(id);
        let span_name: Option<&'static str> = span.as_ref().map(|s| s.name());
        //let fields = span.map(|s| s.fields())

        let (thread_id, new_thread) = self.get_thread_id();
        if let Some(name) = new_thread {
            self.init_thread(thread_id, name);
        }

        let arg_info = if let Some(span_ref) = span {
            if let Some(info) = span_ref.extensions().get::<DebugInfoExt>() {
                Some(info.info.clone())
            } else {
                None
            }
        } else {
            None
        };

        // println!("on_enter: id={:?}, span_name={:?}, ", id, span_name);
        let msg = Message::Enter(
            self.get_timestamp(),
            span_name.unwrap_or(""),
            arg_info,
            thread_id,
        );
        self.send_message(msg);
    }

    fn on_exit(&self, id: &span::Id, ctx: Context<'_, S>) {
        let span = ctx.span(id);
        let span_name = span.map(|s| s.name());

        let (thread_id, new_thread) = self.get_thread_id();
        if let Some(name) = new_thread {
            self.init_thread(thread_id, name);
        }

        // println!("on_exit: id={:?}, span_name={:?}, ", id, span_name);
        let msg = Message::Exit(self.get_timestamp(), span_name.unwrap_or(""), thread_id);
        self.send_message(msg);
    }

    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let name = event.metadata().name();

        let (thread_id, new_thread) = self.get_thread_id();
        if let Some(name) = new_thread {
            self.init_thread(thread_id, name);
        }

        let arg_info = if self.include_args {
            let mut v = DebugAnnotationVisitor { infos: Vec::new() };
            event.record(&mut v);
            if !v.infos.is_empty() {
                Some(Arc::new(v.infos))
            } else {
                None
            }
        } else {
            None
        };

        let msg = Message::Event(self.get_timestamp(), name, arg_info, thread_id);
        self.send_message(msg);
    }
}

struct DebugInfoExt {
    info: Arc<Vec<DebugAnnotation>>,
}

pub struct FlushGuard {
    handle: Option<JoinHandle<()>>, // An option, so we can `take`
    sender: Sender<Message>,
}

impl Drop for FlushGuard {
    fn drop(&mut self) {
        // Tell writer thread to stop. Sending will fail if thread is already
        // stopped. We can ignore that.
        let _ignore_err = self.sender.send(crate::Message::Drop);
        if let Some(handle) = self.handle.take() {
            if handle.join().is_err() {
                eprintln!("tracing_perfetto: writer thread panicked");
            }
        }
    }
}

// TODO: Use custom type here with `&'static str` for name, and custom enum for
// values. Then interning can be handled in the writer.
#[derive(Debug)]
struct DebugAnnotationVisitor {
    infos: Vec<DebugAnnotation>,
}

impl Visit for DebugAnnotationVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.infos.push(DebugAnnotation {
            name: packet::IString::Plain(field.name().to_string()),
            value: packet::DebugValue::String(format!("{:?}", value)),
        })
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.infos.push(DebugAnnotation {
            name: packet::IString::Plain(field.name().to_string()),
            value: packet::DebugValue::Bool(value),
        })
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.infos.push(DebugAnnotation {
            name: packet::IString::Plain(field.name().to_string()),
            value: packet::DebugValue::Uint(value),
        })
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.infos.push(DebugAnnotation {
            name: packet::IString::Plain(field.name().to_string()),
            value: packet::DebugValue::String(value.to_owned()),
        })
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.infos.push(DebugAnnotation {
            name: packet::IString::Plain(field.name().to_string()),
            value: packet::DebugValue::Int(value),
        })
    }

    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        self.infos.push(DebugAnnotation {
            name: packet::IString::Plain(field.name().to_string()),
            value: packet::DebugValue::Double(value),
        })
    }
}

//fn fields_to_debug_attrs()

// pub fn init_thread()

fn writer_thread(rx: Receiver<Message>, path: Option<PathBuf>) {
    let filename = if let Some(path) = path {
        path
    } else {
        PathBuf::from(format!(
            "trace-{}.perfetto-trace",
            std::time::SystemTime::UNIX_EPOCH
                .elapsed()
                .unwrap()
                .as_secs()
        ))
    };

    let file = File::create(filename).unwrap();
    let mut writer = BufWriter::with_capacity(64 * 1024, file);

    let mut em = ProtoEmitter::new();
    let trusted_uid = 42;
    //    let trusted_packet_sequence_id = 1;
    let mut names: Vec<Interned> = vec![Interned::new()];

    for msg in rx {
        em.clear();
        match msg {
            Message::NewThread(thread_id, thread_name) => {
                names.resize_with((thread_id + 1) as usize, || Interned::new());

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
                        name: thread_name,
                    }),
                    sequence_flags: SEQ_NEEDS_INCREMENTAL_STATE,
                    trusted_uid,
                    trusted_packet_sequence_id: 1 + thread_id as u32,
                    interned_data: None,
                    trace_packet_defaults: None,
                };

                em.nested(1, |out| msg0.emit(out));
                em.nested(1, |out| msg1.emit(out));
                writer.write(em.as_bytes()).unwrap();
            }

            Message::Enter(timestamp, name, debug_info, thread_id) => {
                let (name_iid, added) = names[thread_id as usize].event_name(&name);
                let interned_data = if added {
                    Some(InternedData {
                        event_names: vec![EventName {
                            iid: name_iid,
                            name: name.to_string(),
                        }],
                    })
                } else {
                    None
                };

                let msg = TracePacket {
                    timestamp,
                    sequence_flags: SEQ_NEEDS_INCREMENTAL_STATE,
                    data: PacketData::TrackEvent(TrackEvent {
                        event_type: packet::EventType::SliceBegin,
                        name: packet::IString::Interned(name_iid),
                        debug_annotations: if let Some(info) = debug_info {
                            info.deref().to_vec()
                        } else {
                            Vec::new()
                        }, // debug_annotations: vec![DebugAnnotation {
                           //     name: packet::IString::Plain("hello".to_string()),
                           //     value: packet::DebugValue::Uint(42),
                           // }]
                           // debug_annotations: vec![DebugAnnotation {
                           //     name: packet::IString::Plain("blah".to_string()),
                           //     value: packet::DebugValue::Array(vec![
                           //         packet::DebugValue::Int(32)
                           //     ])
                           // }]
                    }),
                    trusted_uid,
                    trusted_packet_sequence_id: 1 + thread_id,
                    interned_data,
                    trace_packet_defaults: None,
                };

                em.nested(1, |out| msg.emit(out));
                writer.write(em.as_bytes()).unwrap();
            }
            Message::Exit(timestamp, name, thread_id) => {
                let (name_iid, added) = names[thread_id as usize].event_name(&name);
                let interned_data = if added {
                    Some(InternedData {
                        event_names: vec![EventName {
                            iid: name_iid,
                            name: name.to_string(),
                        }],
                    })
                } else {
                    None
                };

                let msg = TracePacket {
                    timestamp,
                    sequence_flags: SEQ_NEEDS_INCREMENTAL_STATE,
                    data: PacketData::TrackEvent(TrackEvent {
                        event_type: packet::EventType::SliceEnd,
                        name: packet::IString::Interned(name_iid), // packet::IString::Plain(name),
                        debug_annotations: Vec::new(),
                    }),
                    trusted_uid,
                    trusted_packet_sequence_id: 1 + thread_id as u32,
                    interned_data,
                    trace_packet_defaults: None,
                };

                em.nested(1, |out| msg.emit(out));
                writer.write(em.as_bytes()).unwrap();
            }

            Message::Event(timestamp, name, debug_info, thread_id) => {
                let (name_iid, added) = names[thread_id as usize].event_name(&name);
                let interned_data = if added {
                    Some(InternedData {
                        event_names: vec![EventName {
                            iid: name_iid,
                            name: name.to_string(),
                        }],
                    })
                } else {
                    None
                };

                let msg = TracePacket {
                    timestamp,
                    sequence_flags: SEQ_NEEDS_INCREMENTAL_STATE,
                    data: PacketData::TrackEvent(TrackEvent {
                        event_type: packet::EventType::Instant,
                        name: packet::IString::Interned(name_iid),
                        debug_annotations: if let Some(info) = debug_info {
                            info.deref().to_vec()
                        } else {
                            Vec::new()
                        },
                    }),
                    trusted_uid,
                    trusted_packet_sequence_id: 1 + thread_id,
                    interned_data,
                    trace_packet_defaults: None,
                };

                em.nested(1, |out| msg.emit(out));
                writer.write(em.as_bytes()).unwrap();
            }

            Message::Drop => break,
        }
        writer.flush().unwrap();
    }
}

#[test]
fn basic() {
    use tracing::info_span;
    use tracing_subscriber::prelude::*;

    let (perfetto_layer, _handle) = PerfettoLayerBuilder::new()
        .file("test-basic.perfetto-trace")
        .build();
    tracing_subscriber::registry().with(perfetto_layer).init();

    let span = info_span!("hello world").entered();
    println!("blah");
    span.exit();
}
*/

#[cfg(test)]
mod tests {
    use tracing::{event, instrument, Level};
    //use tracing_subscriber::prelude::*;

    use crate::PerfettoLayerBuilder;

    #[instrument]
    fn fibonacci(number: usize) -> usize {
        if number < 2 {
            number
        } else {
            fibonacci(number - 1) + fibonacci(number - 2)
        }
    }

    #[test]
    fn fib() {
        use tracing_subscriber::prelude::*;

        let (perfetto_layer, _handle) =
            PerfettoLayerBuilder::new().file("test-fib.pftrace").build();
        tracing_subscriber::registry().with(perfetto_layer).init();

        fibonacci(16);
    }

    #[test]
    fn with_args_fib() {
        use tracing_subscriber::prelude::*;

        let (perfetto_layer, _handle) = PerfettoLayerBuilder::new()
            .file("test-fib-with-args.pftrace")
            .include_args(true)
            .build();
        tracing_subscriber::registry().with(perfetto_layer).init();

        //let data = (42, "forty-two");
        fibonacci(16);
        event!(Level::INFO, "something happened");
        fibonacci(4);
    }
}
