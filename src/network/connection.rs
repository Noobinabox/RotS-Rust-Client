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
    info!(target: "mud_client::network", "connecting to {}", address);

    let stream = match TcpStream::connect(&address).await {
        Ok(stream) => stream,
        Err(error) => {
            let _ = network_tx
                .send(NetworkEvent::Error(error.to_string()))
                .await;
            return;
        }
    };

    let (mut reader, mut writer) = stream.into_split();
    let mut window_size = initial_window_size;
    if let Some((width, height)) = window_size
        && let Err(error) = write_naws_size(&mut writer, width, height).await
    {
        let _ = network_tx
            .send(NetworkEvent::Error(error.to_string()))
            .await;
        return;
    }
    let _ = network_tx.send(NetworkEvent::Connected).await;
    let mut parser = TelnetParser::default();
    let mut output_buffer = OutputAccumulator::default();
    let mut buffer = [0_u8; 4096];

    loop {
        tokio::select! {
            read = reader.read(&mut buffer) => {
                match read {
                    Ok(0) => {
                        let _ = network_tx.send(NetworkEvent::Disconnected).await;
                        return;
                    }
                    Ok(count) => {
                        for event in parser.push(&buffer[..count]) {
                            handle_telnet_event(event, &config, &network_tx, &mut writer, &mut output_buffer, window_size).await;
                        }
                    }
                    Err(error) => {
                        error!(target: "mud_client::network", error = %error, "network read failed");
                        let _ = network_tx.send(NetworkEvent::Error(error.to_string())).await;
                        return;
                    }
                }
            }
            command = command_rx.recv() => {
                match command {
                    Some(ClientCommand::SendText(text)) => {
                        let mut bytes = text.into_bytes();
                        bytes.extend_from_slice(config.connection.line_ending.as_bytes());
                        if let Err(error) = writer.write_all(&bytes).await {
                            let _ = network_tx.send(NetworkEvent::Error(error.to_string())).await;
                            return;
                        }
                    }
                    Some(ClientCommand::SendRaw(bytes)) => {
                        if let Err(error) = writer.write_all(&bytes).await {
                            let _ = network_tx.send(NetworkEvent::Error(error.to_string())).await;
                            return;
                        }
                    }
                    Some(ClientCommand::SetWindowSize { width, height }) => {
                        window_size = Some((width, height));
                        if let Err(error) = write_naws_size(&mut writer, width, height).await {
                            let _ = network_tx.send(NetworkEvent::Error(error.to_string())).await;
                            return;
                        }
                    }
                    Some(ClientCommand::Disconnect | ClientCommand::Quit) | None => {
                        let _ = writer.shutdown().await;
                        let _ = network_tx.send(NetworkEvent::Disconnected).await;
                        return;
                    }
                    Some(ClientCommand::Reconnect) => {}
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
) {
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
                if let Err(error) = writer.write_all(&frame).await {
                    let _ = network_tx
                        .send(NetworkEvent::Error(error.to_string()))
                        .await;
                    return;
                }
            }
        }
        TelnetEvent::NawsRequested => {
            if let Some((width, height)) = window_size
                && let Err(error) = writer.write_all(&naws_frame(width, height)).await
            {
                let _ = network_tx
                    .send(NetworkEvent::Error(error.to_string()))
                    .await;
            }
        }
        TelnetEvent::Send(bytes) => {
            if let Err(error) = writer.write_all(&bytes).await {
                let _ = network_tx
                    .send(NetworkEvent::Error(error.to_string()))
                    .await;
            }
        }
        TelnetEvent::ProtocolError(message) => {
            let _ = network_tx.send(NetworkEvent::Error(message)).await;
        }
    }
}

async fn write_naws_size(
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    width: u16,
    height: u16,
) -> std::io::Result<()> {
    writer.write_all(&naws_will()).await?;
    writer.write_all(&naws_frame(width, height)).await
}

#[derive(Debug, Default)]
struct OutputAccumulator {
    pending: String,
    emitted_prompt: Option<String>,
    control: ControlState,
    control_buffer: String,
}

impl OutputAccumulator {
    fn push_bytes(&mut self, bytes: &[u8]) -> Vec<NetworkEvent> {
        let mut events = Vec::new();
        let decoded = String::from_utf8_lossy(bytes);
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
