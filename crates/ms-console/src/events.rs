use serde_json::Value;
use std::collections::VecDeque;
use tokio::sync::broadcast;

#[derive(Clone, Debug, PartialEq)]
pub struct ConsoleEvent {
    pub id: u64,
    pub name: String,
    pub data: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Replay {
    pub events: Vec<ConsoleEvent>,
    pub resync: bool,
}

pub struct EventRing {
    capacity: usize,
    events: VecDeque<ConsoleEvent>,
    next_id: u64,
    sender: broadcast::Sender<ConsoleEvent>,
}

impl EventRing {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity.max(1));
        Self {
            capacity: capacity.max(1),
            events: VecDeque::new(),
            next_id: 1,
            sender,
        }
    }
    pub fn subscribe(&self) -> broadcast::Receiver<ConsoleEvent> {
        self.sender.subscribe()
    }
    pub fn push(&mut self, mut event: ConsoleEvent) -> ConsoleEvent {
        event.id = self.next_id;
        self.next_id += 1;
        self.events.push_back(event.clone());
        while self.events.len() > self.capacity {
            self.events.pop_front();
        }
        let _ = self.sender.send(event.clone());
        event
    }
    pub fn replay(&self, after: Option<u64>) -> Replay {
        let Some(after) = after else {
            return Replay {
                events: Vec::new(),
                resync: false,
            };
        };
        let oldest = self
            .events
            .front()
            .map(|event| event.id)
            .unwrap_or(self.next_id);
        if after.saturating_add(1) < oldest {
            return Replay {
                events: Vec::new(),
                resync: true,
            };
        }
        Replay {
            events: self
                .events
                .iter()
                .filter(|event| event.id > after)
                .cloned()
                .collect(),
            resync: false,
        }
    }

    pub fn latest_id(&self) -> u64 {
        self.next_id.saturating_sub(1)
    }
}
