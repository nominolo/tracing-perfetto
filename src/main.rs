use std::{
    thread::{self, ScopedJoinHandle},
    time::{Duration, Instant},
};

use tracing_perfetto::buffer::*;

fn main() {
    let threads = 20_usize;
    let rounds = 1_000_000;

    bench_shared(threads, rounds);
    bench_batched(threads, rounds);
}

fn bench_shared(threads: usize, rounds: usize) {
    let buf = GlobalShared::new();

    //let count = thread::available_parallelism().unwrap().get();
    //println!("count: {count:?}");

    thread::scope(|s| {
        let _handles: Vec<ScopedJoinHandle<(usize, Duration)>> = (0..threads)
            .into_iter()
            .map(|id| {
                let buf = &buf;
                s.spawn(move || {
                    let t = Instant::now();
                    for _ in 0..rounds {
                        buf.send_message(Message::Enter {
                            ts: 42,
                            tid: 89,
                            file: None,
                            ctx: None,
                        })
                    }
                    (id, t.elapsed())
                })
            })
            .collect::<Vec<_>>();
        for h in _handles {
            let r = h.join().unwrap();
            println!("{:?}, {}", r, rounds as f64 / r.1.as_secs_f64());
        }
    });
}

fn bench_batched(threads: usize, rounds: usize) {
    let buf = LocalQueue::new(1024);

    // let threads = 20_usize;
    // let rounds = 1_000_000;

    //let count = thread::available_parallelism().unwrap().get();
    //println!("count: {count:?}");

    thread::scope(|s| {
        let _handles: Vec<ScopedJoinHandle<(usize, Duration)>> = (0..threads)
            .into_iter()
            .map(|id| {
                let buf = &buf;
                s.spawn(move || {
                    let t = Instant::now();
                    for _ in 0..rounds {
                        buf.send_message(Message::Enter {
                            ts: 42,
                            tid: 89,
                            file: None,
                            ctx: None,
                        })
                    }
                    (id, t.elapsed())
                })
            })
            .collect::<Vec<_>>();
        for h in _handles {
            let r = h.join().unwrap();
            println!("{:?}, {}", r, rounds as f64 / r.1.as_secs_f64());
        }
    });
}
