use std::{
    collections::{HashMap, HashSet},
    io::Write,
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
        pub fn from_who(&self) -> usize {
            self.0
        }
        pub fn message(&self) -> &Message {
            &self.1
        }
    }
}

use messaging::{LeaderInstruction, Letter, Message, Operation};
pub const LEADER_ID: usize = 1;

type Channels<W: Write> = HashMap<usize, W>;

// main state of each process
pub struct Data {
    // membership list recorded across all peers
    memberships: HashMap<u32, HashSet<usize>>,
    // proc id's of processes that sent OK
    confirmations: HashMap<u32, HashSet<usize>>,
    // log of operations to perform
    log: Vec<LeaderInstruction>,
    current_view_id: u32,
    current_request_id: u32,
    peer_list: PeerList,
}

impl Data {
    pub fn new(peer_list: PeerList) -> Self {
        Self {
            current_view_id: 1,
            current_request_id: 0,
            memberships: HashMap::from([(1, HashSet::from([LEADER_ID]))]),
            confirmations: HashMap::new(),
            log: Vec::new(),
            peer_list,
        }
    }

    pub fn is_leader(&self) -> bool {
        self.peer_list.is_leader()
    }

    /// receives a message from
    pub fn recv_message(&mut self, l: &Letter) {
        use messaging::Message as M;
        let id = self.peer_list.id();
        match l.message() {
            M::JOIN => {
                assert!(self.is_leader());
                self.current_request_id += 1;
                self.confirmations
                    .insert(self.current_request_id, HashSet::new());
                self.log.push(LeaderInstruction {
                    request_id: self.current_request_id,
                    peer_id: l.from_who(),
                    view_id: self.current_view_id,
                    op: Operation::Add,
                });
            }

            M::REQ(instruction) => {
                assert!(!self.is_leader());
                self.log.push(*instruction);
            }

            M::OK {
                request_id,
                view_id,
            } => {
                assert!(self.is_leader());
                assert_eq!(*view_id, self.current_view_id);
                let oks = self.confirmations.get_mut(request_id).unwrap();
                oks.insert(l.from_who());

                // means we've gotten all of our confirmations
                if oks.len() == self.peer_list.len() {}
            }

            M::NEWVIEW { view_id, members } => {
                assert!(!self.is_leader());
                self.current_view_id = *view_id;
                self.memberships
                    .insert(self.current_view_id, members.clone());
                eprintln!(
                    "{{proc_id: {}, view_id: {}, leader: {}, memb_list: {:?}}}",
                    id, self.current_view_id, 1, members
                );
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

    pub fn send_ok(&self, sender: &mut impl Write) -> Result<(), Reasons> {
        assert!(!self.is_leader());
        let parcel: Letter = (
            self.peer_list.id(),
            Message::OK {
                request_id: self.current_view_id,
                view_id: self.current_view_id,
            },
        )
            .into();
        self.send_message(parcel, sender)?;

        Ok(())
    }

    // Leader methods
    pub fn all_members_ok(&self) -> bool {
        self.confirmations
            .get(&self.current_request_id)
            .unwrap()
            .len()
            == self.peer_list.len()
    }

    pub fn req_to_members(
        &self,
        outgoing_channels: &mut Channels<impl Write>,
    ) -> Result<(), Reasons> {
        assert!(self.is_leader());
        let current_members = self.memberships.get(&self.current_view_id).unwrap();

        let msg = Message::REQ(self.log[self.log.len() - 1]);
        let parcel: Letter = (self.peer_list.id(), msg).into();
        for (_, channel) in outgoing_channels
            .iter_mut()
            .filter(|(id, _)| current_members.contains(*id))
        {
            self.send_message(parcel.clone(), channel)?;
        }
        Ok(())
    }

    pub fn send_newview(
        &self,
        outgoing_channels: &mut Channels<impl Write>,
    ) -> Result<(), Reasons> {
        assert!(self.is_leader());
        let current_members = self.memberships.get(&self.current_view_id).unwrap();
        let parcel: Letter = (
            self.peer_list.id(),
            Message::NEWVIEW {
                view_id: self.current_view_id,
                members: current_members.clone(),
            },
        )
            .into();

        for (_, channel) in outgoing_channels
            .iter_mut()
            .filter(|(id, _)| current_members.contains(*id))
        {
            self.send_message(parcel.clone(), channel)?;
        }
        Ok(())
    }
}
