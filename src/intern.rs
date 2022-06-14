use std::collections::HashMap;

pub struct Interned {
    event_names: HashMap<String, u64>,
    next_name_iid: u64,
}

impl Interned {
    pub fn new() -> Self {
        Interned {
            event_names: HashMap::new(),
            next_name_iid: 1,
        }
    }

    pub fn event_name(&mut self, name: &str) -> (u64, bool) {
        match self.event_names.get(name) {
            Some(iid) => (*iid, false),
            None => {
                self.event_names
                    .insert(name.to_string(), self.next_name_iid);
                let iid = self.next_name_iid;
                self.next_name_iid += 1;
                (iid, true)
            }
        }
    }
}

impl Default for Interned {
    fn default() -> Self {
        Self::new()
    }
}
