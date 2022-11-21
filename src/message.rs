use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use crossbeam_channel::{Receiver, Sender};
use crossbeam_queue::ArrayQueue;
use crossbeam_utils::atomic::AtomicCell;
use dashmap::DashMap;

use crate::{annotations::FieldValue, packet::DebugAnnotation};

pub type ThreadId = u64;
pub type Timestamp = u64;

#[derive(Debug)]
pub enum Message {
    NewThread(ThreadId, Box<String>),
    Enter {
        ts: Timestamp,
        label: &'static str,
        args: Option<Arc<Vec<FieldValue>>>,
    },
    Exit {
        ts: Timestamp,
        label: &'static str,
        args: Option<Arc<Vec<FieldValue>>>,
    },
    Event {
        ts: Timestamp,
        label: &'static str,
        args: Option<Arc<Vec<FieldValue>>>,
    },
}

pub struct Buffer {
    pub(crate) thread_id: ThreadId,
    pub(crate) queue: ArrayQueue<Message>,
}

pub enum FinishedBuf {
    Buf(Arc<Buffer>),
    StopWriterThread,
}

pub struct MessagingState {
    buffer_size: usize,
    max_thread_id: AtomicU64,
    in_use_local_buffers: Arc<DashMap<ThreadId, Arc<Buffer>>>,

    finished_buffers: Sender<FinishedBuf>,
}

thread_local! {
    static THREAD_QUEUE: AtomicCell<Option<Arc<Buffer>>> = AtomicCell::new(None);
    // static THREAD_ID: RefCell<Option<u64>> = RefCell::new(None);
}

impl MessagingState {
    pub fn new(
        buffer_size: usize,
    ) -> (
        Self,
        Receiver<FinishedBuf>,
        Sender<FinishedBuf>,
        Arc<DashMap<ThreadId, Arc<Buffer>>>,
    ) {
        assert!(buffer_size >= 2);
        let (send, recv) = crossbeam_channel::unbounded();
        let in_use = Arc::new(DashMap::default());
        (
            MessagingState {
                buffer_size,
                max_thread_id: AtomicU64::new(0),
                in_use_local_buffers: in_use.clone(),
                finished_buffers: send.clone(),
            },
            recv,
            send,
            in_use,
        )
    }

    /// Send a message to the thread-local queue.
    ///
    /// If the buffer is full we'll send it to the main writer thread and start
    /// a new buffer.
    pub fn send_message(&self, message: Message) {
        //println!("Sending: {message:?}");
        THREAD_QUEUE.with(move |acell: &AtomicCell<_>| {
            // Take the queue out of the cell, so we can own it (we'll put it
            // back later)
            let opt_buffer: Option<Arc<Buffer>> = acell.swap(None);
            match opt_buffer {
                Some(buffer) => {
                    // Queue has been initialized, try to send the message
                    match buffer.queue.push(message) {
                        Ok(_) => {
                            // all good, we successfully went down the fast path
                            // now put the queue back into the cell
                            acell.store(Some(buffer))
                        }
                        Err(message) => {
                            // Queue is full.
                            let new_buffer = self.handle_full_queue(buffer);
                            if let Err(_) = new_buffer.queue.push(message) {
                                panic!("Failed to push into fresh buffer")
                            }
                            acell.store(Some(new_buffer))
                        }
                    }
                }
                None => {
                    // If this is our first message on this thread we need to
                    // assign a new thread Id
                    let first_buffer = self.alloc_first_buffer();
                    if let Err(_) = first_buffer.queue.push(message) {
                        panic!("Failed to push into fresh buffer")
                    }
                    acell.store(Some(first_buffer))
                }
            }
        })
    }

    fn handle_full_queue(&self, buffer: Arc<Buffer>) -> Arc<Buffer> {
        let thread_id = buffer.thread_id;
        let _ = self.in_use_local_buffers.remove(&thread_id);
        let _ = self.finished_buffers.send(FinishedBuf::Buf(buffer));

        let new_buf = self.alloc_empty_buffer(thread_id);
        self.in_use_local_buffers.insert(thread_id, new_buf.clone());
        new_buf
    }

    fn alloc_first_buffer(&self) -> Arc<Buffer> {
        // First, assign a new thread ID and get an empty buffer
        let thread_id = self.max_thread_id.fetch_add(1, Ordering::SeqCst);
        let buf = self.alloc_empty_buffer(thread_id);

        // Then, the first message should introduce the thread name
        let thread_name = if let Some(name) = std::thread::current().name() {
            format!("{name} {thread_id}")
        } else {
            format!("thread {thread_id}")
        };
        let _ = buf
            .queue
            .push(Message::NewThread(thread_id, Box::new(thread_name)));

        self.in_use_local_buffers.insert(thread_id, buf.clone());

        buf
    }

    fn alloc_empty_buffer(&self, thread_id: ThreadId) -> Arc<Buffer> {
        // TODO: Use a pool of reusable buffers. Could be another channel where
        // the writer thread sends the buffers that it finish processing.
        Arc::new(Buffer {
            thread_id,
            queue: ArrayQueue::new(self.buffer_size),
        })
    }
}
