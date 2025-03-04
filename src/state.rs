use std::{
    collections::{HashMap, HashSet},
    io::Write,
    usize,
};

use crate::{failures::Reasons, hostsfile::PeerList};

pub mod messaging {
    use std::collections::HashSet;

    use serde::{Deserialize, Serialize};
    #[derive(Serialize, Deserialize, Debug, Copy, Clone)]
    pub enum Operation {
        Add,
    }

    #[derive(Serialize, Deserialize, Debug, Copy, Clone)]
    pub struct LeaderInstruction {
        pub request_id: u32,
        pub peer_id: usize,
        pub view_id: u32,
        pub op: Operation,
    }

    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub enum Message {
        REQ(LeaderInstruction),
        JOIN,
        OK {
            request_id: u32,
            view_id: u32,
        },
        NEWVIEW {
            view_id: u32,
            members: HashSet<usize>,
        },
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
}

use messaging::{LeaderInstruction, Letter, Message, Operation};
pub const LEADER_ID: usize = 1;

type Channels<W> = HashMap<usize, W>;

#[derive(Default)]
// stuff only a true leader would need! 👑
struct Leading {
    requests_count: u32,
    // K: request_id
    // V: (peer id to add, confirmed Oks)
    pending_requests: HashMap<u32, (usize, HashSet<usize>)>,
}

struct Following {
    leader_id: usize,
    // view_ids to match to reqs
    acks: Vec<u32>,
}
impl Default for Following {
    fn default() -> Self {
        Self {
            leader_id: 1,
            acks: Vec::new(),
        }
    }
}

enum Role {
    Leader(Leading),
    Follower(Following),
}

// main state of each process
pub struct Data {
    role: Role,
    // membership list recorded across all peers
    memberships: HashMap<u32, HashSet<usize>>,
    // log of operations to perform
    log: Vec<LeaderInstruction>,
    view_id: u32,
    peer_list: PeerList,
}

impl Data {
    pub fn new(peer_list: PeerList) -> Self {
        let leader = if peer_list.is_leader() {
            Role::Leader(Leading::default())
        } else {
            Role::Follower(Following::default())
        };
        Self {
            view_id: 1,
            memberships: HashMap::from([(1, HashSet::from([LEADER_ID]))]),
            log: Vec::new(),
            peer_list,
            role: leader,
        }
    }

    pub fn is_leader(&self) -> bool {
        matches!(self.role, Role::Leader(_))
    }

    /// receives a message from
    pub fn recv_message(&mut self, letter: &Letter) {
        use messaging::Message as M;
        if let Role::Leader(ref mut lead) = self.role {
            match letter.message() {
                M::JOIN => {
                    lead.requests_count += 1;
                    let peer_id = letter.from_whom();
                    println!("Got a join from {peer_id}");
                    lead.pending_requests
                        .insert(lead.requests_count, (peer_id, HashSet::from([1])));
                    self.log.push(LeaderInstruction {
                        request_id: lead.requests_count,
                        view_id: self.view_id,
                        op: Operation::Add,
                        peer_id,
                    });
                }
                M::OK {
                    request_id,
                    view_id,
                } => {
                    println!("Got an OK from {}", letter.from_whom());
                    let pending_request = lead
                        .pending_requests
                        .get_mut(request_id)
                        .expect("Should receive OK after sending a REQ");
                    pending_request.1.insert(letter.from_whom());
                    // get the members sent at the same time that the vid was sent
                    let members_at_vid = self.memberships.get(view_id).unwrap();
                    if pending_request.1 == *members_at_vid {
                        self.view_id += 1;
                        let mut new_members = members_at_vid.clone();
                        new_members.insert(pending_request.0);
                        self.memberships.insert(self.view_id, new_members);
                    }
                }
                _ => unreachable!(),
            }
        } else if let Role::Follower(ref mut follow) = self.role {
            match letter.message() {
                M::REQ(instruction) => {
                    println!("Got a req: {:?}", instruction);
                    // only really need to save the log
                    self.log.push(*instruction);
                    follow.acks.push(instruction.request_id);
                }
                M::NEWVIEW { view_id, members } => {
                    self.view_id = *view_id;
                    eprintln!(
                        "{{proc_id: {}, view_id: {}, leader: {}, memb_list: {:?}}}",
                        self.peer_list.id(),
                        self.view_id,
                        follow.leader_id,
                        members
                    );
                    self.memberships.insert(self.view_id, members.clone());
                }
                _ => unreachable!(),
            }
        }
    }

