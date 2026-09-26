#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeatherKind {
    Clear,
    Cloudy,
    Rain,
    Storm,
    Snow,
    Blizzard,
    Fog,
    Wind,
    Ash,
    Dust,
    Indoor,
    Unknown,
}

impl WeatherKind {
    pub fn classify(value: &str) -> Self {
        let normalized = value.to_ascii_lowercase();
        // RoTS sends sector-specific prose, not the numeric SKY_* enum. Order
        // matters: cloudless swamp text mentions mist and blizzards mention storms.
        if normalized.contains("no feeling about the weather") || normalized.trim() == "indoors" {
            Self::Indoor
        } else if normalized.contains("blizzard") || normalized.contains("snowstorm") {
            Self::Blizzard
        } else if normalized.contains("storm") || normalized.contains("lightning") {
            Self::Storm
        } else if normalized.contains("rain")
            || normalized.contains("stream of water pours in from above")
        {
            Self::Rain
        } else if normalized.contains("snow") || normalized.contains("slowflake") {
            Self::Snow
        } else if normalized.contains("cloudless")
            || normalized.contains("not a cloud")
            || normalized.contains("no clouds")
            || normalized.contains("clear")
        {
            Self::Clear
        } else if normalized.contains("cloud") || normalized.contains("overcast") {
            Self::Cloudy
        } else if normalized.contains("fog") || normalized.contains("mist") {
            Self::Fog
        } else if normalized.contains("wind") {
            Self::Wind
        } else if normalized.contains("ash") {
            Self::Ash
        } else if normalized.contains("dust") {
            Self::Dust
        } else {
            Self::Unknown
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
    fn unknown_indoor_and_generic_weather_are_distinct() {
        for (text, expected) in [
            ("", WeatherKind::Unknown),
            ("unrecognized weather", WeatherKind::Unknown),
            (
                "You can have no feeling about the weather here.",
                WeatherKind::Indoor,
            ),
            ("indoors", WeatherKind::Indoor),
            ("thick fog", WeatherKind::Fog),
            ("morning mist", WeatherKind::Fog),
            ("strong wind", WeatherKind::Wind),
            ("falling ash", WeatherKind::Ash),
            ("dust", WeatherKind::Dust),
            ("overcast", WeatherKind::Cloudy),
        ] {
            assert_eq!(WeatherKind::classify(text), expected, "{text}");
        }
    }

    #[test]
    fn every_nonempty_rots_sky_message_matches_its_server_row() {
        // Exact nonempty weather_messages[SKY_* + 2][sector] cells from
        // RotS_Live/src/weather.cpp. Preserve duplicates, embedded CR/LF, and
        // server spelling mistakes: MSDP WEATHER transmits this prose verbatim
        // except for its trailing line break (comm.cpp).
        let rows: &[(WeatherKind, &[&str])] = &[
            (
                WeatherKind::Clear,
                &[
                    "Not a cloud can be seen in the sky.",
                    "Above the fields, not a cloud can be seen in the sky.",
                    "Through the trees a cloudless sky can be seen.",
                    "No clouds can be seen in the sky.",
                    "No clouds can be seen in the sky above the mountains.",
                    "Above the water a cloudless sky can be seen.",
                    "Above the water a cloudless sky can be seen.",
                    "Not a cloud can be seen in the sky.",
                    "Far above, a cloudless sky can just be seen.",
                    "The thick trees almost block out the sky, though it appears cloudless.",
                    "A slight mist rises from the damp land into the cloudless sky.",
                ],
            ),
            (
                WeatherKind::Cloudy,
                &[
                    "Clouds cover the sky.",
                    "Clouds race through the sky above the fields.",
                    "Clouds can be seen above in the sky.",
                    "Clouds race across the sky above the hills.",
                    "Higher still than the mountains are the thick clouds which cover the sky.",
                    "The cloudy sky makes the water appear cold and lifeless.",
                    "Clouds race across the sky turning the water dark and sullen.",
                    "The dark cloudy sky makes it difficult to see far down the road.",
                    "Far above, a cloudy sky can just be seen.",
                    "It is almost impossible to see the sky here, though it appears to be cloudy.",
                    "The sky is hidden by clouds making this area damp and chill.",
                ],
            ),
            (
                WeatherKind::Rain,
                &[
                    "Puddles form on the street as heavy rain falls.",
                    "Rain falls heavily on the fields.",
                    "Large drops of water fall from the trees, as heavy rain falls overhead.",
                    "Rain falls heavily on the hills, forming little streams as it runs away.",
                    "Rain lashes down over the mountains, chilling you to the bone.",
                    "Rain falls heavily onto the water, dampening its motion.",
                    "Rain lashes down onto the water, stinging the eyes and soaking you to the skin.",
                    "Puddles form in ruts in the road as rain quickly falls.",
                    "A steady stream of water pours in from above.",
                    "Heavy rain can be heard above through the trees,\n\r  and droplets of water fall heavily to the ground here.",
                    "Rain falls heavily on the already saturated ground making movement difficuilt.",
                ],
            ),
            (
                WeatherKind::Storm,
                &[
                    "Lightning lights up the sullen sky through the rain.",
                    "Lightning lights up the fields as the rain continues.",
                    "Flashes of lightning can be seen through the trees.",
                    "Flashes of lightning are visible over the hills and driving rain.",
                    "The mountains are lit by flashes of lightning as the storm intensifies.",
                    "The water is lit by flashes of lightning.",
                    "The water is occasionally lit by flashes of lightning.",
                    "The road is suddenly lit by violent flashes of lightning as the rain continues.",
                    "Occasional flashes of lightning illuminate this area.",
                    "Flashes of lightning and the sound of heavy rain comes through the trees.",
                    "The storm intesifies as flashes of lightning pierce the sky.",
                ],
            ),
            (
                WeatherKind::Snow,
                &[
                    "The street is slowly covered by softly falling snow.",
                    "The fields become blankets of white as snow quickly covers the ground.",
                    "Wet snow falls through the trees, but cannot cover the ground.",
                    "The hills quickly become covered in thick snow.",
                    "Driving snow and a freezing wind blow fiercely down the mountain.",
                    "Snow falls gently instantly thawing as it hits the water.",
                    "Snow falls gently but disappears as soon as it hits the water.",
                    "The road slowly starts to disappear under a carpet of snow.",
                    "The occasional slowflake drifts down.",
                    "Now and again the occasional snowflake drifts through the trees.",
                    "Snow falls on the wet ground, but only in a few places does it manage to settle.",
                ],
            ),
            (
                WeatherKind::Blizzard,
                &[
                    "It is hard to see anything as the snowstorm intensifies.",
                    "Blizzard conditions prevail as the storm intensifies.",
                ],
            ),
        ];
        assert_eq!(rows.iter().map(|(_, row)| row.len()).sum::<usize>(), 57);
        for &(kind, messages) in rows {
            for message in messages {
                assert_eq!(WeatherKind::classify(message), kind, "{message}");
                assert_eq!(WeatherKind::classify(&format!("{message}\n\r")), kind);
                assert_eq!(WeatherKind::classify(&message.to_ascii_uppercase()), kind);
            }
        }
    }
}
