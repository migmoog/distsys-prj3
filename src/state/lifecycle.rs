use std::{net::UdpSocket, os::fd::AsFd, sync::Arc, time::Duration};

use chrono::TimeDelta;
use nix::poll::{poll, PollFd, PollFlags, PollTimeout};
use timer::{Guard, Timer};

use crate::{
    failures::Reasons,
    hostsfile::{Broadcaster, PeerList},
    Letter, Message,
};

// sends stuff real fast real easy
pub struct Heart(Arc<Broadcaster>, Arc<PeerList>, Option<Timer>);

impl Heart {
    pub fn new(peer_list: PeerList) -> Result<Self, Reasons> {
        Ok(Self(
            Arc::new(peer_list.make_broadcaster()?),
            Arc::new(peer_list),
            None,
        ))
    }

    pub fn start(&mut self, repeat: Duration) -> Guard {
        let sock = Arc::clone(&self.0);

        let timer = Timer::new();
        let out = timer.schedule_repeating(
            TimeDelta::from_std(repeat).expect("Should be small time period"),
            move || {
                sock.beat();
            },
        );

        self.2 = Some(timer);

        out
    }

    /// Polls for heartbeats from peers
    pub fn check_heartbeat(&mut self) -> Option<Letter> {
        None
    }
}

pub enum LifeCycle {
    Born,
    Living(Heart),
}