    fn send_message(&self, l: Letter, sender: &mut impl Write) -> Result<(), Reasons> {
        let encoded_buffer = bincode::serialize(&l).map_err(|_| Reasons::BadMessage)?;
        let _ = sender.write(&encoded_buffer).map_err(Reasons::IO)?;
        Ok(())
    }

    // member methods
    pub fn ask_to_join(&self, sender: &mut impl Write) -> Result<(), Reasons> {
        assert!(!self.is_leader());
        let parcel: Letter = (self.peer_list.id(), Message::JOIN).into();
        self.send_message(parcel, sender)?;

        Ok(())
    }

    pub fn send_ok(
        &self,
        outgoing_channels: &mut Channels<impl Write>,
        view_id: u32,
        request_id: u32,
    ) -> Result<(), Reasons> {
        println!("sent an OK");
        if let Role::Follower(ref follow) = self.role {
            let parcel: Letter = (
                self.peer_list.id(),
                Message::OK {
                    request_id,
                    view_id,
                },
            )
                .into();
            self.send_message(
                parcel,
                outgoing_channels
                    .get_mut(&follow.leader_id)
                    .expect("Channel for the leader should exist"),
            )?;
        }

        Ok(())
    }

    // Leader methods
    fn all_members_ok(&self, instruction: &LeaderInstruction) -> bool {
        if let Role::Leader(ref lead) = self.role {
            let confirmations = &lead
                .pending_requests
                .get(&instruction.request_id)
                .unwrap()
                .1;
            let members_at_vid = self.memberships.get(&instruction.view_id).unwrap();
            members_at_vid == confirmations
        } else {
            false
        }
    }

    pub fn flush_instructions(
        &mut self,
        outgoing_channels: &mut Channels<impl Write>,
    ) -> Result<(), Reasons> {
        let mut log_index = 0;
        while log_index < self.log.len() {
            let instruction = &self.log[log_index];
            if self.all_members_ok(instruction) {
                self.view_id += 1;
                let prev_view_id = instruction.view_id;
                let mut new_members = self.memberships.get(&prev_view_id).unwrap().clone();
                new_members.insert(instruction.peer_id);

                self.memberships.insert(self.view_id, new_members);
                self.send_newview(outgoing_channels)?;
                self.log.remove(log_index);
                continue;
            }
            log_index += 1;
        }

        Ok(())
    }

    pub fn req_to_members(
        &self,
        outgoing_channels: &mut Channels<impl Write>,
    ) -> Result<(), Reasons> {
        assert!(self.is_leader());
        let current_members = self.memberships.get(&self.view_id).unwrap();

        let msg = Message::REQ(self.log[self.log.len() - 1]);
        let parcel: Letter = (self.peer_list.id(), msg).into();
        for (id, channel) in outgoing_channels
            .iter_mut()
            .filter(|(id, _)| current_members.contains(*id))
        {
            println!("sent a req\n\tto {}", id);
            self.send_message(parcel.clone(), channel)?;
        }
        Ok(())
    }

    pub fn send_newview(
        &self,
        outgoing_channels: &mut Channels<impl Write>,
    ) -> Result<(), Reasons> {
        assert!(self.is_leader());
        let current_members = self.memberships.get(&self.view_id).unwrap();
        let parcel: Letter = (
            self.peer_list.id(),
            Message::NEWVIEW {
                view_id: self.view_id,
                members: current_members.clone(),
            },
        )
            .into();

        for (id, channel) in outgoing_channels
            .iter_mut()
            .filter(|(id, _)| current_members.contains(*id))
        {
            println!("It's time for {} to see the world a different way..", id);
            self.send_message(parcel.clone(), channel)?;
        }
        println!("Sent a newview");
        Ok(())
    }
}
