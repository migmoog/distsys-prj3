use crate::{
    failures::Reasons,
    socketry::{make_addr, setup_broadcaster},
    state::PeerId,
    Letter,
};
use std::{collections::HashSet, fs::File, io::Read, net::UdpSocket, path::PathBuf};

// Decouples a stage and organizes code better
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
    pub fn make_broadcaster(&self) -> Result<UdpSocket, Reasons> {
        setup_broadcaster(&self.0)
    }

    pub fn broadcast_letter(
        &self,
        broadcaster: &mut UdpSocket,
        letter: &Letter,
    ) -> Result<(), Reasons> {
        let encoded_buffer = bincode::serialize(letter).map_err(|_| Reasons::BadMessage)?;
        for (_, peer_name) in self.ids_and_names() {
            broadcaster
                .send_to(&encoded_buffer, make_addr(peer_name))
                .map_err(Reasons::IO)?;
        }
        Ok(())
    }
}
