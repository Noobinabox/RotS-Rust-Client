use mud_client::{
    app::App,
    config::{AppConfig, ConfigLoadOptions},
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
    let config = AppConfig::load_with_options(None, load_options)?;
    config.init_logging();

    let mut app = App::new_with_options(config, load_options);
    app.run().await
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct CliOptions {
    help: bool,
    local: bool,
}

impl CliOptions {
    fn parse(args: impl IntoIterator<Item = String>) -> Result<Self> {
        let mut options = Self::default();
        for arg in args {
            match arg.as_str() {
                "--help" | "-h" => options.help = true,
                "--local" => options.local = true,
                _ => return Err(MudClientError::Cli(format!("unknown argument `{arg}`"))),
            }
        }
        Ok(options)
    }
}

fn print_help() {
    println!(
        "mud-client\n\nUsage:\n  mud-client [--local]\n\nOptions:\n  --local   Force localhost:3791 even when config points elsewhere\n  -h, --help  Show this help"
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
}
