# tracing-perfetto

## Overview

This is a [tracing-subscriber](https://crates.io/crates/tracing-subscriber) that
outputs traces in [Perfetto]'s [binary format][format]. To view the trace use <https://ui.perfetto.dev/>.

[Perfetto]: https://perfetto.dev/
[format]: https://android.googlesource.com/platform/external/perfetto/+/HEAD/protos/perfetto/trace/perfetto_trace.proto

## Usage

```rust
fn main() {
    use tracing_subscriber::prelude::*;

    let (perfetto_layer, _guard) = tracing_perfetto::PerfettoLayerBuilder::new()
        .file("mytrace.perfetto-trace") // override output file name
        .include_args(true) // include span args and log messages (bigger traces)
        .build();
    tracing_subscriber::registry().with(perfetto_layer).init();

    // start making using spans and #[instrument], etc.

    // ...
}
```
