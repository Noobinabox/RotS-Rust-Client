use std::collections::{HashMap, HashSet, VecDeque};

use crate::{
    config::{AppConfig, MsdpMapping},
    map::{MapState, parse_room_exit_updates, parse_room_exits, parse_room_id},
    network::msdp::{MsdpFrame, MsdpValue},
    path::PathState,
};

pub const OUTPUT_STATUS_VISIBLE_TICKS: u8 = 30;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputCategory {
    Normal,
    Snapshot,
    Combat,
    Communication,
    System,
    Error,
    Prompt,
    Triggered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputDisplayMode {
    Styled,
    Plain,
    Debug,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputView {
    /// Complete logical lines hidden below the viewport.
    pub scroll_offset: usize,
    /// Wrapped rows hidden at the bottom of the last included logical line.
    pub wrapped_row_offset: usize,
    pub follow_newest: bool,
    pub display_mode: OutputDisplayMode,
    pub status_ticks_remaining: u8,
    pub search_query: String,
    pub search_input: String,
    pub search_cursor: usize,
    pub search_active: bool,
    pub search_matches: Vec<usize>,
    pub active_match: Option<usize>,
}

impl Default for OutputView {
    fn default() -> Self {
        Self {
            scroll_offset: 0,
            wrapped_row_offset: 0,
            follow_newest: true,
            display_mode: OutputDisplayMode::Styled,
            status_ticks_remaining: 0,
            search_query: String::new(),
            search_input: String::new(),
            search_cursor: 0,
            search_active: false,
            search_matches: Vec::new(),
            active_match: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputLine {
    pub raw: String,
    pub normalized: String,
    pub category: OutputCategory,
    pub style: Option<OutputStyle>,
    pub starts_new_output: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OutputStyle {
    pub foreground: Option<String>,
    pub background: Option<String>,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub reverse: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptEventRecord {
    pub name: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InputCompletion {
    pub prefix: String,
    pub start: usize,
    pub end: usize,
    pub matches: Vec<String>,
    pub selected: usize,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CharacterState {
    pub name: Option<String>,
    pub race: Option<String>,
    pub level: Option<i64>,
    pub health: Option<i64>,
    pub health_max: Option<i64>,
    pub mana: Option<i64>,
    pub mana_max: Option<i64>,
    pub movement: Option<i64>,
    pub movement_max: Option<i64>,
    pub experience: Option<i64>,
    pub experience_max: Option<i64>,
    pub offensive_bonus: Option<i64>,
    pub dodge: Option<i64>,
    pub parry: Option<i64>,
    pub attack_speed: Option<i64>,
    pub strength: Option<i64>,
    pub intelligence: Option<i64>,
    pub will: Option<i64>,
    pub dexterity: Option<i64>,
    pub constitution: Option<i64>,
    pub learning: Option<i64>,
    pub willpower: Option<i64>,
    pub spell_save: Option<i64>,
    pub spirit: Option<i64>,
    pub spell_power: Option<i64>,
    pub spell_pen: Option<i64>,
    pub warrior_level: Option<i64>,
    pub ranger_level: Option<i64>,
    pub mystic_level: Option<i64>,
    pub mage_level: Option<i64>,
    pub health_regeneration: Option<i64>,
    pub stamina_regeneration: Option<i64>,
    pub movement_regeneration: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OpponentState {
    pub name: Option<String>,
    pub level: Option<String>,
    pub health: Option<i64>,
    pub health_max: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GroupState {
    pub members: Vec<GroupMember>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GroupMember {
    pub name: String,
    pub health_percent: Option<i64>,
    pub mana_percent: Option<i64>,
    pub movement_percent: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorldState {
    pub time: Option<String>,
    pub weather: Option<String>,
    pub weather_frame: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocialChannel {
    Tells,
    Chats,
    Says,
    Narrates,
    Group,
    Yells,
    Sings,
}

impl SocialChannel {
    pub fn label(self) -> &'static str {
        match self {
            Self::Tells => "tell",
            Self::Chats => "chat",
            Self::Says => "say",
            Self::Narrates => "nar",
            Self::Group => "grp",
            Self::Yells => "yell",
            Self::Sings => "sing",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocialMessage {
    pub channel: SocialChannel,
    pub timestamp: String,
    pub prefix: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocialState {
    pub messages: VecDeque<SocialMessage>,
    pub scroll_offset: usize,
    limit: usize,
}

impl SocialState {
    fn new(limit: usize) -> Self {
        Self {
            messages: VecDeque::with_capacity(limit.min(256)),
            scroll_offset: 0,
            limit: limit.max(1),
        }
    }

    pub fn push(
        &mut self,
        channel: SocialChannel,
        timestamp: impl Into<String>,
        prefix: impl Into<String>,
        text: impl Into<String>,
    ) {
        self.messages.push_back(SocialMessage {
            channel,
            timestamp: timestamp.into(),
            prefix: prefix.into(),
            text: text.into(),
        });
        while self.messages.len() > self.limit {
            self.messages.pop_front();
            if self.scroll_offset > 0 {
                self.scroll_offset -= 1;
            }
        }
    }

    pub fn scroll_up_to(&mut self, amount: usize, max_offset: usize) {
        self.scroll_offset = self.scroll_offset.saturating_add(amount).min(max_offset);
    }

    pub fn scroll_down(&mut self, amount: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub input_mode: crate::config::InputMode,
    pub vim: crate::input::vim::VimEditor,
    pub connection: ConnectionStatus,
    pub output: VecDeque<OutputLine>,
    pub input: String,
    pub cursor: usize,
    pub output_view: OutputView,
    pub submitted_input: Option<String>,
    pub submitted_input_selected: bool,
    pub command_history: Vec<String>,
    pub history_position: Option<usize>,
    pub history_draft: Option<String>,
    pub input_completion: InputCompletion,
    pub character: CharacterState,
    pub opponent: OpponentState,
    pub group: GroupState,
    pub world: WorldState,
    pub social: SocialState,
    pub map: MapState,
    pub path: PathState,
    pub raw_msdp: HashMap<String, MsdpValue>,
    pub script_events: Vec<ScriptEventRecord>,
    pub last_error: Option<String>,
    scrollback_limit: usize,
}

impl AppState {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            input_mode: config.terminal.input_mode,
            vim: crate::input::vim::VimEditor::default(),
            connection: ConnectionStatus::Disconnected,
            output: VecDeque::with_capacity(config.layout.scrollback_lines.min(1024)),
            input: String::new(),
            cursor: 0,
            output_view: OutputView::default(),
            submitted_input: None,
            submitted_input_selected: false,
            command_history: Vec::new(),
            history_position: None,
            history_draft: None,
            input_completion: InputCompletion::default(),
            character: CharacterState::default(),
            opponent: OpponentState::default(),
            group: GroupState::default(),
            world: WorldState::default(),
            social: SocialState::new(config.social.scrollback_lines),
            map: MapState::default(),
            path: PathState::default(),
            raw_msdp: HashMap::new(),
            script_events: Vec::new(),
            last_error: None,
            scrollback_limit: config.layout.scrollback_lines,
        }
    }

    pub fn push_output(&mut self, raw: impl Into<String>, category: OutputCategory) {
        self.push_output_styled(raw, category, None);
    }

    pub fn push_output_boundary(&mut self, raw: impl Into<String>, category: OutputCategory) {
        self.push_output(raw, category);
        if let Some(line) = self.output.back_mut() {
            line.starts_new_output = true;
        }
    }

    pub fn push_output_styled(
        &mut self,
        raw: impl Into<String>,
        category: OutputCategory,
        style: Option<OutputStyle>,
    ) {
        self.push_output_styled_internal(raw.into(), category, style, true);
    }

    pub fn push_local_output_styled(
        &mut self,
        raw: impl Into<String>,
        category: OutputCategory,
        style: Option<OutputStyle>,
    ) {
        self.push_output_styled_internal(raw.into(), category, style, false);
    }

    fn push_output_styled_internal(
        &mut self,
        raw: String,
        category: OutputCategory,
        style: Option<OutputStyle>,
        coalesce_casting_spinner: bool,
    ) {
        let normalized = normalize_text(&raw);
        if coalesce_casting_spinner
            && category == OutputCategory::Normal
            && is_casting_spinner_line(&normalized)
        {
            if self.replace_casting_spinner(&raw, &normalized, style.clone()) {
                return;
            }
            self.output.push_back(OutputLine {
                raw,
                normalized,
                category: OutputCategory::Prompt,
                style,
                starts_new_output: false,
            });
            while self.output.len() > self.scrollback_limit {
                self.output.pop_front();
                if self.output_view.scroll_offset > 0 {
                    self.output_view.scroll_offset -= 1;
                }
            }
            self.after_output_changed(true);
            return;
        }
        let starts_new_output = category == OutputCategory::Normal
            && matches!(
                self.output.back().map(|line| &line.category),
                Some(OutputCategory::Prompt)
            );
        if starts_new_output {
            self.output.pop_back();
        }
        self.output.push_back(OutputLine {
            raw,
            normalized,
            category,
            style,
            starts_new_output,
        });
        while self.output.len() > self.scrollback_limit {
            self.output.pop_front();
            if self.output_view.scroll_offset > 0 {
                self.output_view.scroll_offset -= 1;
            }
        }
        self.after_output_changed(true);
    }

    pub fn push_output_snapshot(&mut self, lines: Vec<String>) {
        if lines.is_empty() {
            return;
        }
        let starts_new_output = matches!(
            self.output.back().map(|line| &line.category),
            Some(OutputCategory::Prompt)
        );
        if starts_new_output {
            self.output.pop_back();
        }
        let snapshot_height = lines.len();
        for (index, raw) in lines.into_iter().enumerate() {
            let normalized = normalize_text(&raw);
            self.output.push_back(OutputLine {
                raw,
                normalized,
                category: OutputCategory::Snapshot,
                style: None,
                starts_new_output: starts_new_output && index == 0,
            });
        }
        let retained = self.scrollback_limit.max(snapshot_height);
        while self.output.len() > retained {
            self.output.pop_front();
            if self.output_view.scroll_offset > 0 {
                self.output_view.scroll_offset -= 1;
            }
        }
        self.after_output_changed(true);
    }

    fn replace_casting_spinner(
        &mut self,
        raw: &str,
        normalized: &str,
        style: Option<OutputStyle>,
    ) -> bool {
        if let Some(line) = self
            .output
            .back_mut()
            .filter(|line| line.category == OutputCategory::Prompt)
        {
            line.raw = raw.to_string();
            line.normalized = normalized.to_string();
            line.category = OutputCategory::Prompt;
            line.style = style;
            line.starts_new_output = false;
            self.after_output_changed(false);
            return true;
        }
        if let Some(previous) = self
            .output
            .iter_mut()
            .rev()
            .take(2)
            .find(|line| line.category == OutputCategory::Prompt)
        {
            previous.raw = raw.to_string();
            previous.normalized = normalized.to_string();
            previous.category = OutputCategory::Prompt;
            previous.style = style;
            previous.starts_new_output = false;
            self.after_output_changed(false);
            return true;
        }
        false
    }

    pub fn clear_output(&mut self) {
        self.output.clear();
        self.follow_output();
        self.refresh_search_matches();
    }

    pub fn set_scrollback_limit(&mut self, limit: usize) {
        self.scrollback_limit = limit.max(1);
        while self.output.len() > self.scrollback_limit {
            self.output.pop_front();
            if self.output_view.scroll_offset > 0 {
                self.output_view.scroll_offset -= 1;
            }
        }
        self.refresh_search_matches();
    }

    pub fn push_prompt(&mut self, raw: impl Into<String>) {
        self.push_prompt_styled(raw, None);
    }

    pub fn push_prompt_styled(&mut self, raw: impl Into<String>, style: Option<OutputStyle>) {
        let raw = raw.into();
        if raw.is_empty() {
            return;
        }
        let normalized = normalize_text(&raw);
        if let Some(line) = self.output.back_mut()
            && line.category == OutputCategory::Prompt
        {
            line.raw = raw;
            line.normalized = normalized;
            line.style = style;
            self.after_output_changed(false);
            return;
        }
        self.output.push_back(OutputLine {
            raw,
            normalized,
            category: OutputCategory::Prompt,
            style,
            starts_new_output: false,
        });
        while self.output.len() > self.scrollback_limit {
            self.output.pop_front();
            if self.output_view.scroll_offset > 0 {
                self.output_view.scroll_offset -= 1;
            }
        }
        self.after_output_changed(true);
    }

    pub fn apply_msdp_frames(&mut self, frames: &[MsdpFrame], mapping: &MsdpMapping) {
        let mut room_table = None;
        for frame in frames {
            self.raw_msdp
                .insert(frame.variable.clone(), frame.value.clone());
            if frame.variable == mapping.room {
                room_table = Some(frame.value.clone());
            }
        }
        for frame in frames {
            self.apply_msdp_frame(frame, mapping);
        }
        if let Some(room_table) = room_table {
            self.sync_msdp_room_from_table(mapping, &room_table);
        }
    }

    pub fn has_authoritative_msdp_room(&self, mapping: &MsdpMapping) -> bool {
        self.raw_msdp.contains_key(&mapping.room) || self.raw_msdp.contains_key(&mapping.room_vnum)
    }

    pub fn clear_authoritative_msdp_room(&mut self, mapping: &MsdpMapping) {
        self.raw_msdp.remove(&mapping.room);
        self.raw_msdp.remove(&mapping.room_vnum);
        self.raw_msdp.remove(&mapping.room_name);
        self.raw_msdp.remove(&mapping.room_exits);
    }

    fn apply_msdp_frame(&mut self, frame: &MsdpFrame, mapping: &MsdpMapping) {
        let variable = frame.variable.as_str();
        if variable == mapping.character_name {
            self.character.name = frame.value.as_string().map(ToOwned::to_owned);
        } else if variable == mapping.race {
            self.character.race = frame.value.as_string().map(ToOwned::to_owned);
        } else if variable == mapping.level {
            self.character.level = frame.value.as_i64();
        } else if variable == mapping.health {
            self.character.health = frame.value.as_i64();
        } else if variable == mapping.health_max {
            self.character.health_max = frame.value.as_i64();
        } else if variable == mapping.mana {
            self.character.mana = frame.value.as_i64();
        } else if variable == mapping.mana_max {
            self.character.mana_max = frame.value.as_i64();
        } else if variable == mapping.movement {
            self.character.movement = frame.value.as_i64();
        } else if variable == mapping.movement_max {
            self.character.movement_max = frame.value.as_i64();
        } else if variable == mapping.experience {
            self.character.experience = frame.value.as_i64();
        } else if variable == mapping.experience_max {
            self.character.experience_max = frame.value.as_i64();
        } else if variable == mapping.offensive_bonus {
            self.character.offensive_bonus = frame.value.as_i64();
        } else if variable == mapping.dodge {
            self.character.dodge = frame.value.as_i64();
        } else if variable == mapping.parry {
            self.character.parry = frame.value.as_i64();
        } else if variable == mapping.attack_speed {
            self.character.attack_speed = frame.value.as_i64();
        } else if variable == mapping.strength {
            self.character.strength = frame.value.as_i64();
        } else if variable == mapping.intelligence {
            self.character.intelligence = frame.value.as_i64();
        } else if variable == mapping.will {
            self.character.will = frame.value.as_i64();
        } else if variable == mapping.dexterity {
            self.character.dexterity = frame.value.as_i64();
        } else if variable == mapping.constitution {
            self.character.constitution = frame.value.as_i64();
        } else if variable == mapping.learning {
            self.character.learning = frame.value.as_i64();
        } else if variable == mapping.willpower {
            self.character.willpower = frame.value.as_i64();
        } else if variable == mapping.spell_save {
            self.character.spell_save = frame.value.as_i64();
        } else if variable == mapping.spirit {
            self.character.spirit = frame.value.as_i64();
        } else if variable == mapping.spell_power {
            self.character.spell_power = frame.value.as_i64();
        } else if variable == mapping.spell_pen {
            self.character.spell_pen = frame.value.as_i64();
        } else if variable == mapping.warrior_level {
            self.character.warrior_level = frame.value.as_i64();
        } else if variable == mapping.ranger_level {
            self.character.ranger_level = frame.value.as_i64();
        } else if variable == mapping.mystic_level {
            self.character.mystic_level = frame.value.as_i64();
        } else if variable == mapping.mage_level {
            self.character.mage_level = frame.value.as_i64();
        } else if variable == mapping.health_regeneration {
            self.character.health_regeneration = frame.value.as_i64();
        } else if variable == mapping.stamina_regeneration {
            self.character.stamina_regeneration = frame.value.as_i64();
        } else if variable == mapping.movement_regeneration {
            self.character.movement_regeneration = frame.value.as_i64();
        } else if variable == mapping.opponent_name {
            self.opponent.name = non_empty_string(&frame.value);
        } else if variable == mapping.opponent_level {
            self.opponent.level = non_empty_string(&frame.value);
        } else if variable == mapping.opponent_health {
            self.opponent.health = frame.value.as_i64();
        } else if variable == mapping.opponent_health_max {
            self.opponent.health_max = frame.value.as_i64();
        } else if variable == mapping.group {
            self.group = parse_group(&frame.value, mapping);
        } else if variable == mapping.world_time {
            self.world.time = non_empty_string(&frame.value);
        } else if variable == mapping.weather {
            self.world.weather = non_empty_string(&frame.value);
        }
    }

    fn sync_msdp_room_from_table(&mut self, mapping: &MsdpMapping, room_value: &MsdpValue) {
        let room_table = room_value.as_table();
        let room_field = |key: &str| room_table.and_then(|table| table.get(key));

        let id = room_field("VNUM").and_then(parse_room_id);
        let name = room_field("NAME")
            .and_then(MsdpValue::as_string)
            .map(ToOwned::to_owned);
        let terrain = room_field("TERRAIN")
            .and_then(MsdpValue::as_string)
            .map(ToOwned::to_owned);
        let directions = self
            .raw_msdp
            .get(&mapping.room_exits)
            .map(parse_room_exits)
            .unwrap_or_default();
        let exits = parse_room_exit_updates(directions, room_field("EXITS"));

        self.map.sync_room(id, name, exits, true);
        self.map.sync_current_room_terrain(terrain);
    }

    pub fn scroll_output_up(&mut self, amount: usize) {
        self.output_view.wrapped_row_offset = 0;
        let max_offset = self.output.len().saturating_sub(1);
        self.output_view.scroll_offset = self
            .output_view
            .scroll_offset
            .saturating_add(amount)
            .min(max_offset);
        self.output_view.follow_newest = self.output_view.scroll_offset == 0;
    }

    pub fn scroll_output_down(&mut self, amount: usize) {
        self.output_view.wrapped_row_offset = 0;
        self.output_view.scroll_offset = self.output_view.scroll_offset.saturating_sub(amount);
        self.output_view.follow_newest = self.output_view.scroll_offset == 0;
    }

    pub fn follow_output(&mut self) {
        self.output_view.scroll_offset = 0;
        self.output_view.wrapped_row_offset = 0;
        self.output_view.follow_newest = true;
    }

    pub fn toggle_output_display_mode(&mut self) {
        self.output_view.display_mode = match self.output_view.display_mode {
            OutputDisplayMode::Styled => OutputDisplayMode::Plain,
            OutputDisplayMode::Plain => OutputDisplayMode::Debug,
            OutputDisplayMode::Debug => OutputDisplayMode::Styled,
        };
        self.output_view.status_ticks_remaining = OUTPUT_STATUS_VISIBLE_TICKS;
    }

    pub fn tick_output_status(&mut self) {
        self.output_view.status_ticks_remaining =
            self.output_view.status_ticks_remaining.saturating_sub(1);
    }

    pub fn complete_input(&mut self, reverse: bool) {
        if self.submitted_input_selected {
            self.input = self.submitted_input.clone().unwrap_or_default();
            self.cursor = self.input.len();
            self.submitted_input_selected = false;
        }
        let active_token = self.active_completion_token();
        let Some((start, end, prefix)) =
            active_token.or_else(|| input_token(&self.input, self.cursor))
        else {
            self.clear_input_completion();
            return;
        };
        let matches = completion_matches(&self.output, &prefix);
        if matches.is_empty() {
            self.clear_input_completion();
            return;
        }
        let mut selected = if self.input_completion.active
            && self.input_completion.prefix.eq_ignore_ascii_case(&prefix)
            && self.input_completion.start == start
            && self.input_completion.end == end
            && self.input_completion.matches == matches
        {
            self.input_completion.selected
        } else {
            0
        };
        if matches.len() > 1 {
            selected = if reverse {
                selected.checked_sub(1).unwrap_or(matches.len() - 1)
            } else if self.input_completion.active {
                (selected + 1) % matches.len()
            } else {
                selected
            };
        }
        let completion = matches[selected].clone();
        self.input.replace_range(start..end, &completion);
        self.cursor = start + completion.len();
        self.input_completion = InputCompletion {
            prefix,
            start,
            end: start + completion.len(),
            matches,
            selected,
            active: true,
        };
    }

    fn active_completion_token(&self) -> Option<(usize, usize, String)> {
        let completion = &self.input_completion;
        if !completion.active || self.cursor != completion.end {
            return None;
        }
        let selected = completion.matches.get(completion.selected)?;
        let current = self.input.get(completion.start..completion.end)?;
        if current == selected {
            Some((completion.start, completion.end, completion.prefix.clone()))
        } else {
            None
        }
    }

    pub fn clear_input_completion(&mut self) {
        self.input_completion = InputCompletion::default();
    }

    pub fn start_output_search(&mut self) {
        self.output_view.search_input = self.output_view.search_query.clone();
        self.output_view.search_cursor = self.output_view.search_input.len();
        self.output_view.search_active = true;
    }

    pub fn cancel_output_search(&mut self) {
        self.output_view.search_input.clear();
        self.output_view.search_cursor = 0;
        self.output_view.search_active = false;
    }

    pub fn commit_output_search(&mut self) {
        self.output_view.search_query = self.output_view.search_input.trim().to_string();
        self.output_view.search_active = false;
        self.refresh_search_matches();
        self.jump_to_active_search_match();
    }

    pub fn move_search_match(&mut self, direction: SearchDirection) {
        if self.output_view.search_matches.is_empty() {
            return;
        }
        let current = self.output_view.active_match.unwrap_or(0);
        self.output_view.active_match = Some(match direction {
            SearchDirection::Previous => current.saturating_sub(1),
            SearchDirection::Next if current + 1 < self.output_view.search_matches.len() => {
                current + 1
            }
            SearchDirection::Next => current,
        });
        self.jump_to_active_search_match();
    }

    pub fn edit_search_input(&mut self, action: SearchEdit) {
        match action {
            SearchEdit::Insert(value) => {
                self.output_view
                    .search_input
                    .insert(self.output_view.search_cursor, value);
                self.output_view.search_cursor += value.len_utf8();
            }
            SearchEdit::Backspace => {
                if self.output_view.search_cursor > 0 {
                    self.output_view.search_cursor = previous_boundary(
                        &self.output_view.search_input,
                        self.output_view.search_cursor,
                    );
                    self.output_view
                        .search_input
                        .remove(self.output_view.search_cursor);
                }
            }
            SearchEdit::Delete => {
                if self.output_view.search_cursor < self.output_view.search_input.len() {
                    self.output_view
                        .search_input
                        .remove(self.output_view.search_cursor);
                }
            }
            SearchEdit::Left => {
                self.output_view.search_cursor = previous_boundary(
                    &self.output_view.search_input,
                    self.output_view.search_cursor,
                );
            }
            SearchEdit::Right => {
                self.output_view.search_cursor = next_boundary(
                    &self.output_view.search_input,
                    self.output_view.search_cursor,
                );
            }
            SearchEdit::Home => self.output_view.search_cursor = 0,
            SearchEdit::End => self.output_view.search_cursor = self.output_view.search_input.len(),
        }
    }

    fn after_output_changed(&mut self, appended_line: bool) {
        if self.output_view.follow_newest {
            self.output_view.scroll_offset = 0;
            self.output_view.wrapped_row_offset = 0;
        } else if appended_line {
            self.output_view.scroll_offset = self
                .output_view
                .scroll_offset
                .saturating_add(1)
                .min(self.output.len().saturating_sub(1));
        }
        self.refresh_search_matches();
    }

    fn refresh_search_matches(&mut self) {
        self.output_view.search_matches.clear();
        self.output_view.active_match = None;
        if self.output_view.search_query.is_empty() {
            return;
        }
        let query = self.output_view.search_query.to_ascii_lowercase();
        for (index, line) in self.output.iter().enumerate() {
            if line.category == OutputCategory::Snapshot {
                continue;
            }
            if plain_text(&line.normalized)
                .to_ascii_lowercase()
                .contains(&query)
            {
                self.output_view.search_matches.push(index);
            }
        }
        if !self.output_view.search_matches.is_empty() {
            self.output_view.active_match = Some(self.output_view.search_matches.len() - 1);
        }
    }

    fn jump_to_active_search_match(&mut self) {
        let Some(active_match) = self.output_view.active_match else {
            return;
        };
        let Some(&line_index) = self.output_view.search_matches.get(active_match) else {
            return;
        };
        self.output_view.scroll_offset = self.output.len().saturating_sub(line_index + 1);
        self.output_view.wrapped_row_offset = 0;
        self.output_view.follow_newest = self.output_view.scroll_offset == 0;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchDirection {
    Previous,
    Next,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchEdit {
    Insert(char),
    Backspace,
    Delete,
    Left,
    Right,
    Home,
    End,
}

fn normalize_text(raw: &str) -> String {
    raw.replace("\r\n", "\n").replace('\r', "\n")
}

pub(crate) fn is_casting_spinner_line(value: &str) -> bool {
    matches!(plain_text(value).trim(), "-" | "\\" | "|" | "/")
}

pub fn plain_text(raw: &str) -> String {
    let mut plain = String::new();
    let mut chars = raw.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            for next in chars.by_ref() {
                if ('@'..='~').contains(&next) {
                    break;
                }
            }
        } else {
            plain.push(ch);
        }
    }
    plain
}

fn previous_boundary(input: &str, cursor: usize) -> usize {
    input
        .char_indices()
        .map(|(index, _)| index)
        .take_while(|index| *index < cursor)
        .last()
        .unwrap_or(0)
}

fn next_boundary(input: &str, cursor: usize) -> usize {
    input
        .char_indices()
        .map(|(index, _)| index)
        .find(|index| *index > cursor)
        .unwrap_or(input.len())
}

fn input_token(input: &str, cursor: usize) -> Option<(usize, usize, String)> {
    if cursor > input.len() || !input.is_char_boundary(cursor) {
        return None;
    }
    let start = input[..cursor]
        .char_indices()
        .rev()
        .find(|(_, value)| value.is_whitespace())
        .map(|(index, value)| index + value.len_utf8())
        .unwrap_or(0);
    let end = input[cursor..]
        .char_indices()
        .find(|(_, value)| value.is_whitespace())
        .map(|(index, _)| cursor + index)
        .unwrap_or(input.len());
    let prefix = input[start..cursor].to_string();
    if prefix.trim().is_empty() {
        None
    } else {
        Some((start, end, prefix))
    }
}

fn completion_matches(output: &VecDeque<OutputLine>, prefix: &str) -> Vec<String> {
    let prefix = prefix.to_ascii_lowercase();
    let mut seen = HashSet::new();
    let mut matches = Vec::new();
    for line in output.iter().rev().filter(|line| {
        matches!(
            line.category,
            OutputCategory::Normal
                | OutputCategory::Combat
                | OutputCategory::Communication
                | OutputCategory::Triggered
        )
    }) {
        for word in words(&plain_text(&line.normalized)) {
            let key = word.to_ascii_lowercase();
            if key.starts_with(&prefix) && key != prefix && seen.insert(key) {
                matches.push(word);
                if matches.len() >= 12 {
                    return matches;
                }
            }
        }
    }
    matches
}

fn words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    for value in text.chars() {
        if value.is_alphanumeric() || matches!(value, '\'' | '-') {
            current.push(value);
        } else if !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

fn non_empty_string(value: &MsdpValue) -> Option<String> {
    value
        .as_string()
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn parse_group(value: &MsdpValue, mapping: &MsdpMapping) -> GroupState {
    let mut state = GroupState::default();
    let Some(table) = value.as_table() else {
        return state;
    };
    let Some(MsdpValue::Array(members)) = table.get(&mapping.group_members) else {
        return state;
    };

    for member in members {
        let Some(member_table) = member.as_table() else {
            continue;
        };
        let name = member_table
            .get(&mapping.group_member_name)
            .and_then(MsdpValue::as_string)
            .unwrap_or_default()
            .to_string();
        if name.is_empty() {
            continue;
        }
        state.members.push(GroupMember {
            name,
            health_percent: member_table
                .get(&mapping.group_member_health)
                .and_then(MsdpValue::as_i64),
            mana_percent: member_table
                .get(&mapping.group_member_mana)
                .and_then(MsdpValue::as_i64),
            movement_percent: member_table
                .get(&mapping.group_member_movement)
                .and_then(MsdpValue::as_i64),
        });
    }

    state
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::network::msdp::{
        MSDP_ARRAY_CLOSE, MSDP_ARRAY_OPEN, MSDP_TABLE_CLOSE, MSDP_TABLE_OPEN, MSDP_VAL, MSDP_VAR,
        parse_msdp_payload,
    };

    use super::*;

    #[test]
    fn output_respects_scrollback_limit() {
        let mut config = AppConfig::default();
        config.layout.scrollback_lines = 2;
        let mut state = AppState::new(&config);

        state.push_output("one", OutputCategory::Normal);
        state.push_output("two", OutputCategory::Normal);
        state.push_output("three", OutputCategory::Normal);

        assert_eq!(state.output.len(), 2);
        assert_eq!(
            state.output.front().map(|line| line.raw.as_str()),
            Some("two")
        );
    }

    #[test]
    fn output_scroll_controls_follow_mode() {
        let mut state = AppState::new(&AppConfig::default());
        state.push_output("one", OutputCategory::Normal);
        state.push_output("two", OutputCategory::Normal);
        state.push_output("three", OutputCategory::Normal);

        state.scroll_output_up(2);
        assert_eq!(state.output_view.scroll_offset, 2);
        assert!(!state.output_view.follow_newest);

        state.scroll_output_down(1);
        assert_eq!(state.output_view.scroll_offset, 1);
        assert!(!state.output_view.follow_newest);

        state.follow_output();
        assert_eq!(state.output_view.scroll_offset, 0);
        assert!(state.output_view.follow_newest);
    }

    #[test]
    fn replacing_live_prompt_does_not_shift_scrolled_view() {
        let mut state = AppState::new(&AppConfig::default());
        state.push_output("one", OutputCategory::Normal);
        state.push_output("two", OutputCategory::Normal);
        state.push_prompt("Gram>");
        state.scroll_output_up(1);

        state.push_prompt("Gram> ");

        assert_eq!(state.output_view.scroll_offset, 1);
        assert!(!state.output_view.follow_newest);
    }

    #[test]
    fn casting_spinner_frames_replace_each_other() {
        let mut state = AppState::new(&AppConfig::default());

        state.push_output("You start to concentrate.", OutputCategory::Normal);
        state.push_output("-", OutputCategory::Normal);
        state.push_output("\\", OutputCategory::Normal);
        state.push_output("|", OutputCategory::Normal);
        state.push_output("/", OutputCategory::Normal);

        assert_eq!(state.output.len(), 2);
        assert_eq!(state.output.back().map(|line| line.raw.as_str()), Some("/"));
        assert_eq!(
            state.output.back().map(|line| &line.category),
            Some(&OutputCategory::Prompt)
        );
    }

    #[test]
    fn normal_output_replaces_casting_spinner_frame() {
        let mut state = AppState::new(&AppConfig::default());

        state.push_output("You start to concentrate.", OutputCategory::Normal);
        state.push_output("-", OutputCategory::Normal);
        state.push_output("Ok.", OutputCategory::Normal);

        assert_eq!(state.output.len(), 2);
        assert_eq!(
            state.output.back().map(|line| line.raw.as_str()),
            Some("Ok.")
        );
        assert_eq!(
            state.output.back().map(|line| &line.category),
            Some(&OutputCategory::Normal)
        );
    }

    #[test]
    fn output_clear_resets_view_state() {
        let mut state = AppState::new(&AppConfig::default());
        state.push_output("one", OutputCategory::Normal);
        state.scroll_output_up(1);
        state.output_view.search_query = "one".to_string();
        state.commit_output_search();

        state.clear_output();

        assert!(state.output.is_empty());
        assert_eq!(state.output_view.scroll_offset, 0);
        assert!(state.output_view.follow_newest);
        assert!(state.output_view.search_matches.is_empty());
    }

    #[test]
    fn output_search_tracks_matches_and_jump_position() {
        let mut state = AppState::new(&AppConfig::default());
        state.push_output("alpha", OutputCategory::Normal);
        state.push_output("\x1b[32mbear\x1b[0m", OutputCategory::Normal);
        state.push_output("pony", OutputCategory::Normal);
        state.push_output("bear", OutputCategory::Normal);
        state.start_output_search();
        for value in "bear".chars() {
            state.edit_search_input(SearchEdit::Insert(value));
        }

        state.commit_output_search();

        assert_eq!(state.output_view.search_matches, vec![1, 3]);
        assert_eq!(state.output_view.active_match, Some(1));
        assert_eq!(state.output_view.scroll_offset, 0);

        state.move_search_match(SearchDirection::Previous);
        assert_eq!(state.output_view.active_match, Some(0));
        assert_eq!(state.output_view.scroll_offset, 2);
    }

    #[test]
    fn output_search_ignores_literal_map_snapshots() {
        let mut state = AppState::new(&AppConfig::default());
        state.push_output("bear", OutputCategory::Normal);
        state.push_output_snapshot(vec!["bear".to_string()]);
        state.output_view.search_input = "bear".to_string();

        state.commit_output_search();

        assert_eq!(state.output_view.search_matches, vec![0]);
    }

    #[test]
    fn output_display_mode_cycles() {
        let mut state = AppState::new(&AppConfig::default());

        state.toggle_output_display_mode();
        assert_eq!(state.output_view.display_mode, OutputDisplayMode::Plain);
        assert_eq!(
            state.output_view.status_ticks_remaining,
            OUTPUT_STATUS_VISIBLE_TICKS
        );
        state.toggle_output_display_mode();
        assert_eq!(state.output_view.display_mode, OutputDisplayMode::Debug);
        state.toggle_output_display_mode();
        assert_eq!(state.output_view.display_mode, OutputDisplayMode::Styled);
    }

    #[test]
    fn output_status_tick_expires_after_mode_cycle() {
        let mut state = AppState::new(&AppConfig::default());

        state.toggle_output_display_mode();
        for _ in 0..OUTPUT_STATUS_VISIBLE_TICKS {
            state.tick_output_status();
        }

        assert_eq!(state.output_view.status_ticks_remaining, 0);
        state.tick_output_status();
        assert_eq!(state.output_view.status_ticks_remaining, 0);
    }

    #[test]
    fn normal_output_replaces_stale_live_prompt() {
        let mut state = AppState::new(&AppConfig::default());
        state.push_prompt("Gram>");
        state.push_output("A tall bear strides through.", OutputCategory::Normal);

        assert_eq!(state.output.len(), 1);
        assert_eq!(
            state.output.front().map(|line| line.raw.as_str()),
            Some("A tall bear strides through.")
        );
        assert_eq!(
            state.output.front().map(|line| line.starts_new_output),
            Some(true)
        );
    }

    #[test]
    fn maps_rots_character_and_group_values() {
        let mut payload = Vec::new();
        pair(&mut payload, "HEALTH", b"75");
        pair(&mut payload, "HEALTH_MAX", b"100");
        pair(&mut payload, "MOVEMENT", b"31");
        pair(&mut payload, "EXPERIENCE", b"137000");
        pair(&mut payload, "EXPERIENCE_MAX", b"200000");
        pair(&mut payload, "OFFENSIVE_BONUS", b"134");
        pair(&mut payload, "DODGE", b"29");
        pair(&mut payload, "PARRY", b"58");
        pair(&mut payload, "ATTACK_SPEED", b"12");
        pair(&mut payload, "STR", b"20");
        pair(&mut payload, "INT", b"17");
        pair(&mut payload, "WILL", b"18");
        pair(&mut payload, "DEX", b"19");
        pair(&mut payload, "CON", b"21");
        pair(&mut payload, "LEA", b"16");
        pair(&mut payload, "WILLPOWER", b"44");
        pair(&mut payload, "SPELL_SAVE", b"55");
        pair(&mut payload, "SPIRIT", b"2310");
        pair(&mut payload, "SPELL_POWER", b"7");
        pair(&mut payload, "SPELL_PEN", b"3");
        pair(&mut payload, "WARRIOR_LEVEL", b"10");
        pair(&mut payload, "RANGER_LEVEL", b"12");
        pair(&mut payload, "MYSTIC_LEVEL", b"30");
        pair(&mut payload, "MAGE_LEVEL", b"18");
        pair(&mut payload, "HEALTH_REGENERATION", b"11");
        pair(&mut payload, "STAMINA_REGENERATION", b"13");
        pair(&mut payload, "MOVEMENT_REGENERATION", b"15");
        pair(&mut payload, "WORLD_TIME", b"Late afternoon");
        pair(
            &mut payload,
            "WEATHER",
            b"Above the fields, not a cloud can be seen in the sky.",
        );

        payload.push(MSDP_VAR);
        payload.extend_from_slice(b"GROUP");
        payload.push(MSDP_VAL);
        payload.push(MSDP_TABLE_OPEN);
        payload.push(MSDP_VAR);
        payload.extend_from_slice(b"MEMBERS");
        payload.push(MSDP_VAL);
        payload.push(MSDP_ARRAY_OPEN);
        payload.push(MSDP_VAL);
        payload.push(MSDP_TABLE_OPEN);
        pair(&mut payload, "NAME", b"Aragorn");
        pair(&mut payload, "HEALTH", b"75");
        pair(&mut payload, "MANA", b"50");
        pair(&mut payload, "MOVEMENT", b"25");
        payload.push(MSDP_TABLE_CLOSE);
        payload.push(MSDP_ARRAY_CLOSE);
        payload.push(MSDP_TABLE_CLOSE);

        let frames = parse_msdp_payload(&payload).expect("payload should parse");
        let config = AppConfig::default();
        let mut state = AppState::new(&config);
        state.apply_msdp_frames(&frames, &config.msdp.mapping);

        assert_eq!(state.character.health, Some(75));
        assert_eq!(state.character.health_max, Some(100));
        assert_eq!(state.character.movement, Some(31));
        assert_eq!(state.character.experience, Some(137000));
        assert_eq!(state.character.experience_max, Some(200000));
        assert_eq!(state.character.offensive_bonus, Some(134));
        assert_eq!(state.character.dodge, Some(29));
        assert_eq!(state.character.parry, Some(58));
        assert_eq!(state.character.attack_speed, Some(12));
        assert_eq!(state.character.strength, Some(20));
        assert_eq!(state.character.intelligence, Some(17));
        assert_eq!(state.character.will, Some(18));
        assert_eq!(state.character.dexterity, Some(19));
        assert_eq!(state.character.constitution, Some(21));
        assert_eq!(state.character.learning, Some(16));
        assert_eq!(state.character.willpower, Some(44));
        assert_eq!(state.character.spell_save, Some(55));
        assert_eq!(state.character.spirit, Some(2310));
        assert_eq!(state.character.spell_power, Some(7));
        assert_eq!(state.character.spell_pen, Some(3));
        assert_eq!(state.character.warrior_level, Some(10));
        assert_eq!(state.character.ranger_level, Some(12));
        assert_eq!(state.character.mystic_level, Some(30));
        assert_eq!(state.character.mage_level, Some(18));
        assert_eq!(state.character.health_regeneration, Some(11));
        assert_eq!(state.character.stamina_regeneration, Some(13));
        assert_eq!(state.character.movement_regeneration, Some(15));
        assert_eq!(state.world.time.as_deref(), Some("Late afternoon"));
        assert_eq!(
            state.world.weather.as_deref(),
            Some("Above the fields, not a cloud can be seen in the sky.")
        );
        assert_eq!(state.group.members.len(), 1);
        assert_eq!(state.group.members[0].name, "Aragorn");
        assert_eq!(state.group.members[0].movement_percent, Some(25));
    }

    #[test]
    fn maps_rots_room_table_exit_targets() {
        let mut payload = Vec::new();
        payload.push(MSDP_VAR);
        payload.extend_from_slice(b"ROOM_EXITS");
        payload.push(MSDP_VAL);
        payload.push(MSDP_ARRAY_OPEN);
        payload.push(MSDP_VAL);
        payload.extend_from_slice(b"n");
        payload.push(MSDP_VAL);
        payload.extend_from_slice(b"e");
        payload.push(MSDP_ARRAY_CLOSE);

        payload.push(MSDP_VAR);
        payload.extend_from_slice(b"ROOM");
        payload.push(MSDP_VAL);
        payload.push(MSDP_TABLE_OPEN);
        pair(&mut payload, "VNUM", b"100");
        pair(&mut payload, "NAME", b"Light Forest");
        pair(&mut payload, "TERRAIN", b"Forest");
        payload.push(MSDP_VAR);
        payload.extend_from_slice(b"EXITS");
        payload.push(MSDP_VAL);
        payload.push(MSDP_ARRAY_OPEN);
        payload.push(MSDP_VAL);
        payload.extend_from_slice(b"101");
        payload.push(MSDP_VAL);
        payload.extend_from_slice(b"102");
        payload.push(MSDP_ARRAY_CLOSE);
        payload.push(MSDP_TABLE_CLOSE);

        let frames = parse_msdp_payload(&payload).expect("payload should parse");
        let config = AppConfig::default();
        let mut state = AppState::new(&config);
        state.apply_msdp_frames(&frames, &config.msdp.mapping);

        assert_eq!(state.map.current_room.as_deref(), Some("100"));
        assert_eq!(state.map.rooms["100"].name, "Light Forest");
        assert_eq!(state.map.rooms["100"].terrain, "Forest");
        assert_eq!(state.map.rooms["100"].exits["n"].to.as_deref(), Some("101"));
        assert_eq!(state.map.rooms["100"].exits["e"].to.as_deref(), Some("102"));
    }

    #[test]
    fn maps_rots_room_table_exit_targets_by_direction_key() {
        let config = AppConfig::default();
        let mapping = &config.msdp.mapping;
        let mut exits = HashMap::new();
        exits.insert("w".to_string(), MsdpValue::String("99".to_string()));
        exits.insert("e".to_string(), MsdpValue::String("101".to_string()));
        exits.insert("s".to_string(), MsdpValue::String("200".to_string()));
        let mut room = HashMap::new();
        room.insert("VNUM".to_string(), MsdpValue::String("100".to_string()));
        room.insert(
            "NAME".to_string(),
            MsdpValue::String("Keyed Exits".to_string()),
        );
        room.insert("EXITS".to_string(), MsdpValue::Table(exits));
        let mut state = AppState::new(&config);

        state.apply_msdp_frames(
            &[
                MsdpFrame {
                    variable: mapping.room_exits.clone(),
                    value: room_exits(["e", "s", "w"]),
                },
                MsdpFrame {
                    variable: mapping.room.clone(),
                    value: MsdpValue::Table(room),
                },
            ],
            mapping,
        );

        let room = &state.map.rooms["100"];
        assert_eq!(room.exits["e"].to.as_deref(), Some("101"));
        assert_eq!(room.exits["s"].to.as_deref(), Some("200"));
        assert_eq!(room.exits["w"].to.as_deref(), Some("99"));
        assert_eq!((state.map.rooms["101"].x, state.map.rooms["101"].y), (1, 0));
        assert_eq!((state.map.rooms["200"].x, state.map.rooms["200"].y), (0, 1));
        assert_eq!((state.map.rooms["99"].x, state.map.rooms["99"].y), (-1, 0));
    }

    #[test]
    fn standalone_room_exits_wait_for_room_table_to_record() {
        let config = AppConfig::default();
        let mapping = &config.msdp.mapping;
        let mut state = AppState::new(&config);

        state.apply_msdp_frames(
            &[
                MsdpFrame {
                    variable: mapping.room_exits.clone(),
                    value: room_exits(["n"]),
                },
                MsdpFrame {
                    variable: mapping.room.clone(),
                    value: room_table("100", "Old Room", ["101"]),
                },
            ],
            mapping,
        );

        state.apply_msdp_frames(
            &[MsdpFrame {
                variable: mapping.room_exits.clone(),
                value: room_exits(["e"]),
            }],
            mapping,
        );
        assert!(!state.map.rooms["100"].exits.contains_key("e"));

        state.apply_msdp_frames(
            &[MsdpFrame {
                variable: mapping.room.clone(),
                value: room_table("200", "New Room", ["201"]),
            }],
            mapping,
        );

        assert_eq!(state.map.current_room.as_deref(), Some("200"));
        assert_eq!(state.map.rooms["200"].exits["e"].to.as_deref(), Some("201"));
    }

    #[test]
    fn standalone_room_vnum_does_not_move_map_without_room_table() {
        let config = AppConfig::default();
        let mapping = &config.msdp.mapping;
        let mut state = AppState::new(&config);

        state.apply_msdp_frames(
            &[
                MsdpFrame {
                    variable: mapping.room_exits.clone(),
                    value: room_exits(["n"]),
                },
                MsdpFrame {
                    variable: mapping.room.clone(),
                    value: room_table("100", "Old Room", ["101"]),
                },
            ],
            mapping,
        );

        state.apply_msdp_frames(
            &[MsdpFrame {
                variable: mapping.room_vnum.clone(),
                value: MsdpValue::String("200".to_string()),
            }],
            mapping,
        );

        assert_eq!(state.map.current_room.as_deref(), Some("100"));
        assert!(!state.map.rooms.contains_key("200"));
    }

    #[test]
    fn fresh_room_table_uses_current_room_exits_even_with_stale_room_vnum() {
        let config = AppConfig::default();
        let mapping = &config.msdp.mapping;
        let mut state = AppState::new(&config);

        state.apply_msdp_frames(
            &[
                MsdpFrame {
                    variable: mapping.room_vnum.clone(),
                    value: MsdpValue::String("100".to_string()),
                },
                MsdpFrame {
                    variable: mapping.room_exits.clone(),
                    value: room_exits(["n"]),
                },
                MsdpFrame {
                    variable: mapping.room.clone(),
                    value: room_table("100", "Old Room", ["101"]),
                },
            ],
            mapping,
        );

        state.apply_msdp_frames(
            &[
                MsdpFrame {
                    variable: mapping.room_exits.clone(),
                    value: room_exits(["e"]),
                },
                MsdpFrame {
                    variable: mapping.room.clone(),
                    value: room_table("200", "New Room", ["201"]),
                },
            ],
            mapping,
        );

        assert_eq!(state.map.current_room.as_deref(), Some("200"));
        assert_eq!(state.map.rooms["200"].exits["e"].to.as_deref(), Some("201"));
    }

    #[test]
    fn room_exits_update_replaces_existing_room_exits_after_room_exists() {
        let config = AppConfig::default();
        let mapping = &config.msdp.mapping;
        let mut state = AppState::new(&config);

        state.apply_msdp_frames(
            &[
                MsdpFrame {
                    variable: mapping.room_exits.clone(),
                    value: room_exits(["e", "n", "s", "w"]),
                },
                MsdpFrame {
                    variable: mapping.room.clone(),
                    value: room_table("2811", "Grassland", ["2800", "2812", "2813", "2814"]),
                },
            ],
            mapping,
        );
        assert!(state.map.rooms["2811"].exits.contains_key("n"));

        state.apply_msdp_frames(
            &[MsdpFrame {
                variable: mapping.room_exits.clone(),
                value: room_exits(["e", "s", "w"]),
            }],
            mapping,
        );
        assert!(state.map.rooms["2811"].exits.contains_key("n"));

        state.apply_msdp_frames(
            &[MsdpFrame {
                variable: mapping.room.clone(),
                value: room_table("2811", "Grassland", ["2800", "2813", "2814"]),
            }],
            mapping,
        );

        let room = &state.map.rooms["2811"];
        assert!(room.exits.contains_key("e"));
        assert!(!room.exits.contains_key("n"));
        assert!(room.exits.contains_key("s"));
        assert!(room.exits.contains_key("w"));
    }

    #[test]
    fn fresh_keyed_room_table_does_not_filter_through_stale_room_exits() {
        let config = AppConfig::default();
        let mapping = &config.msdp.mapping;
        let mut state = AppState::new(&config);

        state.apply_msdp_frames(
            &[
                MsdpFrame {
                    variable: mapping.room_exits.clone(),
                    value: room_exits(["n"]),
                },
                MsdpFrame {
                    variable: mapping.room.clone(),
                    value: room_table("100", "Old Room", ["101"]),
                },
            ],
            mapping,
        );

        let mut exits = HashMap::new();
        exits.insert("e".to_string(), MsdpValue::String("201".to_string()));
        let mut room = HashMap::new();
        room.insert("VNUM".to_string(), MsdpValue::String("200".to_string()));
        room.insert(
            "NAME".to_string(),
            MsdpValue::String("New Room".to_string()),
        );
        room.insert("EXITS".to_string(), MsdpValue::Table(exits));

        state.apply_msdp_frames(
            &[
                MsdpFrame {
                    variable: mapping.room_exits.clone(),
                    value: room_exits(["e"]),
                },
                MsdpFrame {
                    variable: mapping.room.clone(),
                    value: MsdpValue::Table(room),
                },
            ],
            mapping,
        );

        assert_eq!(state.map.current_room.as_deref(), Some("200"));
        assert_eq!(state.map.rooms["200"].exits["e"].to.as_deref(), Some("201"));
        assert!(!state.map.rooms["200"].exits.contains_key("n"));
    }

    fn pair(payload: &mut Vec<u8>, variable: &str, value: &[u8]) {
        payload.push(MSDP_VAR);
        payload.extend_from_slice(variable.as_bytes());
        payload.push(MSDP_VAL);
        payload.extend_from_slice(value);
    }

    fn room_exits<const N: usize>(directions: [&str; N]) -> MsdpValue {
        MsdpValue::Array(
            directions
                .into_iter()
                .map(|direction| MsdpValue::String(direction.to_string()))
                .collect(),
        )
    }

    fn room_table<const N: usize>(vnum: &str, name: &str, exits: [&str; N]) -> MsdpValue {
        let mut table = HashMap::new();
        table.insert("VNUM".to_string(), MsdpValue::String(vnum.to_string()));
        table.insert("NAME".to_string(), MsdpValue::String(name.to_string()));
        table.insert(
            "EXITS".to_string(),
            MsdpValue::Array(
                exits
                    .into_iter()
                    .map(|exit| MsdpValue::String(exit.to_string()))
                    .collect(),
            ),
        );
        MsdpValue::Table(table)
    }
}
