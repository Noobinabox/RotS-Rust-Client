use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::mpsc,
};
use tracing::{error, info};

use crate::{
    commands::ClientCommand,
    config::AppConfig,
    events::NetworkEvent,
    network::{
        parser::rots_msdp_setup,
        telnet::{TelnetEvent, TelnetParser, naws_frame, naws_will},
    },
};

pub async fn run_connection(
    config: AppConfig,
    network_tx: mpsc::Sender<NetworkEvent>,
    mut command_rx: mpsc::Receiver<ClientCommand>,
    initial_window_size: Option<(u16, u16)>,
) {
    let address = format!("{}:{}", config.connection.host, config.connection.port);
    let mut window_size = initial_window_size;
    let mut next = SessionEnd::Reconnect;

    loop {
        if matches!(next, SessionEnd::Quit) {
            return;
        }
        if matches!(next, SessionEnd::Disconnected) {
            next = wait_for_reconnect(&mut command_rx, &mut window_size).await;
            continue;
        }

        info!(target: "mud_client::network", "[Connection.run_connection] connecting to {}", address);
        let result = connect_session(
            &address,
            &config,
            &network_tx,
            &mut command_rx,
            &mut window_size,
        )
        .await;
        next = match result {
            Ok(end) => end,
            Err(error) => {
                error!(target: "mud_client::network", endpoint = %address, error = %error, "[Connection.run_connection] connection failed");
                let _ = network_tx
                    .send(NetworkEvent::Error(error.to_string()))
                    .await;
                SessionEnd::Disconnected
            }
        };
        if network_tx.send(NetworkEvent::Disconnected).await.is_err() {
            return;
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum SessionEnd {
    Disconnected,
    Reconnect,
    Quit,
}

async fn wait_for_reconnect(
    command_rx: &mut mpsc::Receiver<ClientCommand>,
    window_size: &mut Option<(u16, u16)>,
) -> SessionEnd {
    loop {
        match command_rx.recv().await {
            Some(ClientCommand::Reconnect) => return SessionEnd::Reconnect,
            Some(ClientCommand::Quit) | None => return SessionEnd::Quit,
            Some(ClientCommand::SetWindowSize { width, height }) => {
                *window_size = Some((width, height));
            }
            _ => {}
        }
    }
}

async fn connect_session(
    address: &str,
    config: &AppConfig,
    network_tx: &mpsc::Sender<NetworkEvent>,
    command_rx: &mut mpsc::Receiver<ClientCommand>,
    window_size: &mut Option<(u16, u16)>,
) -> std::io::Result<SessionEnd> {
    let connection = TcpStream::connect(address);
    tokio::pin!(connection);
    let stream = loop {
        tokio::select! {
            biased;
            command = command_rx.recv() => match command {
                Some(ClientCommand::Reconnect) => return Ok(SessionEnd::Reconnect),
                Some(ClientCommand::Disconnect) => return Ok(SessionEnd::Disconnected),
                Some(ClientCommand::Quit) | None => return Ok(SessionEnd::Quit),
                Some(ClientCommand::SetWindowSize { width, height }) => {
                    *window_size = Some((width, height));
                }
                // Commands entered without a live socket must never be replayed.
                _ => {}
            },
            result = &mut connection => break result?,
        }
    };

    let (mut reader, mut writer) = stream.into_split();
    if let Some((width, height)) = *window_size {
        write_naws_size(&mut writer, width, height).await?;
    }
    if network_tx.send(NetworkEvent::Connected).await.is_err() {
        return Ok(SessionEnd::Quit);
    }
    let mut parser = TelnetParser::default();
    let mut output_buffer = OutputAccumulator::new(config.msdp.utf_8);
    let mut buffer = [0_u8; 4096];

    loop {
        tokio::select! {
            read = reader.read(&mut buffer) => {
                match read {
                    Ok(0) => {
                        return Ok(SessionEnd::Disconnected);
                    }
                    Ok(count) => {
                        for event in parser.push(&buffer[..count]) {
                            handle_telnet_event(event, config, network_tx, &mut writer, &mut output_buffer, *window_size).await?;
                        }
                    }
                    Err(error) => {
                        return Err(error);
                    }
                }
            }
            command = command_rx.recv() => {
                match command {
                    Some(ClientCommand::SendText(text)) => {
                        let mut bytes = encode_mud_text(&text, config.msdp.utf_8);
                        bytes.extend_from_slice(config.connection.line_ending.as_bytes());
                        writer.write_all(&bytes).await?;
                    }
                    Some(ClientCommand::SendRaw(bytes)) => {
                        writer.write_all(&bytes).await?;
                    }
                    Some(ClientCommand::SetWindowSize { width, height }) => {
                        *window_size = Some((width, height));
                        write_naws_size(&mut writer, width, height).await?;
                    }
                    Some(ClientCommand::Disconnect) => return Ok(SessionEnd::Disconnected),
                    Some(ClientCommand::Reconnect) => return Ok(SessionEnd::Reconnect),
                    Some(ClientCommand::Quit) | None => return Ok(SessionEnd::Quit),
                }
            }
        }
    }
}

async fn handle_telnet_event(
    event: TelnetEvent,
    config: &AppConfig,
    network_tx: &mpsc::Sender<NetworkEvent>,
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    output_buffer: &mut OutputAccumulator,
    window_size: Option<(u16, u16)>,
) -> std::io::Result<()> {
    match event {
        TelnetEvent::Text(bytes) => {
            let events = output_buffer.push_bytes(&bytes);
            for event in events {
                let _ = network_tx.send(event).await;
            }
        }
        TelnetEvent::Msdp(frames) => {
            let _ = network_tx.send(NetworkEvent::Msdp(frames)).await;
        }
        TelnetEvent::MsdpEnabled => {
            for frame in rots_msdp_setup(config) {
                writer.write_all(&frame).await?;
            }
        }
        TelnetEvent::NawsRequested => {
            if let Some((width, height)) = window_size {
                writer.write_all(&naws_frame(width, height)).await?;
            }
        }
        TelnetEvent::Send(bytes) => {
            writer.write_all(&bytes).await?;
        }
        TelnetEvent::ProtocolError(message) => {
            let _ = network_tx.send(NetworkEvent::Error(message)).await;
        }
    }
    Ok(())
}

async fn write_naws_size(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    width: u16,
    height: u16,
) -> std::io::Result<()> {
    writer.write_all(&naws_will()).await?;
    writer.write_all(&naws_frame(width, height)).await
}

#[derive(Debug)]
struct OutputAccumulator {
    pending: String,
    emitted_prompt: Option<String>,
    control: ControlState,
    control_buffer: String,
    prefer_utf8: bool,
}

impl OutputAccumulator {
    fn new(prefer_utf8: bool) -> Self {
        Self {
            pending: String::new(),
            emitted_prompt: None,
            control: ControlState::default(),
            control_buffer: String::new(),
            prefer_utf8,
        }
    }

    fn push_bytes(&mut self, bytes: &[u8]) -> Vec<NetworkEvent> {
        let mut events = Vec::new();
        let decoded = decode_mud_text(bytes, self.prefer_utf8);
        let mut chars = decoded.chars().peekable();

        while let Some(ch) = chars.next() {
            let Some(ch) = self.filter_control(ch) else {
                continue;
            };
            match ch {
                '\r' => {
                    self.flush_boundary(&mut events);
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                }
                '\n' => self.flush_boundary(&mut events),
                _ => {
                    if self.emitted_prompt.as_deref() == Some(self.pending.as_str()) {
                        self.pending.clear();
                        self.emitted_prompt = None;
                    }
                    self.pending.push(ch);
                }
            }
        }

        if looks_like_prompt(&self.pending)
            && self.emitted_prompt.as_deref() != Some(self.pending.as_str())
        {
            events.push(NetworkEvent::Prompt(self.pending.clone()));
            self.emitted_prompt = Some(self.pending.clone());
        }

        events
    }

    fn filter_control(&mut self, ch: char) -> Option<char> {
        match self.control {
            ControlState::Text => match ch {
                '\x1b' => {
                    self.control_buffer.clear();
                    self.control_buffer.push(ch);
                    self.control = ControlState::Escape;
                    None
                }
                '\x08' => {
                    self.pending.pop();
                    None
                }
                '\t' => Some(' '),
                '\r' | '\n' => Some(ch),
                _ if ch.is_control() => None,
                _ => Some(ch),
            },
            ControlState::Escape => {
                self.control = match ch {
                    '[' => {
                        self.control_buffer.push(ch);
                        ControlState::Csi
                    }
                    ']' => ControlState::Osc,
                    _ => ControlState::Text,
                };
                None
            }
            ControlState::Csi => {
                self.control_buffer.push(ch);
                if ('@'..='~').contains(&ch) {
                    if ch == 'm' {
                        self.pending.push_str(&self.control_buffer);
                    }
                    self.control_buffer.clear();
                    self.control = ControlState::Text;
                }
                None
            }
            ControlState::Osc => {
                if ch == '\x07' {
                    self.control_buffer.clear();
                    self.control = ControlState::Text;
                }
                None
            }
        }
    }

    fn flush_boundary(&mut self, events: &mut Vec<NetworkEvent>) {
        if self.pending.is_empty() {
            self.emitted_prompt = None;
            return;
        }
        if self.emitted_prompt.as_deref() == Some(self.pending.as_str()) {
            self.pending.clear();
            self.emitted_prompt = None;
            return;
        }
        if looks_like_prompt(&self.pending) {
            events.push(NetworkEvent::Prompt(std::mem::take(&mut self.pending)));
        } else {
            events.push(NetworkEvent::Text(std::mem::take(&mut self.pending)));
        }
        self.emitted_prompt = None;
    }
}

impl Default for OutputAccumulator {
    fn default() -> Self {
        Self::new(true)
    }
}

fn decode_mud_text(bytes: &[u8], prefer_utf8: bool) -> String {
    if prefer_utf8 && let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_string();
    }
    bytes
        .iter()
        .map(|&byte| char::from_u32(byte as u32).unwrap_or('\u{FFFD}'))
        .collect()
}

fn encode_mud_text(text: &str, prefer_utf8: bool) -> Vec<u8> {
    if prefer_utf8 {
        return text.as_bytes().to_vec();
    }

    text.chars()
        .map(|character| {
            if (character as u32) <= u8::MAX as u32 {
                character as u8
            } else {
                b'?'
            }
        })
        .collect()
}

#[derive(Debug, Default)]
enum ControlState {
    #[default]
    Text,
    Escape,
    Csi,
    Osc,
}

fn looks_like_prompt(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && (value.ends_with("> ") || value.ends_with(">") || value.ends_with(": "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{future::Future, time::Duration};
    use tokio::net::{TcpListener, TcpSocket};

    async fn bounded(test: impl Future<Output = ()>) {
        tokio::time::timeout(Duration::from_secs(5), test)
            .await
            .expect("connection test timed out");
    }

    fn start_connection(
        address: std::net::SocketAddr,
    ) -> (
        mpsc::Sender<ClientCommand>,
        mpsc::Receiver<NetworkEvent>,
        tokio::task::JoinHandle<()>,
    ) {
        let mut config = AppConfig::default();
        config.connection.host = address.ip().to_string();
        config.connection.port = address.port();
        let (network_tx, network_rx) = mpsc::channel(32);
        let (command_tx, command_rx) = mpsc::channel(32);
        let task = tokio::spawn(run_connection(config, network_tx, command_rx, None));
        (command_tx, network_rx, task)
    }

    #[tokio::test]
    async fn reconnect_closes_old_socket_and_resets_protocol_and_output() {
        bounded(async {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let (commands, mut events, task) = start_connection(listener.local_addr().unwrap());
            let (mut first, _) = listener.accept().await.unwrap();
            assert_eq!(events.recv().await, Some(NetworkEvent::Connected));
            first.write_all(b"marker\nstale\xff").await.unwrap();
            assert_eq!(
                events.recv().await,
                Some(NetworkEvent::Text("marker".into()))
            );
            commands
                .send(ClientCommand::SetWindowSize {
                    width: 101,
                    height: 41,
                })
                .await
                .unwrap();
            let expected_size = [naws_will(), naws_frame(101, 41)].concat();
            let mut size_bytes = vec![0; expected_size.len()];
            first.read_exact(&mut size_bytes).await.unwrap();
            assert_eq!(size_bytes, expected_size);

            commands.send(ClientCommand::Reconnect).await.unwrap();
            assert_eq!(events.recv().await, Some(NetworkEvent::Disconnected));
            let mut byte = [0];
            assert_eq!(first.read(&mut byte).await.unwrap(), 0);
            let (mut second, _) = listener.accept().await.unwrap();
            second.read_exact(&mut size_bytes).await.unwrap();
            assert_eq!(size_bytes, expected_size);
            assert_eq!(events.recv().await, Some(NetworkEvent::Connected));
            second.write_all(b"fresh\n").await.unwrap();
            assert_eq!(
                events.recv().await,
                Some(NetworkEvent::Text("fresh".into()))
            );
            commands.send(ClientCommand::Quit).await.unwrap();
            task.await.unwrap();
            assert_eq!(second.read(&mut byte).await.unwrap(), 0);
        })
        .await;
    }

    #[tokio::test]
    async fn queued_controls_cancel_before_connect_completes() {
        bounded(async {
            for (command, expected) in [
                (Some(ClientCommand::Reconnect), SessionEnd::Reconnect),
                (Some(ClientCommand::Disconnect), SessionEnd::Disconnected),
                (Some(ClientCommand::Quit), SessionEnd::Quit),
                (None, SessionEnd::Quit),
            ] {
                let (commands, mut command_rx) = mpsc::channel(1);
                let (network_tx, mut events) = mpsc::channel(1);
                if let Some(command) = command {
                    commands.send(command).await.unwrap();
                }
                drop(commands);
                // A ready control wins before even polling the invalid endpoint.
                let result = connect_session(
                    "invalid endpoint",
                    &AppConfig::default(),
                    &network_tx,
                    &mut command_rx,
                    &mut None,
                )
                .await
                .unwrap();
                assert_eq!(result, expected);
                assert!(events.try_recv().is_err());
            }
        })
        .await;
    }

    #[tokio::test]
    async fn reconnect_after_eof_and_explicit_disconnect_discards_idle_commands() {
        bounded(async {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let (commands, mut events, task) = start_connection(listener.local_addr().unwrap());
            let (first, _) = listener.accept().await.unwrap();
            assert_eq!(events.recv().await, Some(NetworkEvent::Connected));
            drop(first);
            assert_eq!(events.recv().await, Some(NetworkEvent::Disconnected));
            commands
                .send(ClientCommand::SendText("discard me".into()))
                .await
                .unwrap();
            commands
                .send(ClientCommand::SendRaw(b"discard raw".to_vec()))
                .await
                .unwrap();
            commands
                .send(ClientCommand::SetWindowSize {
                    width: 80,
                    height: 24,
                })
                .await
                .unwrap();
            commands.send(ClientCommand::Reconnect).await.unwrap();
            let (mut second, _) = listener.accept().await.unwrap();
            assert_eq!(events.recv().await, Some(NetworkEvent::Connected));
            let expected_size = [naws_will(), naws_frame(80, 24)].concat();
            let mut bytes = vec![0; expected_size.len()];
            second.read_exact(&mut bytes).await.unwrap();
            assert_eq!(bytes, expected_size);
            commands
                .send(ClientCommand::SendRaw(b"live".to_vec()))
                .await
                .unwrap();
            let mut live = [0; 4];
            second.read_exact(&mut live).await.unwrap();
            assert_eq!(&live, b"live");
            commands.send(ClientCommand::Disconnect).await.unwrap();
            assert_eq!(events.recv().await, Some(NetworkEvent::Disconnected));
            commands.send(ClientCommand::Reconnect).await.unwrap();
            let (_third, _) = listener.accept().await.unwrap();
            assert_eq!(events.recv().await, Some(NetworkEvent::Connected));
            commands.send(ClientCommand::Disconnect).await.unwrap();
            assert_eq!(events.recv().await, Some(NetworkEvent::Disconnected));
            commands.send(ClientCommand::Quit).await.unwrap();
            task.await.unwrap();
        })
        .await;
    }

    #[tokio::test]
    async fn failed_connect_can_reconnect_and_channel_close_exits_idle() {
        bounded(async {
            // Reserve a port without listening so refusal and later recovery are deterministic.
            let socket = TcpSocket::new_v4().unwrap();
            socket.bind("127.0.0.1:0".parse().unwrap()).unwrap();
            let (commands, mut events, task) = start_connection(socket.local_addr().unwrap());
            assert!(matches!(events.recv().await, Some(NetworkEvent::Error(_))));
            assert_eq!(events.recv().await, Some(NetworkEvent::Disconnected));
            commands.send(ClientCommand::Reconnect).await.unwrap();
            assert!(matches!(events.recv().await, Some(NetworkEvent::Error(_))));
            assert_eq!(events.recv().await, Some(NetworkEvent::Disconnected));
            let listener = socket.listen(1).unwrap();
            commands.send(ClientCommand::Reconnect).await.unwrap();
            let (peer, _) = listener.accept().await.unwrap();
            assert_eq!(events.recv().await, Some(NetworkEvent::Connected));
            drop(peer);
            assert_eq!(events.recv().await, Some(NetworkEvent::Disconnected));
            drop(commands);
            task.await.unwrap();
        })
        .await;
    }

    #[test]
    fn output_accumulator_buffers_fragmented_lines() {
        let mut output = OutputAccumulator::default();
        assert!(output.push_bytes(b"You see ").is_empty());
        assert_eq!(
            output.push_bytes(b"a room.\r\n"),
            vec![NetworkEvent::Text("You see a room.".to_string())]
        );
    }

    #[test]
    fn output_accumulator_decodes_latin1_text_without_replacement_characters() {
        let mut output = OutputAccumulator::new(false);

        assert_eq!(
            output.push_bytes(b"You are Jeggred the b\xE1stard\r\n"),
            vec![NetworkEvent::Text(
                "You are Jeggred the b\u{e1}stard".to_string()
            )]
        );
    }

    #[test]
    fn latin1_commands_are_encoded_without_utf8_mojibake() {
        assert_eq!(encode_mud_text("the áastard", false), b"the \xE1astard");
        assert_eq!(
            encode_mud_text("the áastard", true),
            "the áastard".as_bytes()
        );
    }

    #[test]
    fn output_accumulator_splits_line_and_prompt() {
        let mut output = OutputAccumulator::default();
        assert_eq!(
            output.push_bytes(b"Room text\r\nPrompt> "),
            vec![
                NetworkEvent::Text("Room text".to_string()),
                NetworkEvent::Prompt("Prompt> ".to_string())
            ]
        );
    }

    #[test]
    fn output_accumulator_treats_bare_carriage_return_as_boundary() {
        let mut output = OutputAccumulator::default();
        assert_eq!(
            output.push_bytes(b"Gram>\rA tall bear strides through.\r\n"),
            vec![
                NetworkEvent::Prompt("Gram>".to_string()),
                NetworkEvent::Text("A tall bear strides through.".to_string())
            ]
        );
    }

    #[test]
    fn output_accumulator_does_not_merge_live_prompt_with_next_line() {
        let mut output = OutputAccumulator::default();
        assert_eq!(
            output.push_bytes(b"Gram>"),
            vec![NetworkEvent::Prompt("Gram>".to_string())]
        );
        assert_eq!(
            output.push_bytes(b"A friendly little pony is here.\r\n"),
            vec![NetworkEvent::Text(
                "A friendly little pony is here.".to_string()
            )]
        );
    }

    #[test]
    fn output_accumulator_does_not_repeat_same_live_prompt() {
        let mut output = OutputAccumulator::default();
        assert_eq!(
            output.push_bytes(b"Gram>"),
            vec![NetworkEvent::Prompt("Gram>".to_string())]
        );
        assert!(output.push_bytes(b"").is_empty());
        assert!(output.push_bytes(b"\r\n").is_empty());
    }

    #[test]
    fn output_accumulator_strips_ansi_and_backspace_controls() {
        let mut output = OutputAccumulator::default();
        assert_eq!(
            output.push_bytes(b"\x1b[32mA tall, hardy bearr\x08 strides.\x1b[0m\r\n"),
            vec![NetworkEvent::Text(
                "\x1b[32mA tall, hardy bear strides.\x1b[0m".to_string()
            )]
        );
    }
}
