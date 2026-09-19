use crate::{
    config::AppConfig,
    network::{msdp::encode_pair, telnet::wrap_msdp},
};

pub fn rots_msdp_setup(config: &AppConfig) -> Vec<Vec<u8>> {
    let mut frames = vec![
        wrap_msdp(&encode_pair("CLIENT_ID", &config.msdp.client_id)),
        wrap_msdp(&encode_pair("CLIENT_VERSION", &config.msdp.client_version)),
        wrap_msdp(&encode_pair(
            "ANSI_COLORS",
            bool_as_msdp(config.msdp.ansi_colors),
        )),
        wrap_msdp(&encode_pair(
            "XTERM_256_COLORS",
            bool_as_msdp(config.msdp.xterm_256_colors),
        )),
        wrap_msdp(&encode_pair("UTF_8", bool_as_msdp(config.msdp.utf_8))),
    ];

    for variable in &config.msdp.report_variables {
        frames.push(wrap_msdp(&encode_pair("REPORT", variable)));
    }

    frames
}

fn bool_as_msdp(value: bool) -> &'static str {
    if value { "1" } else { "0" }
}

#[cfg(test)]
mod tests {
    use crate::network::telnet::{IAC, SB, SE};

    use super::*;

    #[test]
    fn setup_contains_rots_report_variables() {
        let config = AppConfig::default();
        let frames = rots_msdp_setup(&config);
        let combined = frames.concat();

        assert!(combined.windows(b"REPORT".len()).any(|w| w == b"REPORT"));
        assert!(
            combined
                .windows(b"MOVEMENT".len())
                .any(|w| w == b"MOVEMENT")
        );
        assert!(combined.windows(b"GROUP".len()).any(|w| w == b"GROUP"));
        assert!(frames.iter().all(|frame| frame.starts_with(&[IAC, SB])));
        assert!(frames.iter().all(|frame| frame.ends_with(&[IAC, SE])));
    }
}
