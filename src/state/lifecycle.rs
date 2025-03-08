use std::{net::UdpSocket, sync::Arc, time::Duration};

use chrono::TimeDelta;
use timer::{Guard, Timer};

use crate::{failures::Reasons, hostsfile::PeerList, socketry::setup_broadcaster, Message};

// sends stuff real fast real easy
pub struct Heart(Arc<UdpSocket>, Arc<PeerList>, Option<Timer>);
impl Heart {
    pub fn new(peer_list: PeerList) -> Result<Self, Reasons> {
        let sock = peer_list.make_broadcaster()?;
        Ok(Self(Arc::new(sock), Arc::new(peer_list), None))
    }

    pub fn start(&mut self, repeat: Duration) -> Guard {
        let sock = Arc::clone(&self.0);
        let pl = Arc::clone(&self.1);

        let timer = Timer::new();
        let id = self.1.id();
        println!("POOPY");
        let out = timer.schedule_repeating(
            TimeDelta::from_std(repeat).expect("Should be small time period"),
            move || {
                println!("HeartBeat");
                pl.broadcast_letter(&sock, &(id, Message::HEARTBEAT).into())
                    .expect("Should send");
            },
        );

        self.2 = Some(timer);

        out
    }
}

pub enum LifeCycle {
    Born,
    Living(Heart),
    Dead,
}
