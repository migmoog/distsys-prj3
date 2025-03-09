use std::collections::{HashMap, HashSet};

use crate::{Instruction, Operation};

use super::{PeerId, RequestId, ViewId, DEFAULT_LEADER_ID};

// stuff only a true leader would need! 👑
#[derive(Default, Debug)]
pub struct Leading {
    requests_count: RequestId,
    waiting_for: Option<RequestId>,
    // K: request_id
    // V: (peer id to add, confirmed Oks)
    pending_requests: HashMap<RequestId, (PeerId, ViewId, HashSet<PeerId>)>,
}
impl Leading {
    pub fn latest_request(&self) -> RequestId {
        self.requests_count
    }

    // increments the request_id and creates a list awaiting a new set of confirmations.
    // Starts in a state without ANY. Including the leader.
    pub fn push_request(&mut self, peer_id: PeerId, view_id: ViewId) {
        self.requests_count += 1;
        self.pending_requests
            .insert(self.requests_count, (peer_id, view_id, HashSet::new()));
    }

    // adds the peer_id to the confirmations in the members list.
    pub fn acknowledge_ok(&mut self, request_id: RequestId, peer_id: PeerId) {
        self.pending_requests
            .entry(request_id)
            .and_modify(|(_, _, confirmations)| {
                confirmations.insert(peer_id);
            });
    }

    /// Check if the leader is not waiting on confirmations from another request
    pub fn can_proceed(&self) -> bool {
        self.waiting_for.is_none() && self.pending_requests.len() > 0
    }

    pub fn start_req(&mut self) -> Instruction {
        let request_id = *self.pending_requests.keys().min().unwrap();
        self.waiting_for = Some(request_id);
        let req = self.pending_requests.get(&request_id).unwrap();
        Instruction {
            request_id,
            peer_id: req.0,
            view_id: req.1,
            op: Operation::Add,
        }
    }

    pub fn check_req_complete(
        &mut self,
        memberships: &HashMap<ViewId, HashSet<PeerId>>,
    ) -> Option<Instruction> {
        if let Some(request_id) = self.waiting_for {
            let req = self.pending_requests.get(&request_id).unwrap();
            let members = memberships
                .get(&req.1)
                .expect("View should exist in memberships");

            if req.2 == *members {
                self.waiting_for = None;
                let val = self.pending_requests.remove(&request_id);
                //println!("REQ_COMPLETE: {:?}", val);
                val.map(|v| Instruction {
                    request_id,
                    peer_id: v.0,
                    view_id: v.1,
                    op: Operation::Add,
                })
            } else {
                None
            }
        } else {
            None
        }
    }
}

pub struct Following {
    leader_id: PeerId,
    ack_queue: HashMap<RequestId, Instruction>,
}
impl Following {
    pub fn leader_id(&self) -> PeerId {
        self.leader_id
    }

    pub fn push_instruction(&mut self, instr: Instruction) {
        self.ack_queue.insert(instr.request_id, instr);
    }

    // finds the earliest request and removes it from the queue
    pub fn send_ok(&mut self) -> Option<Instruction> {
        if let Some(&lowest_req) = self.ack_queue.keys().min() {
            self.ack_queue.remove(&lowest_req)
        } else {
            None
        }
    }
}

pub enum Role {
    Leader(Leading),
    Follower(Following),
}
impl Role {
    pub fn new(is_leader: bool) -> Self {
        if is_leader {
            Self::Leader(Leading::default())
        } else {
            Self::Follower(Following {
                leader_id: DEFAULT_LEADER_ID,
                ack_queue: HashMap::new(),
            })
        }
    }
}
