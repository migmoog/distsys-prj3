use std::{
    collections::HashMap,
    net::{TcpListener, TcpStream, UdpSocket},
    thread::sleep,
    time::Duration,
};

use crate::{failures::Reasons, hostsfile::PeerList};

const PORT: &'static str = "6969";
const MAX_ATTEMPTS: i32 = 10;
const ATTEMPT_WAIT: Duration = Duration::from_secs(5);

// to decomplicate things
pub fn attempt_op<Socket, F>(op: F, peer_name: &str, port: Option<&str>) -> Result<Socket, Reasons>
where
    F: Fn(String) -> std::io::Result<Socket>,
{
    let mut attempts = 0;

    let sock = loop {
        match op(format!("{}:{}", peer_name, port.unwrap_or(PORT))) {
            Ok(s) => break s,
            Err(e) => {
                if attempts == MAX_ATTEMPTS {
                    return Err(Reasons::IO(e));
                }
                attempts += 1;
                sleep(ATTEMPT_WAIT);
            }
        }
    };
    Ok(sock)
}

fn connect_channel(to_send: &str) -> Result<TcpStream, Reasons> {
    attempt_op(TcpStream::connect, to_send, None)
}

// Sets up a TCPListener to await connections from all peers
pub fn bind_listener(hostname: &str) -> Result<TcpListener, Reasons> {
    attempt_op(TcpListener::bind, hostname, None)
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
