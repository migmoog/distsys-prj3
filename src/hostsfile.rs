use nix::poll::{self, PollFd, PollFlags, PollTimeout};

use crate::{failures::Reasons, socketry::attempt_op, state::PeerId, Letter, Message};
use std::{
    collections::HashSet,
    fs::File,
    io::Read,
    net::UdpSocket,
    os::fd::{AsFd, AsRawFd},
    path::PathBuf,
};

pub struct Broadcaster(pub Vec<(String, UdpSocket)>, pub Letter);
impl Broadcaster {
    fn new(peer_list: &PeerList) -> Result<Self, Reasons> {
        let mut heart_port = 6790;
        let mut scks = Vec::new();
        for (_, name) in peer_list.ids_and_names() {
            let sock = attempt_op(
                UdpSocket::bind,
                peer_list.hostname(),
                Some(&heart_port.to_string()),
            )?;
            sock.set_nonblocking(true).map_err(Reasons::IO)?;
            scks.push((format!("{}:{}", name, heart_port), sock));
            heart_port += 1;
        }
        let letter = (peer_list.id(), Message::HEARTBEAT).into();
        Ok(Self(scks, letter))
    }

    pub fn beat(&self) {
        let buf = bincode::serialize(&self.1).unwrap();
        let mut poll_fds: Vec<PollFd> = self
            .0
            .iter()
            .map(|s| PollFd::new(s.1.as_fd(), PollFlags::POLLOUT))
            .collect();

        let Ok(events) = poll::poll(&mut poll_fds, PollTimeout::NONE) else {
            return;
        };
        if events == 0 {
            return;
        }

        for pfd in poll_fds.iter().filter(|pfd| {
            pfd.revents()
                .unwrap_or(PollFlags::empty())
                .contains(PollFlags::POLLOUT)
        }) {
            let Some(sock) = self
                .0
                .iter()
                .find(|v| v.1.as_raw_fd() == pfd.as_fd().as_raw_fd())
            else {
                unreachable!();
            };
            sock.1.send_to(&buf, &sock.0).expect("Successful send");
        }
    }
}

// Decouples a stage and organizes code better
#[derive(Clone)]
pub struct PeerList(String, Vec<String>);

impl PeerList {
    /// Reads a hostsfile to create the structure.
    /// This host of this process must be in the hostsfile.
    pub fn load(path: PathBuf) -> Result<Self, Reasons> {
        let hostname = hostname::get()
            .expect("Hostname of image")
            .into_string()
            .unwrap();
        let peer_names: Vec<String> = match File::open(path) {
            Ok(mut f) => {
                let mut out = String::new();
                let _ = f.read_to_string(&mut out);
                out.lines().map(str::to_string).collect()
            }
            Err(e) => return Err(Reasons::IO(e)),
        };

        if !peer_names.contains(&hostname) {
            return Err(Reasons::HostNotInHostsfile);
        }
        Ok(Self(hostname, peer_names))
    }

    /// gets name of host device as it appears on the system/hostsfile
    pub fn hostname(&self) -> &str {
        &self.0
    }

    /// true if the hostname is the first one in the file
    pub fn is_leader(&self) -> bool {
        self.0 == self.1[0]
    }

    /// Retrieve the 1-based id of this process
    pub fn id(&self) -> usize {
        self.1
            .iter()
            .position(|name| *name == self.0)
            .expect("Host should be in hostsfile")
            + 1
    }

    // Returns a slice of all names excluding the host
    pub fn ids_and_names(&self) -> impl Iterator<Item = (usize, &String)> {
        self.1.iter().enumerate().filter_map(|(index, name)| {
            if *name != self.0 {
                Some((index + 1, name))
            } else {
                None
            }
        })
    }

    /// Count of peers this process has (excludes host from list)
    pub fn len(&self) -> usize {
        self.1.len() - 1
    }

    /// Check if the peer ids in the current membership matches the ones in the list
    pub fn members_match_hosts(&self, current_members: &HashSet<PeerId>) -> bool {
        self.1
            .iter()
            .enumerate()
            .map(|(index, _)| index + 1)
            .collect::<HashSet<PeerId>>()
            == *current_members
    }

    /// bind a UDP socket to the host
    pub fn make_broadcaster(&self) -> Result<Broadcaster, Reasons> {
        Broadcaster::new(self)
    }
}
