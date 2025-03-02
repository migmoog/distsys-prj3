use std::{
    collections::HashMap,
    net::{TcpListener, TcpStream},
    thread::sleep,
    time::Duration,
};

use crate::{failures::Reasons, hostsfile::PeerList};

const PORT: &'static str = "6969";
const MAX_ATTEMPTS: i32 = 10;
const ATTEMPT_WAIT: Duration = Duration::from_secs(5);

fn connect_channel(to_send: &str) -> Result<TcpStream, Reasons> {
    let mut sender_attempts = 0;
    let sendable_address = format!("{}:{}", to_send, PORT);

    let sender = loop {
        match TcpStream::connect(&sendable_address) {
            Ok(s) => break s,
            Err(e) => {
                if sender_attempts == MAX_ATTEMPTS {
                    return Err(Reasons::IO(e));
                }
                sender_attempts += 1;
                sleep(ATTEMPT_WAIT);
            }
        }
    };
    Ok(sender)
}

// Sets up a TCPListener to await connections from all peers
pub fn bind_listener(hostname: &str) -> Result<TcpListener, Reasons> {
    let mut sender_attempts = 0;
    let sendable_address = format!("{}:{}", hostname, PORT);

    let sender = loop {
        match TcpListener::bind(&sendable_address) {
            Ok(s) => break s,
            Err(e) => {
                if sender_attempts == MAX_ATTEMPTS {
                    return Err(Reasons::IO(e));
                }
                sender_attempts += 1;
                sleep(ATTEMPT_WAIT);
            }
        }
    };
    Ok(sender)
}

// creates a vector of listening and sending sockets
// for each process. (connects like a spiderweb across the ring)
pub fn make_channels(peer_list: &PeerList) -> Result<HashMap<usize, TcpStream>, Reasons> {
    let mut out = HashMap::new();
    for (id, peer_name) in peer_list.ids_and_names() {
        let channel_sockets = connect_channel(peer_name)?;
        out.insert(id, channel_sockets);
    }
    Ok(out)
}
