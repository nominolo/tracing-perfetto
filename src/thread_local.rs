use std::{collections::HashMap, sync::atomic::AtomicU32};


pub struct ThreadState {
    messages: Vec<Msg>,

    // Interning, append only
    // shareable if we remember the end offset
    // if full, just wipe it and start new one
    // we don't share the hashmap
    interned: Arc<InternState>,
    strings: HashMap<&str, u32>,
}

pub struct InternState {
    // We cannot resize the vec after creation!
    string_data: Vec<u8>,
    string_ptr: AtomicU32,
}

struct Msg {
    timestamp: u64,
    name: u32,
}