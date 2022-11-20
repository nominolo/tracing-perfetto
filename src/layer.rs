use std::{
    marker::PhantomData,
    path::{Path, PathBuf},
    sync::Arc,
    thread::JoinHandle,
    time::Instant,
};

use crossbeam_channel::Sender;
use crossbeam_queue::ArrayQueue;
use dashmap::DashMap;

use tracing::{field::Visit, span, Subscriber};
use tracing_subscriber::{layer::Context, registry::LookupSpan, Layer};

use crate::message::{FinishedBuf, Message, MessagingState};

pub struct PerfettoLayer<S> {
    started_at: Instant,

    messaging: MessagingState,

    _marker: PhantomData<S>,
}

type ThreadId = u64;

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

pub struct FlushGuard {
    handle: Option<JoinHandle<std::io::Result<()>>>, // An option, so we can `take`
    sender: Sender<FinishedBuf>,
}

impl<S> PerfettoLayer<S> {
    fn new(builder: PerfettoLayerBuilder<S>) -> (Self, FlushGuard) {
        let (msg_state, recv_buf, send_done, in_progress) = MessagingState::new(4096);

        let worker = std::thread::spawn(move || {
            crate::writer::writer_thread(recv_buf, builder.output_file, in_progress)
        });

        let started_at = Instant::now();

        (
            PerfettoLayer {
                started_at,
                messaging: msg_state,
                //include_args: builder.include_args,
                _marker: PhantomData,
            },
            FlushGuard {
                handle: Some(worker),
                sender: send_done,
            },
        )
    }

    fn get_timestamp(&self) -> u64 {
        self.started_at.elapsed().as_nanos() as u64
    }
}

impl Drop for FlushGuard {
    fn drop(&mut self) {
        // Tell writer thread to stop. Sending will fail if thread is already
        // stopped. We can ignore that case.
        let _ignore_err = self.sender.send(FinishedBuf::StopWriterThread);
        if let Some(handle) = self.handle.take() {
            match handle.join() {
                Ok(io_res) => match io_res {
                    Ok(_) => (),
                    Err(io_err) => {
                        eprintln!("tracing_perfetto: I/O write had an error: {:?}", io_err)
                    }
                },
                Err(_join_err) => eprintln!("tracing_perfetto: writer thread panicked"),
            }
        }
    }
}

impl<S> Layer<S> for PerfettoLayer<S>
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    fn on_new_span(&self, attrs: &span::Attributes<'_>, id: &span::Id, ctx: Context<'_, S>) {
        // if self.include_args {
        //     let mut v = DebugAnnotationVisitor { infos: Vec::new() };
        //     attrs.record(&mut v);
        //     //println!("{:?}", &v.infos);
        //     ctx.span(id).unwrap().extensions_mut().insert(DebugInfoExt {
        //         info: Arc::new(v.infos),
        //     });
        // }
    }

    // for handling `Span::record` events
    // fn on_record(&self, _span: &span::Id, _values: &span::Record<'_>, _ctx: Context<'_, S>) {

    // }

    fn on_enter(&self, id: &span::Id, ctx: Context<'_, S>) {
        let span = ctx.span(id);
        let span_name: Option<&'static str> = span.as_ref().map(|s| s.name());
        //let fields = span.map(|s| s.fields())

        // let (thread_id, new_thread) = self.get_thread_id();
        // if let Some(name) = new_thread {
        //     self.init_thread(thread_id, name);
        // }

        // let arg_info = if let Some(span_ref) = span {
        //     if let Some(info) = span_ref.extensions().get::<DebugInfoExt>() {
        //         Some(info.info.clone())
        //     } else {
        //         None
        //     }
        // } else {
        //     None
        // };

        self.messaging.send_message(Message::Enter {
            ts: self.get_timestamp(),
            label: span_name.unwrap_or(""),
        });

        // println!("on_enter: id={:?}, span_name={:?}, ", id, span_name);
        // let msg = Message::Enter(
        //     self.get_timestamp(),
        //     span_name.unwrap_or(""),
        //     arg_info,
        //     thread_id,
        // );
        // self.send_message(msg);
    }

    fn on_exit(&self, id: &span::Id, ctx: Context<'_, S>) {
        let span = ctx.span(id);
        let span_name = span.map(|s| s.name());

        self.messaging.send_message(Message::Exit {
            ts: self.get_timestamp(),
            label: span_name.unwrap_or(""),
        });
    }

    // fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
    //     let name = event.metadata().name();

    //     let (thread_id, new_thread) = self.get_thread_id();
    //     if let Some(name) = new_thread {
    //         self.init_thread(thread_id, name);
    //     }

    //     let arg_info = if self.include_args {
    //         let mut v = DebugAnnotationVisitor { infos: Vec::new() };
    //         event.record(&mut v);
    //         if !v.infos.is_empty() {
    //             Some(Arc::new(v.infos))
    //         } else {
    //             None
    //         }
    //     } else {
    //         None
    //     };

    //     let msg = Message::Event(self.get_timestamp(), name, arg_info, thread_id);
    //     self.send_message(msg);
    // }
}
