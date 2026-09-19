use crate::network::msdp::{TELOPT_MSDP, parse_msdp_payload};

use super::msdp::MsdpFrame;

pub const IAC: u8 = 255;
pub const DONT: u8 = 254;
pub const DO: u8 = 253;
pub const WONT: u8 = 252;
pub const WILL: u8 = 251;
pub const SB: u8 = 250;
pub const SE: u8 = 240;
pub const TELOPT_NAWS: u8 = 31;
pub const MAX_SUBNEGOTIATION_LEN: usize = 8192;

#[derive(Debug, Clone, PartialEq)]
pub enum TelnetEvent {
    Text(Vec<u8>),
    Msdp(Vec<MsdpFrame>),
    MsdpEnabled,
    NawsRequested,
    Send(Vec<u8>),
    ProtocolError(String),
}

#[derive(Debug, Default)]
pub struct TelnetParser {
    state: TelnetState,
    text: Vec<u8>,
    subnegotiation: Vec<u8>,
    subnegotiation_option: Option<u8>,
}

#[derive(Debug, Clone, Copy, Default)]
enum TelnetState {
    #[default]
    Data,
    Iac,
    Command(u8),
    SubnegotiationOption,
    Subnegotiation,
    SubnegotiationIac,
}

impl TelnetParser {
    pub fn push(&mut self, bytes: &[u8]) -> Vec<TelnetEvent> {
        let mut events = Vec::new();

        for &byte in bytes {
            match self.state {
                TelnetState::Data => {
                    if byte == IAC {
                        self.flush_text(&mut events);
                        self.state = TelnetState::Iac;
                    } else {
                        self.text.push(byte);
                    }
                }
                TelnetState::Iac => match byte {
                    IAC => {
                        self.text.push(IAC);
                        self.state = TelnetState::Data;
                    }
                    DO | DONT | WILL | WONT => {
                        self.state = TelnetState::Command(byte);
                    }
                    SB => {
                        self.subnegotiation.clear();
                        self.subnegotiation_option = None;
                        self.state = TelnetState::SubnegotiationOption;
                    }
                    _ => {
                        self.state = TelnetState::Data;
                    }
                },
                TelnetState::Command(command) => {
                    self.handle_negotiation(command, byte, &mut events);
                    self.state = TelnetState::Data;
                }
                TelnetState::SubnegotiationOption => {
                    self.subnegotiation_option = Some(byte);
                    self.state = TelnetState::Subnegotiation;
                }
                TelnetState::Subnegotiation => {
                    if byte == IAC {
                        self.state = TelnetState::SubnegotiationIac;
                    } else {
                        self.subnegotiation.push(byte);
                        self.enforce_subnegotiation_limit(&mut events);
                    }
                }
                TelnetState::SubnegotiationIac => match byte {
                    IAC => {
                        self.subnegotiation.push(IAC);
                        self.enforce_subnegotiation_limit(&mut events);
                        self.state = TelnetState::Subnegotiation;
                    }
                    SE => {
                        self.finish_subnegotiation(&mut events);
                        self.state = TelnetState::Data;
                    }
                    _ => {
                        events.push(TelnetEvent::ProtocolError(
                            "unexpected byte inside subnegotiation".to_string(),
                        ));
                        self.state = TelnetState::Data;
                    }
                },
            }
        }

        self.flush_text(&mut events);
        events
    }

    fn flush_text(&mut self, events: &mut Vec<TelnetEvent>) {
        if !self.text.is_empty() {
            events.push(TelnetEvent::Text(std::mem::take(&mut self.text)));
        }
    }

    fn handle_negotiation(&mut self, command: u8, option: u8, events: &mut Vec<TelnetEvent>) {
        match (command, option) {
            (WILL, TELOPT_MSDP) => {
                events.push(TelnetEvent::Send(vec![IAC, DO, TELOPT_MSDP]));
                events.push(TelnetEvent::MsdpEnabled);
            }
            (DO, TELOPT_MSDP) => {
                events.push(TelnetEvent::Send(vec![IAC, WILL, TELOPT_MSDP]));
                events.push(TelnetEvent::MsdpEnabled);
            }
            (DO, TELOPT_NAWS) => {
                events.push(TelnetEvent::Send(vec![IAC, WILL, TELOPT_NAWS]));
                events.push(TelnetEvent::NawsRequested);
            }
            (WILL, _) => events.push(TelnetEvent::Send(vec![IAC, DONT, option])),
            (DO, _) => events.push(TelnetEvent::Send(vec![IAC, WONT, option])),
            (DONT | WONT, _) => {}
            _ => {}
        }
    }

