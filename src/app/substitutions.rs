use super::*;
use crate::config::SubstitutionRuleConfig;

impl App {
    pub(super) fn handle_substitution_command(
        &mut self,
        input: &str,
    ) -> std::result::Result<String, String> {
        let input = input.trim();
        if input.is_empty() {
            let mut output = String::from("# Substitutions\n");
            for (rule, runtime) in self.substitutions.substitution_entries() {
                output.push_str(&format!(
                    "\n- `{}` → `{}` ({:?}, {})",
                    markdown_inline(&rule.pattern),
                    markdown_inline(&rule.replacement),
                    rule.match_type,
                    if runtime { "runtime" } else { "configured" }
                ));
            }
            return Ok(output);
        }
        if input.eq_ignore_ascii_case("clear") {
            return Ok(format!(
                "Removed {} runtime substitutions.",
                self.substitutions.clear_runtime_substitutions()
            ));
        }
        if let Some(rest) = strip_subcommand(input, "unset") {
            let fields = parse_braced_fields(rest, "substitute unset")?;
            if fields.len() != 1 {
                return Err("usage: /substitute unset {pattern}".into());
            }
            return if self.substitutions.remove_runtime_substitution(&fields[0]) {
                Ok("Removed runtime substitution.".into())
            } else {
                Err("Runtime substitution not found.".into())
            };
        }
        let (rest, match_type) = if let Some(rest) = strip_subcommand(input, "regex") {
            (rest, MatchType::Regex)
        } else {
            (
                strip_subcommand(input, "plain").unwrap_or(input),
                MatchType::Plain,
            )
        };
        let fields = local_commands::parse_definition_fields(rest, "substitute")?;
        if fields.len() != 2 {
            return Err("usage: /substitute [plain|regex] {pattern} {replacement}".into());
        }
        self.substitutions
            .add_runtime_substitution(SubstitutionRuleConfig {
                name: format!("runtime:{}", fields[0]),
                pattern: fields[0].clone(),
                replacement: fields[1].clone(),
                match_type,
                ..SubstitutionRuleConfig::default()
            })
            .map_err(|error| error.to_string())?;
        Ok("Substitution set. Use /save to persist it.".into())
    }
}
