use crossterm::event::{KeyEvent, MouseEvent};

use crate::{commands::ClientCommand, network::msdp::MsdpFrame};

#[derive(Debug, Clone, PartialEq)]
pub enum AppEvent {
    Terminal(TerminalEvent),
    Network(NetworkEvent),
    Timer(TimerEvent),
    Script(ScriptEvent),
    Command(ClientCommand),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptEvent {
    LowHealth,
    ManaLow,
    StaminaLow,
    EnemyEntered { name: String },
    EnemyDefeated { name: String },
    RoomChanged,
    WeatherChanged,
    CommunicationReceived,
    Custom(String),
}

impl ScriptEvent {
    pub fn parse(name: impl Into<String>) -> Self {
        let name = name.into();
        match name.as_str() {
            "LowHealth" => Self::LowHealth,
            "ManaLow" => Self::ManaLow,
            "StaminaLow" => Self::StaminaLow,
            "RoomChanged" => Self::RoomChanged,
            "WeatherChanged" => Self::WeatherChanged,
            "CommunicationReceived" => Self::CommunicationReceived,
            _ => {
                if let Some(name) = name.strip_prefix("EnemyEntered:") {
                    Self::EnemyEntered {
                        name: name.to_string(),
                    }
                } else if let Some(name) = name.strip_prefix("EnemyDefeated:") {
                    Self::EnemyDefeated {
                        name: name.to_string(),
                    }
                } else {
                    Self::Custom(name)
                }
            }
        }
    }

    pub fn name(&self) -> String {
        match self {
            Self::LowHealth => "LowHealth".to_string(),
            Self::ManaLow => "ManaLow".to_string(),
            Self::StaminaLow => "StaminaLow".to_string(),
            Self::EnemyEntered { name } => format!("EnemyEntered:{name}"),
            Self::EnemyDefeated { name } => format!("EnemyDefeated:{name}"),
            Self::RoomChanged => "RoomChanged".to_string(),
            Self::WeatherChanged => "WeatherChanged".to_string(),
            Self::CommunicationReceived => "CommunicationReceived".to_string(),
            Self::Custom(name) => name.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerEvent {
    Tick,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TerminalEvent {
    Key(KeyEvent),
    Paste(String),
    PasteTooLarge,
    Mouse(MouseEvent),
    Resize { width: u16, height: u16 },
}

#[derive(Debug, Clone, PartialEq)]
pub enum NetworkEvent {
    Connected,
    Disconnected,
    Text(String),
    Prompt(String),
    Msdp(Vec<MsdpFrame>),
    Error(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_typed_and_custom_script_events() {
        assert_eq!(ScriptEvent::parse("LowHealth"), ScriptEvent::LowHealth);
        assert_eq!(
            ScriptEvent::parse("EnemyEntered:Orc"),
            ScriptEvent::EnemyEntered {
                name: "Orc".to_string()
            }
        );
        assert_eq!(
            ScriptEvent::parse("CombatStarted:Orc"),
            ScriptEvent::Custom("CombatStarted:Orc".to_string())
        );
    }

    #[test]
    fn script_event_names_round_trip() {
        for event in [
            ScriptEvent::ManaLow,
            ScriptEvent::EnemyDefeated {
                name: "Orc".to_string(),
            },
            ScriptEvent::Custom("CombatStarted:Orc".to_string()),
        ] {
            assert_eq!(ScriptEvent::parse(event.name()), event);
        }
    }
}
