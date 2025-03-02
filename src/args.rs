use std::path::PathBuf;

use clap::Parser;

#[derive(Parser)]
pub struct Project3 {
    #[arg(short = 'h')]
    pub hostsfile: PathBuf,

    #[arg(short = 'd')]
    pub start_delay: Option<u32>,

    #[arg(short = 'c')]
    pub join_delay: Option<u32>,

    #[arg(short = 't')]
    pub testcase4: bool,
}
