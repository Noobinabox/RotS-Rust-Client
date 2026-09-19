#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeatherKind {
    Clear,
    Rain,
    Storm,
    Snow,
    Fog,
    Wind,
    Ash,
    Dust,
}

impl WeatherKind {
    pub fn classify(value: &str) -> Self {
        let normalized = value.to_ascii_lowercase();
        if normalized.contains("storm") || normalized.contains("lightning") {
            Self::Storm
        } else if normalized.contains("rain") {
            Self::Rain
        } else if normalized.contains("snow") {
            Self::Snow
        } else if normalized.contains("fog") || normalized.contains("mist") {
            Self::Fog
        } else if normalized.contains("wind") {
            Self::Wind
        } else if normalized.contains("ash") {
            Self::Ash
        } else if normalized.contains("dust") {
            Self::Dust
        } else {
            Self::Clear
        }
    }

    pub fn frame(self, index: usize) -> &'static str {
        match self {
            Self::Clear => ["·", " "][index % 2],
            Self::Rain => ["╱", "│", "╲"][index % 3],
            Self::Storm => ["╱", "⚡", "╲"][index % 3],
            Self::Snow => ["*", "·", "+"][index % 3],
            Self::Fog => ["~", "≈"][index % 2],
            Self::Wind => [">", "»", "-"][index % 3],
            Self::Ash => ["*", ".", "`"][index % 3],
            Self::Dust => [".", ":", "'"][index % 3],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_weather_text() {
        assert_eq!(
            WeatherKind::classify("A fierce storm rolls in"),
            WeatherKind::Storm
        );
        assert_eq!(WeatherKind::classify("light rain"), WeatherKind::Rain);
        assert_eq!(WeatherKind::classify("clear skies"), WeatherKind::Clear);
    }

    #[test]
    fn selects_deterministic_frames() {
        assert_eq!(WeatherKind::Rain.frame(0), "╱");
        assert_eq!(WeatherKind::Rain.frame(3), "╱");
        assert_eq!(WeatherKind::Snow.frame(1), "·");
    }
}
