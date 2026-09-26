//! Presentation-only daylight derived from the server's in-game clock.
//!
//! RoTS `weather.cpp` emits WORLD_TIME as `It is about 8:00 AM on `:
//! the MSDP value has no month or authoritative sunlight state. The client uses
//! an approximate 06:00 sunrise and 18:00 sunset solely for presentation, not
//! the native seasonal schedule. Never use this value for automation decisions.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Daylight {
    Dawn,
    Day,
    Dusk,
    Night,
    Unknown,
}

const APPROXIMATE_SUNRISE: u8 = 6;
const APPROXIMATE_SUNSET: u8 = 18;

impl Daylight {
    /// Classify the in-game clock, never local wall-clock time. Unrecognized or
    /// malformed clock strings return `Unknown`. Sunrise and sunset are fixed
    /// presentation approximations because WORLD_TIME does not include a month.
    pub fn from_world_time(value: Option<&str>) -> Self {
        let Some(value) = value else {
            return Self::Unknown;
        };
        let Some(hour) = parse_hour(value) else {
            return Self::Unknown;
        };
        let (rise, set) = (APPROXIMATE_SUNRISE, APPROXIMATE_SUNSET);
        if hour == rise {
            Self::Dawn
        } else if hour == set {
            Self::Dusk
        } else if hour > rise && hour < set {
            Self::Day
        } else {
            Self::Night
        }
    }
}

fn parse_hour(value: &str) -> Option<u8> {
    let value = value.trim();
    const PREFIX: &str = "It is about ";
    let clock = if value
        .get(..PREFIX.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(PREFIX))
    {
        &value[PREFIX.len()..]
    } else {
        value
    };
    let mut fields = clock.split_whitespace();
    let (hour, minute) = fields.next()?.split_once(':')?;
    if hour.is_empty()
        || hour.len() > 2
        || !hour.bytes().all(|byte| byte.is_ascii_digit())
        || minute.len() != 2
        || !minute.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let hour = hour.parse::<u8>().ok()?;
    let minute = minute.parse::<u8>().ok()?;
    if !(1..=12).contains(&hour) || minute > 59 {
        return None;
    }
    let meridiem = fields.next()?;
    if meridiem.eq_ignore_ascii_case("AM") {
        Some(hour % 12)
    } else if meridiem.eq_ignore_ascii_case("PM") {
        Some(hour % 12 + 12)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_hour_only_values_use_documented_approximation() {
        for (clock, expected) in [
            ("12:00 AM", Daylight::Night),
            ("5:00 AM", Daylight::Night),
            ("6:00 AM", Daylight::Dawn),
            ("7:00 AM", Daylight::Day),
            ("12:00 PM", Daylight::Day),
            ("5:00 PM", Daylight::Day),
            ("6:00 PM", Daylight::Dusk),
            ("7:00 PM", Daylight::Night),
            ("11:00 PM", Daylight::Night),
        ] {
            assert_eq!(
                Daylight::from_world_time(Some(&format!("It is about {clock} on "))),
                expected
            );
        }
    }

    #[test]
    fn noon_midnight_case_and_whitespace_are_unambiguous() {
        assert_eq!(parse_hour("12:00 AM"), Some(0));
        assert_eq!(parse_hour("12:00 PM"), Some(12));
        assert_eq!(parse_hour("  it IS about 01:59 pm on  "), Some(13));
        assert_eq!(Daylight::from_world_time(Some(" 6:30 aM ")), Daylight::Dawn);
    }

    #[test]
    fn absent_or_malformed_times_remain_unknown() {
        assert_eq!(Daylight::from_world_time(None), Daylight::Unknown);
        for value in [
            "",
            "Morning",
            "Late afternoon",
            "00:00 AM",
            "13:00 PM",
            "6:60 AM",
            "6:0 AM",
            "6:00",
            "6:00 afternoon",
            "6:00 AMextra",
            "-1:00 AM",
            "+1:00 AM",
            "999999999999:00 PM",
            "It is about :00 AM on ",
            "雪:00 AM",
            "text 6:00 AM",
        ] {
            assert_eq!(
                Daylight::from_world_time(Some(value)),
                Daylight::Unknown,
                "{value}"
            );
        }
    }
}
