use crate::failures::Reasons;
use std::{fs::File, io::Read, path::PathBuf};

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
}
