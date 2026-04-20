use crate::protocol::response::WireMessage;
use std::sync::mpsc::{Sender};
use std::sync::{Arc, Mutex};

type MatchFn = Box<dyn Fn(&WireMessage) -> bool + Send + Sync>;


struct Waiter {
    /// Function to determine if a message matches the waiter's criteria.
    matches: MatchFn,
    /// Transmitter to send matched messages to the waiter.
    tx: Sender<WireMessage>,
    /// How many messages are still needed before the waiter is satisfied:
    remaining: u32,
}

#[derive(Clone)]
pub struct Dispatcher {
    /// The transmitter for the main data stream. Used to forward messages not claimed by waiters.
    stream_tx: Option<Sender<WireMessage>>,
    /// The list of waiters listening for specific messages.
    waiters: Arc<Mutex<Vec<Waiter>>>,
}

impl Dispatcher {
    pub fn new() -> Self {
        Dispatcher {
            stream_tx: None,
            waiters: Arc::new(Mutex::new(Vec::new())),
        }
    }
    
    pub fn set_stream_tx(&mut self, tx: Sender<WireMessage>) {
        self.stream_tx = Some(tx);
    }
    
    /// Registers a new waiter with a matching function and a transmitter.
    pub fn register_waiter(&self, matches: MatchFn, tx: Sender<WireMessage>, count: u32) {
        let mut waiters = self.waiters.lock().unwrap();
        waiters.push(Waiter { matches, tx, remaining: count }); 
    }
    
    pub fn dispatch(&self, msg: WireMessage) {
        // Try to satisfy waiters first:
        let mut waiters = self.waiters.lock().unwrap();
        let mut i = 0;
        
        // println!("Waiters count: {}", waiters.len());
        while i < waiters.len() {
            if (waiters[i].matches)(&msg) {
                // Send the message to the waiter:
                // println!("A waiter claimed the message {:?}, sending to waiter.", &msg);
                let _ = waiters[i].tx.send(msg.clone());
                // Decrement remaining count
                waiters[i].remaining -= 1;
                
                // Remove if done
                if waiters[i].remaining == 0 {
                    waiters.remove(i);
                }
                drop(waiters); // Release lock
                return;
            } else {
                i += 1;
            }
        }
        drop(waiters); // Release lock
        // No waiter claimed the message, send to stream:
        // println!("No waiter claimed the message, sending to stream.");
        self.fanout_stream(msg);
    }
    
    fn fanout_stream(&self, msg: WireMessage) {
        if let Some(ref tx) = self.stream_tx {
            // println!("Sending message to stream.");
            let _ = tx.send(msg);
        }
    }
}

