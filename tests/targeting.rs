use mud_client::{
    color::{AnsiColor, AnsiColors},
    config::AppConfig,
    scripting::{
        lua::{LuaAction, LuaEngine, LuaHookContext},
        triggers::TriggerEngine,
        variables::VariableStore,
    },
    state::{AppState, OutputCategory},
};

struct Harness {
    engine: LuaEngine,
    state: AppState,
    variables: VariableStore,
}

impl Harness {
    fn new() -> Self {
        let mut config = AppConfig::default();
        config.lua.enabled = true;
        config.lua.script_dir = format!("{}/scripts", env!("CARGO_MANIFEST_DIR"));
        config.lua.scripts = Some(vec!["targeting.lua".into()]);
        let state = AppState::new(&config);
        let variables = VariableStore::empty();
        let engine = LuaEngine::new(&config.lua, None, &state, &variables);
        assert!(
            !engine.status().contains("Last error"),
            "{}",
            engine.status()
        );
        Self {
            engine,
            state,
            variables,
        }
    }

    fn call(&mut self, hook: &str, context: LuaHookContext) -> Vec<LuaAction> {
        self.engine
            .call_hook(hook, context, &self.state, &self.variables)
            .unwrap()
            .actions
    }

    fn observe(&mut self, line: &str) -> Vec<LuaAction> {
        self.call(
            "targeting_observe",
            LuaHookContext {
                kind: "trigger".into(),
                line: Some(line.into()),
                raw_line: Some(format!("\x1b[1;36m{line}\x1b[0m")),
                category: Some(OutputCategory::Normal),
                ..Default::default()
            },
        )
    }

    fn input(&mut self, hook: &str, input: &str) -> Vec<LuaAction> {
        self.call(
            hook,
            LuaHookContext {
                kind: "alias".into(),
                input: Some(input.into()),
                ..Default::default()
            },
        )
    }

    fn reset(&mut self) {
        assert!(
            self.call("targeting_reset", LuaHookContext::default())
                .is_empty()
        );
    }
}

fn assert_no_send(actions: &[LuaAction]) {
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, LuaAction::Send(_) | LuaAction::Execute(_)))
    );
}

#[test]
fn numbered_targets_preserve_ansi_and_count_duplicate_keywords() {
    let mut h = Harness::new();
    let line = "A hungry wolf stands here.";
    assert_eq!(
        h.observe(line),
        vec![LuaAction::ReplaceLine(format!(
            "(1) \x1b[1;36m{line}\x1b[0m"
        ))]
    );
    h.observe("A grey wolf stands here.");
    assert_eq!(
        h.input("targeting_action", "k1"),
        vec![LuaAction::Execute("kill 1.wolf".into())]
    );
    assert_eq!(
        h.input("targeting_action", "l2"),
        vec![LuaAction::Execute("exam 2.wolf".into())]
    );
    h.reset();
    assert_no_send(&h.input("targeting_action", "k1"));
    h.observe(line);
    assert_eq!(
        h.input("targeting_action", "k1"),
        vec![LuaAction::Execute("kill 1.wolf".into())]
    );
}

#[test]
fn action_families_and_manual_override_keep_numbered_selection_independent() {
    let mut h = Harness::new();
    h.observe("A wolf stands here.");
    h.input("targeting_manual", "target Vincent");
    for (key, command) in [
        ('k', "kill"),
        ('l', "exam"),
        ('p', "p"),
        ('f', "f"),
        ('c', "c"),
        ('a', "a"),
        ('b', "b"),
        ('q', "q"),
        ('x', "x"),
        ('h', "h"),
        ('i', "i"),
        ('y', "y"),
    ] {
        assert_eq!(
            h.input("targeting_action", &format!("{key}t")),
            vec![LuaAction::Execute(format!("{command} Vincent"))]
        );
        assert_eq!(
            h.input("targeting_action", &format!("{key}1")),
            vec![LuaAction::Execute(format!("{command} 1.wolf"))]
        );
    }
    h.reset();
    assert_eq!(
        h.input("targeting_action", "kt"),
        vec![LuaAction::Execute("kill Vincent".into())]
    );
    h.input("targeting_manual", "target");
    assert_no_send(&h.input("targeting_action", "kt"));
    h.observe("A wolf stands here.");
    assert_eq!(
        h.input("targeting_action", "kt"),
        vec![LuaAction::Execute("kill 1.wolf".into())]
    );
}