    fn finish_subnegotiation(&mut self, events: &mut Vec<TelnetEvent>) {
        if self.subnegotiation_option == Some(TELOPT_MSDP) {
            match parse_msdp_payload(&self.subnegotiation) {
                Ok(frames) => events.push(TelnetEvent::Msdp(frames)),
                Err(error) => events.push(TelnetEvent::ProtocolError(error.message)),
            }
        }
        self.subnegotiation.clear();
        self.subnegotiation_option = None;
    }

    fn enforce_subnegotiation_limit(&mut self, events: &mut Vec<TelnetEvent>) {
        if self.subnegotiation.len() > MAX_SUBNEGOTIATION_LEN {
            events.push(TelnetEvent::ProtocolError(format!(
                "telnet subnegotiation exceeded {MAX_SUBNEGOTIATION_LEN} bytes"
            )));
            self.subnegotiation.clear();
            self.subnegotiation_option = None;
            self.state = TelnetState::Data;
        }
    }
}

pub fn wrap_msdp(payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(payload.len() + 5);
    frame.extend_from_slice(&[IAC, SB, TELOPT_MSDP]);
    for &byte in payload {
        frame.push(byte);
        if byte == IAC {
            frame.push(IAC);
        }
    }
    frame.extend_from_slice(&[IAC, SE]);
    frame
}

pub fn naws_frame(width: u16, height: u16) -> Vec<u8> {
    let mut frame = vec![IAC, SB, TELOPT_NAWS];
    append_escaped_u16(width, &mut frame);
    append_escaped_u16(height, &mut frame);
    frame.extend_from_slice(&[IAC, SE]);
    frame
}

pub fn naws_will() -> Vec<u8> {
    vec![IAC, WILL, TELOPT_NAWS]
}

fn append_escaped_u16(value: u16, output: &mut Vec<u8>) {
    for byte in value.to_be_bytes() {
        output.push(byte);
        if byte == IAC {
            output.push(IAC);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::network::msdp::encode_pair;

    use super::*;

    #[test]
    fn replies_to_msdp_negotiation() {
        let mut parser = TelnetParser::default();
        let events = parser.push(&[IAC, WILL, TELOPT_MSDP]);
        assert!(events.contains(&TelnetEvent::Send(vec![IAC, DO, TELOPT_MSDP])));
        assert!(events.contains(&TelnetEvent::MsdpEnabled));
    }

    #[test]
    fn replies_to_naws_negotiation() {
        let mut parser = TelnetParser::default();
        let events = parser.push(&[IAC, DO, TELOPT_NAWS]);

        assert!(events.contains(&TelnetEvent::Send(vec![IAC, WILL, TELOPT_NAWS])));
        assert!(events.contains(&TelnetEvent::NawsRequested));
    }

    #[test]
    fn naws_frame_encodes_window_size() {
        assert_eq!(
            naws_frame(120, 29),
            vec![IAC, SB, TELOPT_NAWS, 0, 120, 0, 29, IAC, SE]
        );
    }

    #[test]
    fn naws_frame_escapes_iac_bytes() {
        assert_eq!(
            naws_frame(255, 511),
            vec![IAC, SB, TELOPT_NAWS, 0, IAC, IAC, 1, IAC, IAC, IAC, SE]
        );
    }

    #[test]
    fn parses_partial_msdp_subnegotiation() {
        let mut parser = TelnetParser::default();
        let mut bytes = vec![IAC, SB, TELOPT_MSDP];
        bytes.extend_from_slice(&encode_pair("HEALTH", "88"));

        assert!(parser.push(&bytes).is_empty());
        let events = parser.push(&[IAC, SE]);

        assert!(matches!(&events[0], TelnetEvent::Msdp(frames) if frames[0].variable == "HEALTH"));
    }

    #[test]
    fn preserves_escaped_iac_in_text() {
        let mut parser = TelnetParser::default();
        let events = parser.push(&[b'a', IAC, IAC, b'b']);
        let text = events
            .into_iter()
            .filter_map(|event| match event {
                TelnetEvent::Text(bytes) => Some(bytes),
                _ => None,
            })
            .flatten()
            .collect::<Vec<_>>();
        assert_eq!(text, vec![b'a', IAC, b'b']);
    }

    #[test]
    fn bounds_unterminated_subnegotiation() {
        let mut parser = TelnetParser::default();
        let mut bytes = vec![IAC, SB, TELOPT_MSDP];
        bytes.extend(std::iter::repeat_n(b'x', MAX_SUBNEGOTIATION_LEN + 1));

        let events = parser.push(&bytes);

        assert!(
            events
                .iter()
                .any(|event| matches!(event, TelnetEvent::ProtocolError(message) if message.contains("exceeded")))
        );
    }
}
