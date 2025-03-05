use std::collections::{HashMap, HashSet};

use crate::{Instruction, Operation};

use super::LEADER_ID;
pub type PeerId = usize;
pub type ViewId = u32;
pub type RequestId = u32;

// stuff only a true leader would need! 👑
pub struct Leading {
    requests_count: RequestId,
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

    // converts a pending request to an Instruction
    pub fn instr_from_req(&self, request_id: RequestId) -> Instruction {
        let req = self.pending_requests.get(&request_id).unwrap();
        Instruction {
            request_id,
            peer_id: req.0,
            view_id: req.1,
            op: Operation::Add,
        }
    }

    // finds satisfied requests (based on membership lists at the time of request)
    // and returns a vec of peer_ids to be added
    pub fn evaluate_requests(
        &mut self,
        memberships: &HashMap<ViewId, HashSet<PeerId>>,
    ) -> Vec<PeerId> {
        let mut reqs_to_delete: Vec<RequestId> = Vec::new();
        let mut out = Vec::new();
        for (req_id, (peer_id, view_id, confirmations)) in self.pending_requests.iter() {
            if confirmations == memberships.get(&view_id).unwrap() {
                reqs_to_delete.push(*req_id);
                out.push(*peer_id);
            }
        }

        // clean up satisfied requests
        for req_id in reqs_to_delete {
            self.pending_requests.remove(&req_id);
        }
        out
    }
}

pub struct Following {
    leader_id: PeerId,
}
impl Following {
    pub fn leader_id(&self) -> PeerId {
        self.leader_id
    }
}

pub enum Role {
    Leader(Leading),
    Follower(Following),
}
impl Role {
    pub fn new(is_leader: bool) -> Self {
        if is_leader {
            Self::Leader(Leading {
                requests_count: 0,
                pending_requests: HashMap::new(),
            })
        } else {
            Self::Follower(Following {
                leader_id: LEADER_ID,
            })
        }
    }
}
