use crate::state::SocialChannel;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CapturedSocial {
    pub(super) channel: SocialChannel,
    pub(super) prefix: String,
    pub(super) text: String,
}

pub(super) fn classify_social_line(line: &str) -> Option<CapturedSocial> {
    let value = line.trim();
    if let Some((target, text)) = outgoing_target_payload(value, "You tell ") {
        return social_capture(SocialChannel::Tells, format!("to {target} - "), text);
    }
    if let Some(text) = outgoing_payload(value, "You chat ") {
        return social_capture(SocialChannel::Chats, "", text);
    }
    if let Some(text) = outgoing_payload(value, "You say ") {
        return social_capture(SocialChannel::Says, "", text);
    }
    if let Some(text) = outgoing_payload(value, "You narrate ") {
        return social_capture(SocialChannel::Narrates, "", text);
    }
    if let Some(text) = outgoing_payload(value, "You group-say ") {
        return social_capture(SocialChannel::Group, "", text);
    }
    if let Some(text) = outgoing_payload(value, "You yell ") {
        return social_capture(SocialChannel::Yells, "", text);
    }
    if let Some(text) = outgoing_payload(value, "You sing ") {
        return social_capture(SocialChannel::Sings, "", text);
    }
    if let Some((speaker, text)) = incoming_payload(value, " tells you ") {
        return social_capture(SocialChannel::Tells, format!("from {speaker} - "), text);
    }
    if let Some((speaker, text)) = incoming_payload(value, " chats ") {
        return social_capture(SocialChannel::Chats, format!("{speaker} - "), text);
    }
    if let Some((speaker, text)) = incoming_payload(value, " says ") {
        return social_capture(SocialChannel::Says, format!("{speaker} - "), text);
    }
    if let Some((speaker, text)) = incoming_payload(value, " narrates ") {
        return social_capture(SocialChannel::Narrates, format!("{speaker} - "), text);
    }
    if let Some((speaker, text)) = incoming_payload(value, " group-says ") {
        return social_capture(SocialChannel::Group, format!("{speaker} - "), text);
    }
    if let Some((speaker, text)) = incoming_payload(value, " yells ") {
        return social_capture(SocialChannel::Yells, format!("{speaker} - "), text);
    }
    if let Some((speaker, text)) = incoming_payload(value, " sings ") {
        return social_capture(SocialChannel::Sings, format!("{speaker} - "), text);
    }
    None
}

fn social_capture(
    channel: SocialChannel,
    prefix: impl Into<String>,
    text: &str,
) -> Option<CapturedSocial> {
    (!text.is_empty()).then(|| CapturedSocial {
        channel,
        prefix: prefix.into(),
        text: text.to_string(),
    })
}

fn outgoing_payload<'a>(line: &'a str, marker: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(marker)?;
    quoted_payload(rest)
}

fn outgoing_target_payload<'a>(line: &'a str, marker: &str) -> Option<(&'a str, &'a str)> {
    let rest = line.strip_prefix(marker)?;
    let open_quote = rest.find('\'')?;
    let target = rest[..open_quote].trim();
    let text = quoted_payload(rest)?;
    (!target.is_empty()).then_some((target, text))
}

fn incoming_payload<'a>(line: &'a str, marker: &str) -> Option<(&'a str, &'a str)> {
    let index = line.find(marker)?;
    let speaker = line[..index].trim();
    let rest = &line[index + marker.len()..];
    let text = quoted_payload(rest)?;
    (!speaker.is_empty()).then_some((speaker, text))
}

fn quoted_payload(line: &str) -> Option<&str> {
    let open_quote = line.find('\'')?;
    let payload = &line[open_quote + 1..];
    let close_quote = payload.rfind('\'')?;
    Some(&payload[..close_quote])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_social_lines_matches_rots_comm_output() {
        assert_eq!(
            classify_social_line("Gimli tells you 'hello'"),
            Some(CapturedSocial {
                channel: SocialChannel::Tells,
                prefix: "from Gimli - ".to_string(),
                text: "hello".to_string(),
            })
        );
        assert_eq!(
            classify_social_line("You tell Gimli 'hello'"),
            Some(CapturedSocial {
                channel: SocialChannel::Tells,
                prefix: "to Gimli - ".to_string(),
                text: "hello".to_string(),
            })
        );
        assert_eq!(
            classify_social_line("Bilbo chats 'pipeweed?'"),
            Some(CapturedSocial {
                channel: SocialChannel::Chats,
                prefix: "Bilbo - ".to_string(),
                text: "pipeweed?".to_string(),
            })
        );
        assert_eq!(
            classify_social_line("You chat 'ready'"),
            Some(CapturedSocial {
                channel: SocialChannel::Chats,
                prefix: String::new(),
                text: "ready".to_string(),
            })
        );
        assert_eq!(
            classify_social_line("Sam says 'wait'"),
            Some(CapturedSocial {
                channel: SocialChannel::Says,
                prefix: "Sam - ".to_string(),
                text: "wait".to_string(),
            })
        );
        assert_eq!(
            classify_social_line("You narrate 'help'"),
            Some(CapturedSocial {
                channel: SocialChannel::Narrates,
                prefix: String::new(),
                text: "help".to_string(),
            })
        );
        assert_eq!(
            classify_social_line("Frodo group-says 'east'"),
            Some(CapturedSocial {
                channel: SocialChannel::Group,
                prefix: "Frodo - ".to_string(),
                text: "east".to_string(),
            })
        );
        assert_eq!(
            classify_social_line("Pippin yells 'run'"),
            Some(CapturedSocial {
                channel: SocialChannel::Yells,
                prefix: "Pippin - ".to_string(),
                text: "run".to_string(),
            })
        );
        assert_eq!(
            classify_social_line("You sing 'tra-la-la'"),
            Some(CapturedSocial {
                channel: SocialChannel::Sings,
                prefix: String::new(),
                text: "tra-la-la".to_string(),
            })
        );
        assert_eq!(classify_social_line("You narrate no quotes"), None);
        assert_eq!(classify_social_line("A bear strides through."), None);
    }
}
