use std::{
    fs::File,
    io::{BufWriter, Write},
    marker::PhantomData,
    time::Instant, thread::JoinHandle,
};

use crossbeam_channel::{Receiver, Sender};
use tracing::{info_span, span, Subscriber};
use tracing_subscriber::{registry::LookupSpan, Layer};

use crate::{
    emit::ProtoEmitter,
    packet::{Emit, PacketData, TracePacket, TrackEvent},
};

mod emit;
mod packet;

pub struct PerfettoLayer<S> {
    sender: crossbeam_channel::Sender<Message>,
    start: Instant,
    _marker: PhantomData<S>,
}

#[derive(Debug)]
pub enum Message {
    Enter(u64, String),
    Exit(u64, String),
    Drop,
}

impl<S> PerfettoLayer<S> {
    pub fn new() -> (Self, (JoinHandle<()>, Sender<Message>)) {
        let (tx, rx) = crossbeam_channel::unbounded();
        let worker = std::thread::spawn(move || writer_thread(rx));

        // TODO: Add Guard with drop impl that tells worker to stop processing
        // and flush to disk.

        let start = Instant::now();

        (PerfettoLayer {
            sender: tx.clone(),
            start,
            _marker: PhantomData,
        }, (worker, tx))
    }

    fn send_message(&self, msg: Message) {
        let _ignore_send_err = self.sender.send(msg);
    }

    fn get_timestamp(&self) -> u64 {
        self.start.elapsed().as_nanos() as u64
    }

    pub fn finish(&self) {
        self.send_message(Message::Drop)
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
    fn on_enter(&self, id: &span::Id, ctx: tracing_subscriber::layer::Context<'_, S>) {
        let span = ctx.span(id);
        let span_name = span.map(|s| s.name());
        // println!("on_enter: id={:?}, span_name={:?}, ", id, span_name);
        let msg = Message::Enter(self.get_timestamp(), span_name.unwrap_or("").to_string());
        self.send_message(msg);
    }

    fn on_exit(&self, id: &span::Id, ctx: tracing_subscriber::layer::Context<'_, S>) {
        let span = ctx.span(id);
        let span_name = span.map(|s| s.name());
        // println!("on_exit: id={:?}, span_name={:?}, ", id, span_name);
        let msg = Message::Exit(self.get_timestamp(), span_name.unwrap_or("").to_string());
        self.send_message(msg);
    }
}

fn writer_thread(rx: Receiver<Message>) {
    let file = File::create("test-trace.perfetto-trace").unwrap();
    let mut writer = BufWriter::new(file);

    let mut em = ProtoEmitter::new();
    let mut buf = ProtoEmitter::new();
    let trusted_uid = 42;
    let trusted_packet_sequence_id = 1;

    for msg in rx {
        em.clear();
        buf.clear();
        println!("msg: {:?}", msg);
        match msg {
            Message::Enter(timestamp, name) => {
                let msg = TracePacket {
                    timestamp,
                    data: PacketData::TrackEvent(TrackEvent {
                        event_type: packet::EventType::SliceBegin,
                        name,
                    }),
                    trusted_uid,
                    trusted_packet_sequence_id,
                    interned_data: None,
                };
                msg.emit(&mut buf);
                em.bytes_field(1, buf.as_bytes());
                writer.write(em.as_bytes()).unwrap();
            }
            Message::Exit(timestamp, name) => {
                let msg = TracePacket {
                    timestamp,
                    data: PacketData::TrackEvent(TrackEvent {
                        event_type: packet::EventType::SliceEnd,
                        name,
                    }),
                    trusted_uid,
                    trusted_packet_sequence_id,
                    interned_data: None,
                };
                msg.emit(&mut buf);
                em.bytes_field(1, buf.as_bytes());
                writer.write(em.as_bytes()).unwrap();
            }
            Message::Drop => break,
        }
        writer.flush().unwrap();
    }
}

#[test]
fn basic() {
    use tracing_subscriber::prelude::*;

    let (perfetto_layer, handle) = PerfettoLayer::new();
    tracing_subscriber::registry().with(perfetto_layer).init();

    let span = info_span!("hello world").entered();
    println!("blah");
    span.exit();
    //handle.join();
}

#[cfg(test)]
mod tests {
    use tracing::instrument;
    use tracing_subscriber::prelude::*;

    use crate::PerfettoLayer;

    #[instrument]
    fn fibonacci(n: usize) -> usize {
        if n < 2 {
            n
        } else {
            fibonacci(n - 1) + fibonacci(n - 2)
        }
    }


#[test]
fn fib() {
    use tracing_subscriber::prelude::*;

    let (perfetto_layer, handle) = PerfettoLayer::new();
    tracing_subscriber::registry().with(perfetto_layer).init();

    //let span = info_span!("hello world").entered();
    fibonacci(5);
    //perfetto_layer.finish();
    //span.exit();
    handle.1.send(crate::Message::Drop).unwrap();
    handle.0.join().unwrap()
}
}
