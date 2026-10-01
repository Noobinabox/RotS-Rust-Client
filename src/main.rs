use mud_client::{
    app::App,
    config::{AppConfig, ConfigLoadOptions, default_config_path},
    error::{MudClientError, Result},
};

mod weather_demo;

#[tokio::main]
async fn main() -> Result<()> {
    let options = CliOptions::parse(std::env::args().skip(1))?;
    if options.help {
        print_help();
        return Ok(());
    }
    if options.weather_demo {
        return weather_demo::run();
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
    weather_demo: bool,
}

impl CliOptions {
    fn parse(args: impl IntoIterator<Item = String>) -> Result<Self> {
        let mut options = Self::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--help" | "-h" => options.help = true,
                "--local" => options.local = true,
                "--demo" => {
                    if options.weather_demo || args.next().as_deref() != Some("weather") {
                        return Err(MudClientError::Cli(
                            "use --demo weather once to preview weather offline".into(),
                        ));
                    }
                    options.weather_demo = true;
                }
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
        if options.weather_demo && (options.local || options.character.is_some()) {
            return Err(MudClientError::Cli(
                "--demo weather cannot be combined with --local or --character".into(),
            ));
        }
        Ok(options)
    }
}

fn print_help() {
    println!(
        "mud-client\n\nUsage:\n  mud-client [--local] [--character NAME]\n  mud-client --demo weather\n\nOptions:\n  --local   Force localhost:3791 even when config points elsewhere\n  --character NAME  Load characters/NAME.toml over shared config.toml (lowercase filename)\n  --demo weather  Preview all weather offline without loading configuration\n  -h, --help  Show this help"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_weather_demo_is_explicit_and_isolated() {
        assert!(
            CliOptions::parse(["--demo", "weather"].map(str::to_owned))
                .unwrap()
                .weather_demo
        );
        for args in [
            vec!["--demo"],
            vec!["--demo", "other"],
            vec!["--demo", "weather", "--demo", "weather"],
            vec!["--demo", "weather", "--local"],
            vec!["--local", "--demo", "weather"],
            vec!["--demo", "weather", "--character", "test"],
            vec!["--character", "test", "--demo", "weather"],
        ] {
            assert!(CliOptions::parse(args.into_iter().map(str::to_owned)).is_err());
        }
    }

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
        let options =
            CliOptions::parse(["--character", "Aragorn", "--local"].map(str::to_owned)).unwrap();
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
