#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientCommand {
    SendText(String),
    SendRaw(Vec<u8>),
    SetWindowSize { width: u16, height: u16 },
    Disconnect,
    Reconnect,
    Quit,
}
