use std::{collections::HashMap, hash::Hash};

pub struct Interned {
    event_names: InternMap,
    debug_annotation_names: InternMap,
}

struct InternMap {
    names: HashMap<String, u64>,
    next_iid: u64,
}

impl InternMap {
    fn new() -> Self {
        InternMap {
            names: HashMap::default(),
            next_iid: 1,
        }
    }

    fn get_iid(&mut self, name: &str) -> (u64, bool) {
        match self.names.get(name) {
            Some(iid) => (*iid, false),
            None => {
                self.names.insert(name.to_string(), self.next_iid);
                let iid = self.next_iid;
                self.next_iid += 1;
                (iid, true)
            }
        }
    }

    fn reset(&mut self) {
        self.names.clear();
        self.next_iid = 1;
    }
}

impl Interned {
    pub fn new() -> Self {
        Interned {
            event_names: InternMap::new(),
            debug_annotation_names: InternMap::new(),
        }
    }

    pub fn event_name(&mut self, name: &str) -> (u64, bool) {
        self.event_names.get_iid(name)
    }

    pub fn debug_annotation_name(&mut self, name: &str) -> (u64, bool) {
        self.debug_annotation_names.get_iid(name)
    }

    pub fn reset(&mut self) {
        self.event_names.reset();
        self.debug_annotation_names.reset();
    }
}

impl Default for Interned {
    fn default() -> Self {
        Self::new()
    }
}
