use std::{
    collections::{HashMap, HashSet},
    io::Write,
    usize,
};

use crate::{failures::Reasons, hostsfile::PeerList};

pub mod messaging;
mod roles;
use messaging::{Instruction, Letter, Message, Operation};
use roles::{PeerId, Role, ViewId};
pub const LEADER_ID: usize = 1;

type Channels<W> = HashMap<usize, W>;
// main state of each process
pub struct Data {
    role: Role,
    // membership list recorded across all peers
    memberships: HashMap<ViewId, HashSet<PeerId>>,
    // log of operations to perform
    view_id: ViewId,
    peer_list: PeerList,
}

impl Data {
    pub fn new(peer_list: PeerList) -> Self {
        let role = Role::new(peer_list.is_leader());
        Self {
            view_id: 1,
            memberships: HashMap::from([(1, HashSet::from([LEADER_ID]))]),
            peer_list,
            role,
        }
    }

    pub fn is_leader(&self) -> bool {
        matches!(self.role, Role::Leader(_))
    }

    /// receives a message from
    pub fn recv_message(&mut self, letter: &Letter) {
        println!("recv: {:?}", letter);

        use messaging::Message as M;
        if let Role::Leader(ref mut lead) = self.role {
            match letter.message() {
                M::JOIN => {
                    lead.push_request(letter.from_whom(), self.view_id);
                    lead.acknowledge_ok(lead.latest_request(), self.peer_list.id());
                }
                M::OK {
                    request_id,
                    view_id,
                } => {
                    lead.acknowledge_ok(*request_id, letter.from_whom());
                }
                _ => unreachable!(),
            }
        } else if let Role::Follower(ref mut follow) = self.role {
            match letter.message() {
                M::REQ(instr) => {
                    follow.push_instruction(*instr);
                }
                M::NEWVIEW { view_id, members } => {
                    self.view_id = *view_id;
                    eprintln!(
                        "{{proc_id: {}, view_id: {}, leader: {}, memb_list: {:?}}}",
                        self.peer_list.id(),
                        self.view_id,
                        follow.leader_id(),
                        members
                    );
                    self.memberships.insert(self.view_id, members.clone());
                }
                _ => unreachable!(),
            }
        }
    }

    fn send_letter(&self, letter: &Letter, sender: &mut impl Write) -> Result<(), Reasons> {
        println!("send: {:?}", letter);

        let encoded_buffer = bincode::serialize(&letter).map_err(|_| Reasons::BadMessage)?;
        let _ = sender.write(&encoded_buffer).map_err(Reasons::IO)?;
        Ok(())
    }

    // member methods
    pub fn ask_to_join(&self, sender: &mut impl Write) -> Result<(), Reasons> {
        assert!(!self.is_leader());
        let parcel: Letter = (self.peer_list.id(), Message::JOIN).into();
        self.send_letter(&parcel, sender)?;

        Ok(())
    }

    // Leader methods
    fn push_new_view(&mut self, to_add: PeerId) {
        let mut prev_members = self
            .memberships
            .get(&self.view_id)
            .expect("Should have a view prior to this one existing.")
            .clone();
        prev_members.insert(to_add);
        self.view_id += 1;
        self.memberships.insert(self.view_id, prev_members);
    }

    pub fn flush_instructions(
        &mut self,
        outgoing_channels: &mut Channels<impl Write>,
    ) -> Result<(), Reasons> {
        if let Role::Leader(ref mut lead) = self.role {
            if let Some(Instruction { peer_id, .. }) = lead.check_req_complete(&self.memberships) {
                self.push_new_view(peer_id);
                self.update_views(outgoing_channels)?;
            }
        } else if let Role::Follower(ref mut follow) = self.role {
            let leader_id = follow.leader_id();
            if let Some(ack_instr) = follow.send_ok() {
                self.send_letter(
                    &(
                        self.peer_list.id(),
                        Message::OK {
                            request_id: ack_instr.request_id,
                            view_id: ack_instr.view_id,
                        },
                    )
                        .into(),
                    outgoing_channels.get_mut(&leader_id).unwrap(),
                )?;
            }
        }
        Ok(())
    }

    pub fn proceed_reqs(
        &mut self,
        outgoing_channels: &mut Channels<impl Write>,
    ) -> Result<(), Reasons> {
        if let Role::Leader(ref mut lead) = self.role {
            // check if lead isnt waiting for any reqs
            // check if we have one ready to send
            // send out reqs
            if lead.can_proceed() {
                let msg = lead.start_req();
                let letter: Letter = (self.peer_list.id(), Message::REQ(msg)).into();

                let current_members = self.memberships.get(&self.view_id).unwrap();
                for (_, channel) in outgoing_channels
                    .iter_mut()
                    .filter(|(id, _)| current_members.contains(id))
                {
                    self.send_letter(&letter, channel)?;
                }
            }
        }
        Ok(())
    }

    pub fn update_views(
        &self,
        outgoing_channels: &mut Channels<impl Write>,
    ) -> Result<(), Reasons> {
        if let Role::Leader(_) = self.role {
            let current_members = self.memberships.get(&self.view_id).unwrap();
            let letter = (
                self.peer_list.id(),
                Message::NEWVIEW {
                    view_id: self.view_id,
                    members: current_members.clone(),
                },
            )
                .into();

            for (_, channel) in outgoing_channels
                .iter_mut()
                .filter(|(id, _)| current_members.contains(id))
            {
                self.send_letter(&letter, channel)?;
            }
        }
        Ok(())
    }
}
