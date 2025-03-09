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

pub fn make_addr(peer_name: &str) -> String {
    format!("{}:{}", peer_name, PORT)
}

// to decomplicate things
fn attempt_op<Socket, F>(op: F, peer_name: &str) -> Result<Socket, Reasons>
where
    F: Fn(String) -> std::io::Result<Socket>,
{
    let mut attempts = 0;

    let sock = loop {
        match op(make_addr(peer_name)) {
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
    attempt_op(TcpStream::connect, to_send)
}

// Sets up a TCPListener to await connections from all peers
pub fn bind_listener(hostname: &str) -> Result<TcpListener, Reasons> {
    attempt_op(TcpListener::bind, hostname)
}

// Sets up the broadcaster for sending heartbeats
pub fn setup_broadcaster(hostname: &str) -> Result<UdpSocket, Reasons> {
    attempt_op(UdpSocket::bind, hostname)
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
