use std::{
    cell::RefCell,
    sync::{
        atomic::{AtomicPtr, AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use crossbeam_channel::{Receiver, Sender};
use crossbeam_queue::ArrayQueue;
use crossbeam_utils::atomic::{AtomicCell, AtomicConsume};
use dashmap::DashMap;

pub struct GlobalShared {
    recv: Receiver<Message>,
    out: Arc<Mutex<Sender<Message>>>,
    max_thread_id: AtomicU64,
    new_senders: AtomicU64,
}

pub enum Message {
    Enter {
        ts: u64,
        tid: u64,
        file: Option<&'static str>,
        ctx: Option<u64>,
    },
}

thread_local! {
    static THREAD_ID: RefCell<Option<u64>> = RefCell::new(None);
    static SENDER: AtomicCell<Option<Arc<Sender<Message>>>> = AtomicCell::new(None);
}

impl GlobalShared {
    pub fn new() -> Self {
        let (send, recv) = crossbeam_channel::unbounded();

        let out = Arc::new(Mutex::new(send));

        GlobalShared {
            recv,
            out,
            new_senders: AtomicU64::new(0),
            max_thread_id: AtomicU64::new(0),
        }
    }

    /// Returns `(thread_id, is_new)`
    pub fn get_thread_id(&self) -> (u64, bool) {
        THREAD_ID.with(|value_ref| {
            let thread_id = *value_ref.borrow();
            match thread_id {
                Some(thread_id) => (thread_id, false),
                None => {
                    let thread_id = self.max_thread_id.fetch_add(1, Ordering::SeqCst);
                    value_ref.replace(Some(thread_id));
                    (thread_id, true)
                }
            }
        })
    }

    pub fn send_message(&self, message: Message) {
        SENDER.with(move |acell: &AtomicCell<_>| {
            let ptr: Option<Arc<Sender<Message>>> = acell.swap(None);
            match ptr {
                Some(sender) => {
                    let _ignore_err = sender.send(message);
                    acell.store(Some(sender));
                }
                None => {
                    let sender = Arc::new(self.out.lock().unwrap().clone());
                    self.new_senders.fetch_add(1, Ordering::Relaxed);
                    let _ignore_err = sender.send(message);
                    acell.store(Some(sender));
                }
            }
        })
        // SENDER.with(move |val| {
        //     // TODO: Is there a way to make this more efficient?
        //     if val.borrow().is_some() {
        //         let _ignore_err = val.borrow().as_ref().unwrap().send(message);
        //     } else {
        //         let sender = self.out.lock().unwrap().clone();
        //         let _ignore_err = sender.send(message);
        //         val.replace(Some(sender));
        //     }
        // })
    }

    pub fn stats(&self) {
        println!("new senders: {:?}", self.new_senders.load_consume());
        println!(
            "is lock free: {}",
            AtomicCell::<Option<Arc<Sender<Message>>>>::is_lock_free()
        );
    }
}

#[test]
fn send_twice() {
    let buf = GlobalShared::new();

    buf.send_message(Message::Enter {
        ts: 42,
        tid: 0,
        file: None,
        ctx: None,
    });
    buf.send_message(Message::Enter {
        ts: 42,
        tid: 0,
        file: None,
        ctx: None,
    });
    buf.send_message(Message::Enter {
        ts: 42,
        tid: 0,
        file: None,
        ctx: None,
    });
    buf.stats()
}

pub struct LocalQueue {
    pending_bufs: DashMap<u64, Arc<(u64, ArrayQueue<Message>)>>,
    finished_bufs: Sender<Arc<(u64, ArrayQueue<Message>)>>,
    max_thread_id: AtomicU64,
    buf_size: usize,
}

thread_local! {
    static MY_QUEUE: AtomicCell<Option<Arc<(u64, ArrayQueue<Message>)>>> = AtomicCell::new(None);
    static THREAD_ID2: RefCell<Option<u64>> = RefCell::new(None);
}

impl LocalQueue {
    pub fn new(buf_size: usize) -> Self {
        let pending_bufs = DashMap::new();
        assert!(buf_size > 0);
        let (tx, rv) = crossbeam_channel::unbounded();

        LocalQueue {
            pending_bufs,
            finished_bufs: tx,
            max_thread_id: AtomicU64::new(0),
            buf_size,
        }
    }

    /// Returns `(thread_id, is_new)`
    pub fn get_thread_id(&self) -> (u64, bool) {
        THREAD_ID.with(|value_ref| {
            let thread_id = *value_ref.borrow();
            match thread_id {
                Some(thread_id) => (thread_id, false),
                None => {
                    let thread_id = self.max_thread_id.fetch_add(1, Ordering::SeqCst);
                    value_ref.replace(Some(thread_id));
                    (thread_id, true)
                }
            }
        })
    }

    fn finish_queue(
        &self,
        queue: Arc<(u64, ArrayQueue<Message>)>,
    ) -> Arc<(u64, ArrayQueue<Message>)> {
        let tid = queue.0;
        let _ = self.pending_bufs.remove(&tid).unwrap();
        let _ = self.finished_bufs.send(queue);
        println!("finish {tid}");

        let new_queue = ArrayQueue::new(self.buf_size);
        let r = Arc::new((tid, new_queue));
        self.pending_bufs.insert(tid, r.clone());
        r
    }

    fn fresh_buffer(&self) -> Arc<(u64, ArrayQueue<Message>)> {
        let (tid, _is_new) = self.get_thread_id();
        println!("fresh {tid}");
        // TODO: Handle `_is_new`
        let new_queue = ArrayQueue::new(self.buf_size);
        let r = Arc::new((tid, new_queue));
        self.pending_bufs.insert(tid, r.clone());
        r
    }

    pub fn send_message(&self, message: Message) {
        MY_QUEUE.with(move |acell: &AtomicCell<_>| {
            let ptr: Option<Arc<(u64, ArrayQueue<Message>)>> = acell.swap(None);
            match ptr {
                Some(r) => match r.1.push(message) {
                    Ok(_) => acell.store(Some(r)),
                    Err(message) => {
                        let new_r = self.finish_queue(r);
                        if let Err(_) = new_r.1.push(message) {
                            panic!("Failed to push into fresh buffer");
                        }
                        acell.store(Some(new_r))
                    }
                },
                None => {
                    let new_r = self.fresh_buffer();
                    if let Err(_) = new_r.1.push(message) {
                        panic!("Failed to push into fresh buffer");
                    }
                    acell.store(Some(new_r))
                }
            }
        })
    }
}

#[test]
fn send_many_local() {
    let buf = LocalQueue::new(10);

    for _ in 0..50 {
        buf.send_message(Message::Enter {
            ts: 42,
            tid: 0,
            file: None,
            ctx: None,
        });
    }
}
