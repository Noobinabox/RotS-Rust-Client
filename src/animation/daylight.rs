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
const MINUTES_PER_HOUR: u16 = 60;
const MINUTES_PER_DAY: u16 = 24 * MINUTES_PER_HOUR;

/// The server clock's presentation state, with no local-time extrapolation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkyClock {
    pub daylight: Daylight,
    pub celestial: Option<CelestialPosition>,
    /// Presentation opacity: zero is hidden, 255 is fully visible.
    pub opacity: u8,
}

/// Position along an approximate east-to-west sun or moon passage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CelestialPosition {
    pub moon: bool,
    /// Fraction of the passage: 0 at rising, 0.5 overhead, 1 at setting.
    pub progress: f32,
}

impl SkyClock {
    /// Derive a deterministic sky from WORLD_TIME. The sun travels from 06:00
    /// through 18:00 inclusive; the moon occupies the remaining twelve hours.
    /// These are visual approximations, not authoritative astronomical data.
    pub fn from_world_time(value: Option<&str>) -> Self {
        let Some(minutes) = value.and_then(parse_minutes) else {
            return Self {
                daylight: Daylight::Unknown,
                celestial: None,
                opacity: 255,
            };
        };
        let sunrise = u16::from(APPROXIMATE_SUNRISE) * MINUTES_PER_HOUR;
        let sunset = u16::from(APPROXIMATE_SUNSET) * MINUTES_PER_HOUR;
        let moon = !(sunrise..=sunset).contains(&minutes);
        let (elapsed, duration) = if moon {
            (
                (minutes + MINUTES_PER_DAY - sunset) % MINUTES_PER_DAY,
                MINUTES_PER_DAY - (sunset - sunrise),
            )
        } else {
            (minutes - sunrise, sunset - sunrise)
        };
        Self {
            opacity: 255,
            daylight: Daylight::from_hour((minutes / MINUTES_PER_HOUR) as u8),
            celestial: Some(CelestialPosition {
                moon,
                progress: f32::from(elapsed) / f32::from(duration),
            }),
        }
    }
}

impl Daylight {
    /// Classify the in-game clock, never local wall-clock time. Unrecognized or
    /// malformed clock strings return `Unknown`. Sunrise and sunset are fixed
    /// presentation approximations because WORLD_TIME does not include a month.
    pub fn from_world_time(value: Option<&str>) -> Self {
        let Some(minutes) = value.and_then(parse_minutes) else {
            return Self::Unknown;
        };
        Self::from_hour((minutes / MINUTES_PER_HOUR) as u8)
    }

    fn from_hour(hour: u8) -> Self {
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

fn parse_minutes(value: &str) -> Option<u16> {
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
        Some(u16::from(hour % 12) * MINUTES_PER_HOUR + u16::from(minute))
    } else if meridiem.eq_ignore_ascii_case("PM") {
        Some(u16::from(hour % 12 + 12) * MINUTES_PER_HOUR + u16::from(minute))
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
        assert_eq!(parse_minutes("12:00 AM"), Some(0));
        assert_eq!(parse_minutes("12:00 PM"), Some(720));
        assert_eq!(parse_minutes("  it IS about 01:59 pm on  "), Some(839));
        assert_eq!(Daylight::from_world_time(Some(" 6:30 aM ")), Daylight::Dawn);
    }

    #[test]
    fn absent_or_malformed_times_remain_unknown() {
        assert_eq!(Daylight::from_world_time(None), Daylight::Unknown);
        assert_eq!(SkyClock::from_world_time(None).celestial, None);
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
            assert_eq!(
                SkyClock::from_world_time(Some(value)),
                SkyClock {
                    opacity: 255,
                    daylight: Daylight::Unknown,
                    celestial: None,
                }
            );
        }
    }

    #[test]
    fn celestial_landmarks_and_minute_precision() {
        for (clock, moon, progress) in [
            ("6:00 AM", false, 0.0),
            ("9:00 AM", false, 0.25),
            ("12:00 PM", false, 0.5),
            ("6:00 PM", false, 1.0),
            ("6:01 PM", true, 1.0 / 720.0),
            ("12:00 AM", true, 0.5),
            ("12:30 AM", true, 0.5 + 30.0 / 720.0),
            ("5:59 AM", true, 719.0 / 720.0),
        ] {
            let sky = SkyClock::from_world_time(Some(clock));
            let position = sky.celestial.unwrap();
            assert_eq!(position.moon, moon, "{clock}");
            assert!((position.progress - progress).abs() < 0.000_001, "{clock}");
            assert_eq!(sky.daylight, Daylight::from_world_time(Some(clock)));
        }
    }

    #[test]
    fn full_day_has_bounded_monotonic_passages_and_wraps_midnight() {
        let mut previous: Option<CelestialPosition> = None;
        // Begin at sunrise so the moon's midnight wrap is inside one passage.
        for elapsed in 0..MINUTES_PER_DAY {
            let minutes = (elapsed + 6 * MINUTES_PER_HOUR) % MINUTES_PER_DAY;
            let hour = minutes / MINUTES_PER_HOUR;
            let clock = format!(
                "{}:{:02} {}",
                (hour + 11) % 12 + 1,
                minutes % MINUTES_PER_HOUR,
                if hour < 12 { "AM" } else { "PM" },
            );
            let sky = SkyClock::from_world_time(Some(&clock));
            let position = sky.celestial.unwrap();
            assert!((0.0..=1.0).contains(&position.progress), "{clock}");
            assert_eq!(sky.daylight, Daylight::from_world_time(Some(&clock)));
            if let Some(previous) = previous
                && previous.moon == position.moon
            {
                assert!(position.progress > previous.progress, "{clock}");
            }
            previous = Some(position);
        }
    }
}