#[test]
fn special_descriptions_and_exclusions_resolve_expected_keywords() {
    let mut h = Harness::new();
    for (description, keyword) in [
        (
            "A writhing mass of swamp bugs is buzzing through the air here.",
            "bugs",
        ),
        ("A long, purplish vine lies on the ground.", "vine"),
        (
            "A mordwight hovers here, transluscent and grey.",
            "mordwight",
        ),
        (
            "A strong hunched orc bearing the crest of the Black Tower stands here.",
            "orc",
        ),
        (
            "An olog-hai stands here, growling angrily at its discovery.",
            "olog",
        ),
        (
            "A blood covered troll stands here grimacing in delight.",
            "troll",
        ),
        (
            "A tall, powerful uruk-hai stands here, ordering about his troops.",
            "uruk",
        ),
        (
            "A large, powerful uruk-hai stands here, brandishing his weapon.",
            "uruk",
        ),
        ("A massive redbacked spider fills the cave here.", "spider"),
        (
            "Some poisonous ivy grows here, its greenish-red fronds snaking towards you.",
            "ivy",
        ),
        (
            "The skeletal, wight, from of the lord of Durgurth sits here.",
            "wight",
        ),
        (
            "Karugh the orcish captain is here, keeping his troops in line.",
            "karugh",
        ),
        (
            "Two reptilian eyes observe the surroundings from out of the mud.",
            "lizard",
        ),
        ("Some twisted vines hang from the trees.", "vine"),
        (
            "Dark shadows shift and twist here into different forms. (shadow)",
            "shifting",
        ),
        ("A wolf grazes here.", "wolf"),
        ("A grey-haired man stands here.", "man"),
        ("A tall grey-haired man stands here.", "man"),
    ] {
        h.reset();
        h.observe(description);
        assert_eq!(
            h.input("targeting_action", "k1"),
            vec![LuaAction::Execute(format!("kill 1.{keyword}"))],
            "{description}"
        );
    }
}

#[test]
fn invalid_targets_cannot_enqueue_commands_or_replace_manual_override() {
    let mut h = Harness::new();
    for line in [
        "",
        "!!!",
        "A is stands here.",
        "A wolf;quit stands here.",
        "${target}",
    ] {
        h.reset();
        h.observe(line);
        assert_no_send(&h.input("targeting_action", "k1"));
    }
    for input in [
        "k0",
        "k101",
        "k999999999999999999999999999999999999",
        "k1;quit",
        "kt extra",
    ] {
        assert_no_send(&h.input("targeting_action", input));
    }
    h.input("targeting_manual", "target 2.orc");
    for input in [
        "target orc;quit",
        "target ${bad}",
        "target orc\nquit",
        "target orc captain",
        "target 0.orc",
        "target 101.orc",
        "target /quit",
    ] {
        assert_no_send(&h.input("targeting_manual", input));
        assert_eq!(
            h.input("targeting_action", "kt"),
            vec![LuaAction::Execute("kill 2.orc".into())]
        );
    }
}

#[test]
fn configured_color_and_spell_triggers_select_only_expected_hooks() {
    let config: AppConfig = toml::from_str(include_str!("../config.toml")).unwrap();
    let mut engine = TriggerEngine::new(&config.triggers).unwrap();
    for (color, expected) in [
        (AnsiColor::BrightCyan, "targeting_observe"),
        (AnsiColor::BrightYellow, "targeting_reset"),
    ] {
        let result = engine.evaluate_with_colors(
            "A wolf stands here.",
            OutputCategory::Normal,
            &AnsiColors {
                foregrounds: vec![color],
                backgrounds: vec![],
            },
        );
        assert_eq!(
            result
                .lua
                .iter()
                .map(|hook| hook.function.as_str())
                .collect::<Vec<_>>(),
            vec![expected]
        );
    }
    assert!(
        engine
            .evaluate("A wolf stands here.", OutputCategory::Normal)
            .lua
            .is_empty()
    );
    for line in [
        "Your mind probes the area seeking other souls.",
        "A surge of light reveals to you every corner of the room.",
    ] {
        let result = engine.evaluate(line, OutputCategory::Normal);
        assert_eq!(result.lua.len(), 1);
        assert_eq!(result.lua[0].function, "targeting_reset");
    }
}

#[test]
fn sightings_are_bounded_and_listing_fits_the_action_budget() {
    let mut h = Harness::new();
    for _ in 0..100 {
        h.observe("A wolf stands here.");
    }
    assert_eq!(
        h.input("targeting_action", "k100"),
        vec![LuaAction::Execute("kill 100.wolf".into())]
    );
    assert!(matches!(
        h.observe("A wolf stands here.").as_slice(),
        [LuaAction::Echo(_, _)]
    ));
    assert!(h.observe("A wolf stands here.").is_empty());
    let listed = h.input("targeting_list", "vt");
    assert!(
        matches!(listed.as_slice(), [LuaAction::Echo(text, _)] if text.contains("100: 100.wolf") && !text.contains("101:"))
    );
}

#[test]
fn bundled_configuration_enables_targeting_without_replacing_existing_hooks() {
    let config: AppConfig = toml::from_str(include_str!("../config.toml")).unwrap();
    config.validate().unwrap();
    assert_eq!(
        config.lua.scripts,
        Some(vec!["init.lua".into(), "targeting.lua".into()])
    );
    for name in [
        "smart-kill",
        "run-path",
        "targeting-actions",
        "targeting-list",
        "targeting-manual",
    ] {
        assert!(config.aliases.rules.iter().any(|rule| rule.name == name));
    }
    for (name, color) in [
        ("targeting-room-header", "lightyellow"),
        ("targeting-mobile", "lightcyan"),
    ] {
        let rule = config
            .triggers
            .rules
            .iter()
            .find(|rule| rule.name == name)
            .unwrap();
        assert_eq!(rule.foreground.as_deref(), Some(color));
    }
    for name in ["targeting-word-of-sight", "targeting-reveal"] {
        assert!(config.triggers.rules.iter().any(|rule| rule.name == name));
    }
}
