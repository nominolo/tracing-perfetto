use tracing::instrument;

#[instrument]
fn fibonacci(n: usize) -> usize {
    if n < 2 {
        std::thread::sleep_ms(30);
        n
    } else {
        fibonacci(n - 1) + fibonacci(n - 2)
    }
}

fn main() {
    use tracing_subscriber::prelude::*;

    let (perfetto_layer, _guard) = tracing_perfetto::PerfettoLayerBuilder::new()
        .file("fibonacci.perfetto-trace")
        .include_args(true)
        .build();
    tracing_subscriber::registry().with(perfetto_layer).init();

    // let (chrome_layer, _guard) = tracing_chrome::ChromeLayerBuilder::new()
    //     .file("trace-fibonacci.json")
    //     .include_locations(false)
    //     .include_args(true)
    //     .build();
    // tracing_subscriber::registry().with(chrome_layer).init();

    let j = std::thread::Builder::new()
        .name("myworker".to_string())
        .spawn(move || {
            fibonacci(6);
        })
        .unwrap();

    let start = cpu_time::ProcessTime::now();

    tracing::debug!("cpu time: {}", 0);
    fibonacci(5);
    tracing::debug!("cpu time: {:?}", start.elapsed());

    j.join().unwrap();
    tracing::debug!("cpu time: {:?}", start.elapsed());
}
