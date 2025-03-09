#[derive(Debug)]
pub enum Reasons {
    IO(std::io::Error),
    HostNotInHostsfile,
    BadMessage,
    BadTimeConversion,
}
