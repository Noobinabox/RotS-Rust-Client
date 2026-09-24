use mud_client::{
    app::App,
    config::{AppConfig, ConfigLoadOptions, default_config_path},
    error::{MudClientError, Result},
};

#[tokio::main]
async fn main() -> Result<()> {
    let options = CliOptions::parse(std::env::args().skip(1))?;
    if options.help {
        print_help();
        return Ok(());
    }
    let load_options = ConfigLoadOptions {
        local_test_endpoint: options.local,
    };
    let config_path = default_config_path();
    let character_path = options
        .character
        .as_deref()
        .map(|name| {
            let base = config_path.as_deref().ok_or_else(|| {
                MudClientError::Cli(
                    "cannot locate the configuration directory for character profiles".into(),
                )
            })?;
            mud_client::profiles::character_path(base, name)
        })
        .transpose()?;
    let character_path = character_path
        .map(mud_client::profiles::existing_character_path)
        .transpose()?
        .flatten();
    let config = AppConfig::load_with_character(
        config_path.clone(),
        character_path.as_deref(),
        load_options,
    )?;
    config.init_logging();

    let mut app = App::new_with_character_source(config, config_path, load_options, character_path);
    app.run().await
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct CliOptions {
    help: bool,
    local: bool,
    character: Option<String>,
}

impl CliOptions {
    fn parse(args: impl IntoIterator<Item = String>) -> Result<Self> {
        let mut options = Self::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--help" | "-h" => options.help = true,
                "--local" => options.local = true,
                "--character" => {
                    if options.character.is_some() {
                        return Err(MudClientError::Cli(
                            "--character may only be supplied once".into(),
                        ));
                    }
                    let name = args
                        .next()
                        .ok_or_else(|| MudClientError::Cli("--character requires a name".into()))?;
                    mud_client::profiles::character_path(
                        std::path::Path::new("config.toml"),
                        &name,
                    )?;
                    options.character = Some(name);
                }
                _ => return Err(MudClientError::Cli(format!("unknown argument `{arg}`"))),
            }
        }
        Ok(options)
    }
}

fn print_help() {
    println!(
        "mud-client\n\nUsage:\n  mud-client [--local] [--character NAME]\n\nOptions:\n  --local   Force localhost:3791 even when config points elsewhere\n  --character NAME  Load characters/NAME.toml over shared config.toml (lowercase filename)\n  -h, --help  Show this help"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_parses_local_and_help() {
        let options = CliOptions::parse(["--local".to_string(), "--help".to_string()])
            .expect("valid flags should parse");

        assert!(options.local);
        assert!(options.help);
    }

    #[test]
    fn cli_rejects_unknown_args() {
        assert!(CliOptions::parse(["--locla".to_string()]).is_err());
    }

    #[test]
    fn cli_selects_character_and_retains_local_mode() {
        let options = CliOptions::parse(
            ["--character", "Aragorn", "--local"]
                .map(str::to_owned)
                .into_iter(),
        )
        .unwrap();
        assert_eq!(options.character.as_deref(), Some("Aragorn"));
        assert!(options.local);
    }

    #[test]
    fn cli_rejects_missing_duplicate_or_unsafe_character_names() {
        for args in [
            vec!["--character"],
            vec!["--character", "--local"],
            vec!["--character", "../other"],
            vec!["--character", "a", "--character", "b"],
            vec!["--server", "other"],
        ] {
            assert!(CliOptions::parse(args.into_iter().map(str::to_owned)).is_err());
        }
    }
}
