use std::{
    collections::{HashMap, HashSet},
    io::Write,
    thread::{self, sleep},
    time::{Duration, Instant},
    usize,
};

use crate::{failures::Reasons, hostsfile::PeerList};

mod lifecycle;
pub mod messaging;
mod roles;

use lifecycle::{Heart, LifeCycle};
use messaging::{Instruction, Letter, Message, Operation};
use roles::Role;

pub type PeerId = usize;
pub type ViewId = u32;
pub type RequestId = u32;
pub const DEFAULT_LEADER_ID: usize = 1;
const HEARTBEAT_PERIOD: Duration = Duration::from_secs(2);

type Channels<W> = HashMap<usize, W>;
// main state of each process
pub struct Data {
    role: Role,
    status: LifeCycle,
    // membership list recorded across all peers
    memberships: HashMap<ViewId, HashSet<PeerId>>,
    // log of operations to perform
    view_id: ViewId,
    peer_list: PeerList,
    crash_delay: Option<Duration>,
}

impl Data {
    pub fn new(peer_list: PeerList, crash_delay: Option<u64>) -> Self {
        let role = Role::new(peer_list.is_leader());
        Self {
            view_id: 1,
            status: LifeCycle::Born,
            memberships: HashMap::from([(1, HashSet::from([DEFAULT_LEADER_ID]))]),
            peer_list,
            role,
            crash_delay: crash_delay.map(Duration::from_secs),
        }
    }

    /// receives a message from
    pub fn recv_message(&mut self, letter: &Letter) {
        //println!("recv: {:?}", letter);

        use messaging::Message as M;
        if let Role::Leader(ref mut lead) = self.role {
            match letter.message() {
                M::JOIN => {
                    lead.push_request(letter.from_whom(), self.view_id, Operation::Add);
                    lead.acknowledge_ok(lead.latest_request(), self.peer_list.id());
                }
                M::OK { request_id, .. } => {
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
                        members.iter().collect::<Vec<_>>()
                    );
                    self.memberships.insert(self.view_id, members.clone());
                }
                _ => unreachable!(),
            }
        }
    }

    fn send_letter(&self, letter: &Letter, sender: &mut impl Write) -> Result<(), Reasons> {
        //println!("send: {:?}", letter);

        let encoded_buffer = bincode::serialize(&letter).map_err(|_| Reasons::BadMessage)?;
        let _ = sender.write(&encoded_buffer).map_err(Reasons::IO)?;
        Ok(())
    }

    // member methods
    pub fn ask_to_join(&self, outgoing_channels: &mut Channels<impl Write>) -> Result<(), Reasons> {
        if let Role::Follower(ref follow) = self.role {
            let parcel: Letter = (self.peer_list.id(), Message::JOIN).into();
            self.send_letter(
                &parcel,
                outgoing_channels
                    .get_mut(&follow.leader_id())
                    .expect("Channel to leader"),
            )?;
        }

        Ok(())
    }

    // Leader methods //

    // increments view_id and adds a new member to the list
    fn push_new_view(&mut self, peer: PeerId, op: Operation) {
        let mut prev_members = self
            .memberships
            .get(&self.view_id)
            .expect("Should have a view prior to this one existing.")
            .clone();

        if let Operation::Add = op {
            prev_members.insert(peer);
        } else if let Operation::Delete = op {
            assert!(prev_members.remove(&peer));
        }
        self.view_id += 1;
        self.memberships.insert(self.view_id, prev_members);
    }

    // Performs all operations in the queue.
    // Also transitions to sending heartbeat once every peer in the hostsfile joins.
    pub fn flush_instructions(
        &mut self,
        outgoing_channels: &mut Channels<impl Write>,
    ) -> Result<(), Reasons> {
        // Regular instruction flushing
        if let Role::Leader(ref mut lead) = self.role {
            // pop an instruction of the queue after we've gotten all our confirmations
            if let Some(Instruction { peer_id, op, .. }) =
                lead.check_req_complete(&self.memberships)
            {
                self.push_new_view(peer_id, op);
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

        // Preparing to broadcast heartbeats
        if let (LifeCycle::Born, Some(current_members)) =
            (&self.status, self.memberships.get(&self.view_id))
        {
            // once we have all our members we need, we can start sending heartbeats
            if self.peer_list.members_match_hosts(current_members) {
                let prev_beats = self
                    .peer_list
                    .ids_and_names()
                    .map(|(id, _)| (id, Instant::now()))
                    .collect();
                self.status = LifeCycle::Living(Heart::new(&self.peer_list)?, prev_beats);
                // sleep to allow other processes to change their states
                sleep(Duration::from_secs(1));

                let LifeCycle::Living(ref mut heart, _) = &mut self.status else {
                    unreachable!(); // just instanced this
                };
                let beat_stop = heart.start(HEARTBEAT_PERIOD);

                if let Some(dur) = self.crash_delay {
                    let beat_stop = beat_stop;
                    let (id, vid, lid) = (self.peer_list.id(), self.view_id, self.leader_id());
                    thread::spawn(move || {
                        sleep(dur);
                        drop(beat_stop);
                        eprintln!("{{peer_id: {id}, view_id: {vid}, leader: {lid}, message: \"crashing\"}}");
                    });
                } else {
                    beat_stop.ignore();
                }
            }
        }
        Ok(())
    }

    /// Returns the id of the current leader in the system
    fn leader_id(&self) -> usize {
        match &self.role {
            Role::Leader(_) => self.peer_list.id(),
            Role::Follower(ref follow) => follow.leader_id(),
        }
    }

    /// If in the living stage, will poll its heart to check
    /// for heartbeats from its peers
    pub fn validate_peers(&mut self) -> Result<(), Reasons> {
        let lid = self.leader_id();
        let current_members = self.memberships.get(&self.view_id).unwrap();
        if let LifeCycle::Living(ref mut heart, ref mut prev_beats) = &mut self.status {
            let now = Instant::now();
            let mut rm = Vec::new();
            for (&id, &prev) in prev_beats
                .iter()
                .filter(|(id, _)| current_members.contains(id))
            {
                let diff = { now - prev }.as_secs();
                if diff > { HEARTBEAT_PERIOD * 2 }.as_secs() {
                    eprintln!(
                        "{{peer_id: {}, view_id: {}, leader: {}, message:\"peer {} unreachable\"}}",
                        self.peer_list.id(),
                        self.view_id,
                        lid,
                        id,
                    );
                    rm.push(id);
                }
            }

            for rmid in rm {
                if let Role::Leader(ref mut lead) = self.role {
                    lead.push_request(rmid, self.view_id, Operation::Delete);
                    lead.acknowledge_ok(lead.latest_request(), lid);
                }
                prev_beats.remove(&rmid);
            }

            if let Some(letter) = heart.check_heartbeat() {
                assert!(matches!(letter.message(), Message::HEARTBEAT));
                let from = letter.from_whom();
                prev_beats.insert(from, now);
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

            eprintln!(
                "{{proc_id: {}, view_id: {}, leader: {0}, memb_list: {:?}}}",
                self.peer_list.id(),
                self.view_id,
                current_members.iter().collect::<Vec<_>>()
            );

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
