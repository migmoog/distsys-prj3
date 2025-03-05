use std::{
    collections::HashMap,
    io::Read,
    os::fd::{AsFd, AsRawFd},
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

    if !data.is_leader() {
        data.ask_to_join(
            outgoing_channels
                .get_mut(&1)
                .expect("Should be a leader process id"),
        )?;
    }

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
            match letter.message() {
                // Member requests/confirmations
                Message::JOIN => data.req_to_members(&mut outgoing_channels)?,

                Message::OK { .. } => {}

                // Leader responses
                Message::REQ(instruction) => data.send_ok(
                    &mut outgoing_channels,
                    instruction.view_id,
                    instruction.request_id,
                )?,
                Message::NEWVIEW { .. } => {
                    // nothing gets sent back at this point
                }
            }

            // if we have any satisfied OKs then send a newview
            data.flush_instructions(&mut outgoing_channels)?;
        }
    }
}
