use std::{
    collections::HashMap,
    os::fd::{AsFd, AsRawFd},
    sync::{
        mpsc::{channel, Receiver},
        Arc,
    },
    thread::spawn,
    time::{Duration, Instant},
};

use chrono::TimeDelta;
use nix::poll::{poll, PollFd, PollFlags, PollTimeout};
use timer::{Guard, Timer};

use crate::{
    failures::Reasons,
    hostsfile::{Broadcaster, PeerList},
    Letter, Message,
};

use super::PeerId;

// sends stuff real fast real easy
pub struct Heart {
    broadcaster: Arc<Broadcaster>,
    rec: Receiver<Letter>,
    timer: Option<Timer>,
}

impl Heart {
    pub fn new(peer_list: &PeerList) -> Result<Self, Reasons> {
        let broadcaster = Arc::new(peer_list.make_broadcaster()?);
        let (tx, rec) = channel::<Letter>();

        let bc = Arc::clone(&broadcaster);
        // NOTE: this thread seems to be running just fine
        spawn(move || {
            let mut poll_fds: Vec<PollFd> =
                bc.0.iter()
                    .map(|(_, s)| PollFd::new(s.as_fd(), PollFlags::POLLIN))
                    .collect();
            let tx = tx;

            loop {
                if let Ok(events) = poll(&mut poll_fds, PollTimeout::NONE) {
                    if events == 0 {
                        continue;
                    }
                }
                for pfd in poll_fds.iter().filter(|p| {
                    p.revents()
                        .unwrap_or(PollFlags::empty())
                        .contains(PollFlags::POLLIN)
                }) {
                    let sock =
                        bc.0.iter()
                            .find(|(_, s)| s.as_raw_fd() == pfd.as_fd().as_raw_fd())
                            .expect("Socket should exist");

                    let mut buf = [0; 1024];
                    let bytes_read = sock.1.recv(&mut buf).expect("Successful Read");

                    if let Ok(letter) = bincode::deserialize::<Letter>(&buf[..bytes_read]) {
                        tx.send(letter).expect("Channel couldn't send");
                    }
                }
            }
        });

        Ok(Heart {
            broadcaster,
            rec,
            timer: None,
        })
    }

    pub fn start(&mut self, repeat: Duration) -> Guard {
        assert!(self.timer.is_none());
        let bc = Arc::clone(&self.broadcaster);

        let timer = Timer::new();
        let out = timer.schedule_repeating(
            TimeDelta::from_std(repeat).expect("Should be small time period"),
            move || {
                bc.beat();
            },
        );

        self.timer = Some(timer);

        out
    }

    /// Polls for heartbeats from peers
    pub fn check_heartbeat(&mut self) -> Option<Letter> {
        self.rec.try_recv().ok()
    }
}

pub enum LifeCycle {
    Born,
    Living(Heart, HashMap<PeerId, Instant>),
}
