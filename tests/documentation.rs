use mud_client::config::AppConfig;

#[test]
fn vim_example_enables_opt_in_editing() {
    let guide = include_str!("../docs/commands/vim.md");
    let snippet = guide
        .split("```toml\n")
        .nth(1)
        .unwrap()
        .split("```")
        .next()
        .unwrap();
    let config: AppConfig = toml::from_str(snippet).unwrap();
    config.validate().unwrap();
    assert_eq!(
        config.terminal.input_mode,
        mud_client::config::InputMode::Vim
    );
}

#[test]
fn multiline_input_example_enables_the_documented_option() {
    let guide = include_str!("../docs/commands/input.md");
    let snippet = guide
        .split("```toml\n")
        .nth(1)
        .unwrap()
        .split("```")
        .next()
        .unwrap();
    let config: AppConfig = toml::from_str(snippet).unwrap();
    config.validate().unwrap();
    assert!(config.terminal.multiline_input);
    assert!(!AppConfig::default().terminal.multiline_input);
}

#[test]
fn character_guide_toml_examples_are_valid_configurations() {
    let guide = include_str!("../docs/character-profiles.md");
    let snippets: Vec<_> = guide.split("```toml\n").skip(1).collect();
    assert_eq!(snippets.len(), 2);
    for snippet in snippets {
        let (raw, _) = snippet.split_once("```").unwrap();
        let config: AppConfig = toml::from_str(raw).unwrap();
        config.validate().unwrap();
    }
}

#[test]
fn panel_guide_toml_examples_are_valid_configurations() {
    let guide = include_str!("../docs/commands/panels.md");
    let snippets = guide.split("```toml\n").skip(1).collect::<Vec<_>>();
    assert_eq!(snippets.len(), 3, "keep all three panel recipes validated");
    for (index, snippet) in snippets.iter().enumerate() {
        let (toml, _) = snippet.split_once("```").expect("closed example fence");
        let config: AppConfig = toml::from_str(toml)
            .unwrap_or_else(|error| panic!("panel example {}: {error}", index + 1));
        config
            .validate()
            .unwrap_or_else(|error| panic!("panel example {}: {error}", index + 1));
    }
}
