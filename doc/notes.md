- Interned state is __per `trusted_sequence_id`__
- We could have one per thread.

- Idea: 

# TODOs:

Faster Hashmap:
- use fxhash or similar

Protobuf encoding without copying
- Write nested messages directly into output buffer. If we reserve 4 bytes for
  the message we can support: 2^(7 * 4) = 256MiB, 3 bytes => 2MiB, 2 bytes =>
  16KiB, 1 byte => 127 bytes.

Send less frequent messages to channel
- accumulate in thread-local buffer, buffer just has the unique data we need
  not encoded protobuf
- keep strings in string-local table, ensure writer thread can read from it,
  either by starting a new table after buffer flush and sending ownership to
  writer thread, or by using some append-only structure
