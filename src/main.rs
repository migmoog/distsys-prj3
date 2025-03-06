use std::{
    collections::HashMap,
    io::Read,
    os::fd::{AsFd, AsRawFd},
    thread::sleep,
    time::Duration,
};

use args::Project3;
use clap::Parser;
use failures::Reasons;
use hostsfile::PeerList;
use nix::poll::{poll, PollFd, PollFlags, PollTimeout};
use socketry::{bind_listener, make_channels};
use state::{messaging::*, Data};

mod args;
mod failures;
mod hostsfile;
mod socketry;
mod state;

fn main() -> Result<(), Reasons> {
    let args = Project3::parse();
    let peer_list = PeerList::load(args.hostsfile)?;

    let listener = bind_listener(peer_list.hostname())?;
    let mut outgoing_channels = make_channels(&peer_list)?;

    let mut incoming_channels = HashMap::new();
    while incoming_channels.len() < peer_list.len() {
        if let Ok((sock, _)) = listener.accept() {
            sock.set_nonblocking(true).map_err(Reasons::IO)?;
            incoming_channels.insert(sock.as_raw_fd(), sock);
        }
    }
    let mut poll_fds: Vec<_> = incoming_channels
        .iter()
        .map(|(_, s)| PollFd::new(s.as_fd(), PollFlags::POLLIN))
        .collect();

    let mut data = Data::new(peer_list);

    // i am a great big fool and need to read the project specs more
    sleep(Duration::from_secs(args.start_delay.unwrap_or(0)));
    data.ask_to_join(&mut outgoing_channels)?;

    loop {
        if poll(&mut poll_fds, PollTimeout::NONE).map_err(|v| Reasons::IO(v.into()))? == 0 {
            continue;
        }

        let mut message_queue = Vec::new();
        for pfd in poll_fds.iter().filter(|pfd| {
            pfd.revents()
                .unwrap_or(PollFlags::empty())
                .contains(PollFlags::POLLIN)
        }) {
            let mut chan = incoming_channels
                .get(&pfd.as_fd().as_raw_fd())
                .expect("Existent channel");
            let mut buffer = [0; 1024];
            let bytes_read = chan.read(&mut buffer).map_err(Reasons::IO)?;
            let letter: Letter =
                bincode::deserialize(&buffer[..bytes_read]).map_err(|_| Reasons::BadMessage)?;
            message_queue.push(letter);
        }

        for letter in message_queue {
            data.recv_message(&letter);
        }
        // send out any reqs we may need to take care of
        data.proceed_reqs(&mut outgoing_channels)?;

        // if we have any satisfied OKs then send a newview
        data.flush_instructions(&mut outgoing_channels)?;
    }
}
