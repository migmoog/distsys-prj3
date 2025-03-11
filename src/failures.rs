#[derive(Debug)]
pub enum Reasons {
    #[allow(dead_code)]
    IO(std::io::Error),
    HostNotInHostsfile,
    BadMessage,
}
