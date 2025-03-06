use std::collections::HashSet;

use serde::{Deserialize, Serialize};
#[derive(Serialize, Deserialize, Debug, Copy, Clone)]
pub enum Operation {
    Add,
}

#[derive(Serialize, Deserialize, Debug, Copy, Clone)]
pub struct Instruction {
    pub request_id: u32,
    pub peer_id: usize,
    pub view_id: u32,
    pub op: Operation,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Message {
    // Part 1
    REQ(Instruction),
    JOIN,
    OK {
        request_id: u32,
        view_id: u32,
    },
    NEWVIEW {
        view_id: u32,
        members: HashSet<usize>,
    },

    // Part 2
    HEARTBEAT,
}

// Need this because as far as I know there isn't a way to get the from
// id over a TCP connection
// also it's a letter bc it's a message with an address :-)
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Letter(usize, Message);
impl From<(usize, Message)> for Letter {
    fn from(value: (usize, Message)) -> Self {
        Self(value.0, value.1)
    }
}
impl Letter {
    pub fn from_whom(&self) -> usize {
        self.0
    }
    pub fn message(&self) -> &Message {
        &self.1
    }
}
