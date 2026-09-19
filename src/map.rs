use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use serde::{Deserialize, Serialize};

use crate::network::msdp::MsdpValue;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct MapState {
    pub rooms: BTreeMap<String, Room>,
    pub current_room: Option<String>,
    pub previous_room: Option<String>,
    pub follow: bool,
    pub static_mode: bool,
    pub nofollow: bool,
    pub direction_arrow: bool,
    pub unicode: bool,
    pub show_vnums: bool,
    pub last_direction: Option<String>,
    next_generated_id: u64,
    last_move: Option<MapMove>,
}

impl Default for MapState {
    fn default() -> Self {
        Self {
            rooms: BTreeMap::new(),
            current_room: None,
            previous_room: None,
            follow: true,
            static_mode: false,
            nofollow: false,
            direction_arrow: true,
            unicode: true,
            show_vnums: false,
            last_direction: None,
            next_generated_id: 1,
            last_move: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Room {
    pub id: String,
    pub name: String,
    pub description: String,
    pub area: String,
    pub note: String,
    pub terrain: String,
    pub symbol: String,
    pub weight: f32,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub exits: BTreeMap<String, Exit>,
    pub exit_order: Vec<String>,
    pub flags: BTreeSet<RoomFlag>,
}

impl Default for Room {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            description: String::new(),
            area: String::new(),
            note: String::new(),
            terrain: String::new(),
            symbol: String::new(),
            weight: 1.0,
            x: 0,
            y: 0,
            z: 0,
            exits: BTreeMap::new(),
            exit_order: Vec::new(),
            flags: BTreeSet::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(default)]
pub struct Exit {
    pub direction: String,
    pub to: Option<String>,
    pub command: Option<String>,
    pub flags: BTreeSet<ExitFlag>,
    pub door: Option<DoorState>,
    pub door_name: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DoorState {
    Open,
    Closed,
    Pickable,
    Locked,
    Trigger,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum RoomFlag {
    Avoid,
    Block,
    Curved,
    Fog,
    Hide,
    Invis,
    Leave,
    NoGlobal,
    Void,
    Static,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum ExitFlag {
    Avoid,
    Block,
    Hide,
    Invis,
    Teleport,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MapCommandResult {
    pub message: String,
    pub commands: Vec<String>,
    pub show_map: bool,
    pub variable: Option<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomExitUpdate {
    pub direction: String,
    pub to: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct MapMove {
    from: String,
    to: String,
    direction: String,
    created_room: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Direction {
    name: &'static str,
    reverse: &'static str,
    dx: i32,
    dy: i32,
    dz: i32,
}

impl MapState {
    pub fn execute(&mut self, input: &str) -> Result<MapCommandResult, String> {
        let mut parts = input.split_whitespace();
        let Some(command) = parts.next() else {
            return Ok(self.help());
        };
        match command {
            "create" => {
                self.create();
                Ok(self.message("Map created at room 1."))
            }
            "goto" => {
                let target = required(parts.next(), "usage: /map goto <vnum|name>")?;
                let dig = parts.next().is_some_and(|value| value == "dig");
                self.goto(target, dig)?;
                Ok(self.message(format!("Map location set to {}.", self.current_label())))
            }
            "move" => {
                let direction = required(parts.next(), "usage: /map move <direction>")?;
                self.move_direction(direction)?;
                Ok(self.message(format!(
                    "Moved map {} to {}.",
                    direction,
                    self.current_label()
                )))
            }
            "dig" => {
                let direction = required(parts.next(), "usage: /map dig <direction> [new|vnum]")?;
                let target = parts.next();
                let room = self.dig(direction, target)?;
                Ok(self.message(format!("Dug {} to {}.", direction, room)))
            }
            "link" => {
                let direction =
                    required(parts.next(), "usage: /map link <direction> <vnum> [both]")?;
                let target = required(parts.next(), "usage: /map link <direction> <vnum> [both]")?;
                let both = parts.next().is_some_and(|value| value == "both");
                self.link(direction, target, both)?;
                Ok(self.message(format!("Linked {} to {}.", direction, target)))
            }
            "unlink" => {
                let direction = required(parts.next(), "usage: /map unlink <direction>")?;
                self.unlink(direction)?;
                Ok(self.message(format!("Unlinked {}.", direction)))
            }
            "delete" => {
                let target = required(parts.next(), "usage: /map delete <direction|vnum>")?;
                self.delete(target)?;
                Ok(self.message(format!("Deleted {}.", target)))
            }
            "undo" => {
                self.undo()?;
                Ok(self.message(format!("Returned to {}.", self.current_label())))
            }
            "return" => {
                let previous = self
                    .previous_room
                    .clone()
                    .ok_or_else(|| "No previous map room is available.".to_string())?;
                self.current_room = Some(previous);
                Ok(self.message(format!("Returned to {}.", self.current_label())))
            }
            "leave" => {
                self.previous_room = self.current_room.take();
                Ok(self.message("Left the map. Use /map return to restore the previous room."))
            }
            "set" => {
                let option = required(parts.next(), "usage: /map set <option> <value>")?;
                let value = parts.collect::<Vec<_>>().join(" ");
                if value.is_empty() {
                    return Err("usage: /map set <option> <value>".to_string());
                }
                self.set(option, &value)?;
                Ok(self.message(format!("Set {} on {}.", option, self.current_label())))
            }
            "get" | "info" => Ok(self.message(self.info())),
            "list" => {
                let query = parts.collect::<Vec<_>>().join(" ");
                Ok(self.message(self.list(&query)))
            }
            "find" => {
                let query = parts.collect::<Vec<_>>().join(" ");
                let path = self.find_path(&query)?;
                Ok(self.message(format_path(&path)))
            }
            "run" => {
                let query = parts.collect::<Vec<_>>().join(" ");
                let path = self.find_path(&query)?;
                Ok(MapCommandResult {
                    message: format_path(&path),
                    commands: path,
                    show_map: false,
                    variable: None,
                })
            }
            "map" => Ok(MapCommandResult {
                message: String::new(),
                commands: Vec::new(),
                show_map: true,
                variable: None,
            }),
            "flag" => {
                let flag = required(parts.next(), "usage: /map flag <name> [on|off]")?;
                let enabled = parse_toggle(parts.next());
                self.set_flag(flag, enabled)?;
                Ok(self.message(format!("Map flag {} updated.", flag)))
            }
            "roomflag" => {
                let Some(flags) = parts.next() else {
                    return Ok(self.message(self.room_flag_listing()?));
                };
                let action = parts.next();
                if action.is_some_and(|value| room_flag_action_matches(value, "get")) {
                    let variable =
                        required(parts.next(), "usage: /map roomflag <flag> get <variable>")?;
                    let enabled = self.room_flags_enabled(flags)?;
                    return Ok(MapCommandResult {
                        message: format!(
                            "Room flag {} is {}.",
                            flags,
                            if enabled { "ON" } else { "OFF" }
                        ),
                        commands: Vec::new(),
                        show_map: false,
                        variable: Some((
                            variable.to_string(),
                            if enabled { "1" } else { "0" }.to_string(),
                        )),
                    });
                }
                let enabled = parse_toggle(action);
                let updates = self.set_room_flags(flags, enabled)?;
                Ok(self.message(room_flag_update_message(&updates)))
            }
            "exitflag" => {
                let direction = required(
                    parts.next(),
                    "usage: /map exitflag <direction> <flag> [on|off]",
                )?;
                let flag = required(
                    parts.next(),
                    "usage: /map exitflag <direction> <flag> [on|off]",
                )?;
                let enabled = parse_toggle(parts.next());
                self.set_exit_flag(direction, flag, enabled)?;
                Ok(self.message(format!("Exit flag {} {} updated.", direction, flag)))
            }
            "door" => {
                let direction = required(
                    parts.next(),
                    "usage: /map door <direction> [state|none] [name]",
                )?;
                let values = parts.collect::<Vec<_>>();
                let (state, name) = parse_door_args(&values)?;
                self.set_door(direction, state, name)?;
                Ok(self.message(format!("Door {} updated.", direction)))
            }
            "read" => {
                let path = required(parts.next(), "usage: /map read <file>")?;
                self.read_from_path(path)?;
                Ok(self.message(format!("Read map from {}.", path)))
            }
            "write" => {
                let path = required(parts.next(), "usage: /map write <file>")?;
                self.write_to_path(path)?;
                Ok(self.message(format!("Wrote map to {}.", path)))
            }
            "help" => Ok(self.help()),
            _ => Err(format!("Unknown map command `{}`. Try /map help.", command)),
        }
    }

    pub fn create(&mut self) {
        self.rooms.clear();
        self.current_room = Some("1".to_string());
        self.previous_room = None;
        self.next_generated_id = 2;
        self.last_move = None;
        self.rooms.insert(
            "1".to_string(),
            Room {
                id: "1".to_string(),
                name: "Room 1".to_string(),
                ..Room::default()
            },
        );
    }

    pub fn sync_room(
        &mut self,
        id: Option<String>,
        name: Option<String>,
        exits: Vec<RoomExitUpdate>,
        replace_exits: bool,
    ) {
        let Some(id) = id.filter(|value| !value.trim().is_empty()) else {
            if let Some(name) = name {
                self.set_current_name(&name);
            }
            return;
        };
        if self.rooms.is_empty() {
            self.create();
        }
        self.adopt_speculative_current_room_id(&id);
        if !self.rooms.contains_key(&id) {
            let (x, y, z) = self.next_coordinate_from_current().unwrap_or((0, 0, 0));
            self.rooms.insert(
                id.clone(),
                Room {
                    id: id.clone(),
                    x,
                    y,
                    z,
                    ..Room::default()
                },
            );
        }
        if self.current_room.as_deref() != Some(&id) {
            self.previous_room = self.current_room.replace(id.clone());
        } else {
            self.current_room = Some(id.clone());
        }
        if let Some(room) = self.rooms.get_mut(&id)
            && let Some(name) = name.filter(|value| !value.trim().is_empty())
        {
            room.name = name;
        }
        if replace_exits {
            self.sync_current_room_exits(&id, exits);
        }
    }

    pub fn sync_current_room_terrain(&mut self, terrain: Option<String>) {
        let Some(current_id) = self.current_room.as_ref() else {
            return;
        };
        let Some(terrain) = terrain.filter(|value| !value.trim().is_empty()) else {
            return;
        };
        if let Some(room) = self.rooms.get_mut(current_id) {
            room.set_terrain(terrain);
        }
    }

    fn adopt_speculative_current_room_id(&mut self, authoritative_id: &str) {
        if self.rooms.contains_key(authoritative_id) {
            return;
        }
        let Some(last_move) = self.last_move.clone().filter(|value| value.created_room) else {
            return;
        };
        if self.current_room.as_deref() != Some(&last_move.to) {
            return;
        }
        let Some(mut room) = self.rooms.remove(&last_move.to) else {
            return;
        };

        room.id = authoritative_id.to_string();
        self.rooms.insert(authoritative_id.to_string(), room);
        self.current_room = Some(authoritative_id.to_string());
        self.rewrite_exit_targets(&last_move.to, authoritative_id);
        self.last_move = Some(MapMove {
            to: authoritative_id.to_string(),
            ..last_move
        });
    }

    fn rewrite_exit_targets(&mut self, old_id: &str, new_id: &str) {
        for room in self.rooms.values_mut() {
            for exit in room.exits.values_mut() {
                if exit.to.as_deref() == Some(old_id) {
                    exit.to = Some(new_id.to_string());
                }
            }
        }
    }

    fn sync_current_room_exits(&mut self, id: &str, exits: Vec<RoomExitUpdate>) {
        let reported_directions = exits
            .iter()
            .map(|exit| normalize_direction(&exit.direction))
            .collect::<BTreeSet<_>>();
        if let Some(room) = self.rooms.get_mut(id) {
            room.exits
                .retain(|direction, _| reported_directions.contains(direction));
            room.exit_order = exits
                .iter()
                .map(|exit| normalize_direction(&exit.direction))
                .filter(|direction| reported_directions.contains(direction))
                .collect();
        }
        for exit in exits {
            let direction = normalize_direction(&exit.direction);
            let target = exit.to.filter(|value| !value.trim().is_empty());
            if let Some(target) = target.as_ref() {
                self.ensure_adjacent_room(id, &direction, target);
                if let Some(reverse) = direction_by_name(&direction).map(|value| value.reverse) {
                    self.set_exit(target, reverse, Some(id.to_string()));
                }
            }
            if let Some(room) = self.rooms.get_mut(id) {
                let mapped_exit = room.exits.entry(direction.clone()).or_insert_with(|| Exit {
                    direction: direction.clone(),
                    ..Exit::default()
                });
                if target.is_some() {
                    mapped_exit.to = target;
                }
            }
        }
    }

    fn ensure_adjacent_room(&mut self, from: &str, direction: &str, target: &str) {
        let (x, y, z) = self
            .rooms
            .get(from)
            .map(|room| {
                if let Some(direction) = direction_by_name(direction) {
                    (
                        room.x + direction.dx,
                        room.y + direction.dy,
                        room.z + direction.dz,
                    )
                } else {
                    (room.x, room.y, room.z)
                }
            })
            .unwrap_or((0, 0, 0));
        if let Some(room) = self.rooms.get_mut(target) {
            if room.name.is_empty() && room.exits.is_empty() && room.flags.is_empty() {
                room.x = x;
                room.y = y;
                room.z = z;
            }
            return;
        }
        self.rooms.insert(
            target.to_string(),
            Room {
                id: target.to_string(),
                x,
                y,
                z,
                ..Room::default()
            },
        );
    }

    pub fn move_direction(&mut self, direction: &str) -> Result<(), String> {
        if self.nofollow {
            return Ok(());
        }
        if self.rooms.is_empty() {
            self.create();
        }
        let direction = normalize_direction(direction);
        let Some(from) = self.current_room.clone() else {
            return Err("You are not inside the map. Use /map goto <vnum>.".to_string());
        };
        let Some(dir) = direction_by_name(&direction) else {
            return Err(format!("Unknown movement direction `{}`.", direction));
        };
        let existing_to = self
            .rooms
            .get(&from)
            .and_then(|room| room.exits.get(&direction))
            .and_then(|exit| exit.to.clone());
        let (to, created_room) = if let Some(to) = existing_to {
            (to, false)
        } else {
            if self.static_mode || self.room_has_flag(&from, RoomFlag::Static) {
                return Err(format!("No mapped exit {} from {}.", direction, from));
            }
            let new_id = self.next_id();
            let (x, y, z) = self
                .rooms
                .get(&from)
                .map(|room| (room.x + dir.dx, room.y + dir.dy, room.z + dir.dz))
                .unwrap_or((0, 0, 0));
            self.rooms.insert(
                new_id.clone(),
                Room {
                    id: new_id.clone(),
                    x,
                    y,
                    z,
                    ..Room::default()
                },
            );
            self.set_exit(&from, &direction, Some(new_id.clone()));
            self.set_exit(&new_id, dir.reverse, Some(from.clone()));
            (new_id, true)
        };
        self.previous_room = Some(from.clone());
        self.current_room = Some(to.clone());
        self.last_direction = Some(direction.clone());
        self.last_move = Some(MapMove {
            from,
            to: to.clone(),
            direction,
            created_room,
        });
        if self.room_has_flag(&to, RoomFlag::Leave) {
            self.previous_room = self.current_room.take();
        }
        Ok(())
    }

    pub fn shortest_path_to(&self, target: &str) -> Option<Vec<String>> {
        let start = self.current_room.as_ref()?;
        let target = self.resolve_room(target)?;
        if start == &target {
            return Some(Vec::new());
        }
        let mut unsettled = BTreeSet::from([start.clone()]);
        let mut distance = BTreeMap::from([(start.clone(), 0.0_f32)]);
        let mut previous: BTreeMap<String, (String, String)> = BTreeMap::new();
        while let Some(room_id) = unsettled
            .iter()
            .min_by(|left, right| {
                let left_distance = distance.get(*left).copied().unwrap_or(f32::INFINITY);
                let right_distance = distance.get(*right).copied().unwrap_or(f32::INFINITY);
                left_distance
                    .partial_cmp(&right_distance)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| left.cmp(right))
            })
            .cloned()
        {
            unsettled.remove(&room_id);
            let current_distance = distance.get(&room_id).copied().unwrap_or(f32::INFINITY);
            let Some(room) = self.rooms.get(&room_id) else {
                continue;
            };
            if room.flags.contains(&RoomFlag::Block) || room.flags.contains(&RoomFlag::Avoid) {
                continue;
            }
            if room_id == target {
                break;
            }
            for (direction, exit) in &room.exits {
                if exit
                    .flags
                    .iter()
                    .any(|flag| matches!(flag, ExitFlag::Block | ExitFlag::Avoid))
                {
                    continue;
                }
                let Some(next) = &exit.to else {
                    continue;
                };
                let next = self.tunnel_void_target(&room_id, next, &exit.direction);
                let Some(next_room) = self.rooms.get(&next) else {
                    continue;
                };
                if next_room.flags.contains(&RoomFlag::Block)
                    || next_room.flags.contains(&RoomFlag::Avoid)
                {
                    continue;
                }
                let next_distance = current_distance + next_room.path_weight();
                if next_distance < distance.get(&next).copied().unwrap_or(f32::INFINITY) {
                    distance.insert(next.clone(), next_distance);
                    previous.insert(next.clone(), (room_id.clone(), direction.clone()));
                    unsettled.insert(next);
                }
            }
        }
        if !previous.contains_key(&target) {
            return None;
        }
        let mut path = Vec::new();
        let mut cursor = target;
        while let Some((prior, direction)) = previous.get(&cursor) {
            path.push(direction.clone());
            cursor = prior.clone();
            if &cursor == start {
                break;
            }
        }
        path.reverse();
        Some(path)
    }

    fn tunnel_void_target(&self, from: &str, room: &str, direction: &str) -> String {
        let mut from = from.to_string();
        let mut room = room.to_string();
        let mut direction = normalize_direction(direction);
        let mut seen = BTreeSet::new();
        while seen.insert(room.clone()) {
            let Some(current) = self.rooms.get(&room) else {
                return room;
            };
            if !current.flags.contains(&RoomFlag::Void) {
                return room;
            }
            if current.exits.len() != 2 {
                let Some(exit) = current.exits.get(&direction) else {
                    return room;
                };
                let Some(next) = exit.to.as_ref() else {
                    return room;
                };
                let next = next.clone();
                let next_direction = normalize_direction(&exit.direction);
                from = room;
                room = next;
                direction = next_direction;
                continue;
            }
            let Some(exit) = current
                .exits
                .values()
                .find(|exit| exit.to.as_deref() != Some(from.as_str()))
            else {
                return room;
            };
            let Some(next) = exit.to.as_ref() else {
                return room;
            };
            let next = next.clone();
            let next_direction = normalize_direction(&exit.direction);
            from = room;
            room = next;
            direction = next_direction;
        }
        room
    }

    fn goto(&mut self, target: &str, dig: bool) -> Result<(), String> {
        let id = if let Some(id) = self.resolve_room(target) {
            id
        } else if dig {
            self.new_room_with_id(target)
        } else {
            return Err(format!("No mapped room matches `{}`.", target));
        };
        self.previous_room = self.current_room.replace(id);
        Ok(())
    }

    fn dig(&mut self, direction: &str, target: Option<&str>) -> Result<String, String> {
        if self.rooms.is_empty() {
            self.create();
        }
        let direction = normalize_direction(direction);
        let Some(from) = self.current_room.clone() else {
            return Err("You are not inside the map. Use /map goto <vnum>.".to_string());
        };
        let room_id = match target {
            Some("new") | None => {
                let id = self.next_id();
                let (x, y, z) = self
                    .rooms
                    .get(&from)
                    .and_then(|room| {
                        direction_by_name(&direction)
                            .map(|dir| (room.x + dir.dx, room.y + dir.dy, room.z + dir.dz))
                    })
                    .unwrap_or((0, 0, 0));
                self.rooms.insert(
                    id.clone(),
                    Room {
                        id: id.clone(),
                        x,
                        y,
                        z,
                        ..Room::default()
                    },
                );
                id
            }
            Some(id) => {
                if !self.rooms.contains_key(id) {
                    self.new_room_with_id(id)
                } else {
                    id.to_string()
                }
            }
        };
        self.set_exit(&from, &direction, Some(room_id.clone()));
        Ok(room_id)
    }

    fn link(&mut self, direction: &str, target: &str, both: bool) -> Result<(), String> {
        let Some(from) = self.current_room.clone() else {
            return Err("You are not inside the map. Use /map goto <vnum>.".to_string());
        };
        if !self.rooms.contains_key(target) {
            self.new_room_with_id(target);
        }
        let direction = normalize_direction(direction);
        self.set_exit(&from, &direction, Some(target.to_string()));
        if both && let Some(reverse) = direction_by_name(&direction).map(|dir| dir.reverse) {
            self.set_exit(target, reverse, Some(from));
        }
        Ok(())
    }

    fn unlink(&mut self, direction: &str) -> Result<(), String> {
        let Some(current) = self.current_room.clone() else {
            return Err("You are not inside the map.".to_string());
        };
        if let Some(room) = self.rooms.get_mut(&current) {
            room.exits.remove(&normalize_direction(direction));
        }
        Ok(())
    }

    fn delete(&mut self, target: &str) -> Result<(), String> {
        let direction = normalize_direction(target);
        if let Some(current) = self.current_room.clone()
            && self
                .rooms
                .get(&current)
                .is_some_and(|room| room.exits.contains_key(&direction))
        {
            self.unlink(&direction)?;
            return Ok(());
        }
        self.rooms.remove(target);
        for room in self.rooms.values_mut() {
            room.exits
                .retain(|_, exit| exit.to.as_deref() != Some(target));
        }
        if self.current_room.as_deref() == Some(target) {
            self.current_room = None;
        }
        Ok(())
    }

    fn undo(&mut self) -> Result<(), String> {
        let Some(last) = self.last_move.take() else {
            return Err("No map move to undo.".to_string());
        };
        self.current_room = Some(last.from.clone());
        self.previous_room = Some(last.to.clone());
        if last.created_room {
            self.rooms.remove(&last.to);
            if let Some(room) = self.rooms.get_mut(&last.from) {
                room.exits.remove(&last.direction);
            }
        }
        Ok(())
    }

    fn set(&mut self, option: &str, value: &str) -> Result<(), String> {
        let Some(current) = self.current_room.clone() else {
            return Err("You are not inside the map.".to_string());
        };
        let Some(room) = self.rooms.get_mut(&current) else {
            return Err(format!("Current room {} is missing.", current));
        };
        match option {
            "roomname" | "name" => room.name = value.to_string(),
            "roomdesc" | "desc" | "description" => room.description = value.to_string(),
            "roomarea" | "area" => room.area = value.to_string(),
            "roomnote" | "note" => room.note = value.to_string(),
            "roomterrain" | "terrain" => room.set_terrain(value.to_string()),
            "roomsymbol" | "symbol" => room.symbol = value.chars().take(3).collect(),
            "roomweight" | "weight" => {
                room.weight = value
                    .parse::<f32>()
                    .map_err(|_| "roomweight must be a number".to_string())?;
            }
            _ => return Err(format!("Unknown map set option `{}`.", option)),
        }
        Ok(())
    }

    fn set_flag(&mut self, flag: &str, enabled: Option<bool>) -> Result<(), String> {
        let current = match flag {
            "static" => &mut self.static_mode,
            "nofollow" => &mut self.nofollow,
            "direction" => &mut self.direction_arrow,
            "unicode" => &mut self.unicode,
            "asciigraphics" => {
                self.unicode = enabled.unwrap_or(!self.unicode);
                return Ok(());
            }
            "asciivnums" => &mut self.show_vnums,
            _ => return Err(format!("Unknown map flag `{}`.", flag)),
        };
        *current = enabled.unwrap_or(!*current);
        Ok(())
    }

    fn room_flag_listing(&self) -> Result<String, String> {
        let Some(current) = self.current_room.as_ref() else {
            return Err("You are not inside the map.".to_string());
        };
        let Some(room) = self.rooms.get(current) else {
            return Err(format!("Current room {} is missing.", current));
        };
        let mut message = "# Room Flags".to_string();
        for flag in ROOM_FLAGS {
            message.push_str(&format!(
                "\n- `{}`: {}",
                flag.name(),
                if room.flags.contains(&flag) {
                    "ON"
                } else {
                    "OFF"
                }
            ));
        }
        Ok(message)
    }

    fn room_flags_enabled(&self, flags: &str) -> Result<bool, String> {
        let Some(current) = self.current_room.as_ref() else {
            return Err("You are not inside the map.".to_string());
        };
        let Some(room) = self.rooms.get(current) else {
            return Err(format!("Current room {} is missing.", current));
        };
        let flags = parse_room_flags(flags)?;
        Ok(flags.iter().all(|flag| room.flags.contains(flag)))
    }

    fn set_room_flags(
        &mut self,
        flags: &str,
        enabled: Option<bool>,
    ) -> Result<Vec<(RoomFlag, bool)>, String> {
        let Some(current) = self.current_room.clone() else {
            return Err("You are not inside the map.".to_string());
        };
        let flags = parse_room_flags(flags)?;
        let Some(room) = self.rooms.get_mut(&current) else {
            return Err(format!("Current room {} is missing.", current));
        };
        let mut updates = Vec::new();
        for flag in flags {
            set_membership(&mut room.flags, flag, enabled);
            updates.push((flag, room.flags.contains(&flag)));
        }
        Ok(updates)
    }

    fn set_exit_flag(
        &mut self,
        direction: &str,
        flag: &str,
        enabled: Option<bool>,
    ) -> Result<(), String> {
        let Some(current) = self.current_room.clone() else {
            return Err("You are not inside the map.".to_string());
        };
        let flag = parse_exit_flag(flag)?;
        let direction = normalize_direction(direction);
        let Some(exit) = self
            .rooms
            .get_mut(&current)
            .and_then(|room| room.exits.get_mut(&direction))
        else {
            return Err(format!("No mapped exit {}.", direction));
        };
        set_membership(&mut exit.flags, flag, enabled);
        Ok(())
    }

    fn set_door(
        &mut self,
        direction: &str,
        state: Option<DoorState>,
        name: Option<String>,
    ) -> Result<(), String> {
        let Some(current) = self.current_room.clone() else {
            return Err("You are not inside the map.".to_string());
        };
        let direction = normalize_direction(direction);
        let Some(exit) = self
            .rooms
            .get_mut(&current)
            .and_then(|room| room.exits.get_mut(&direction))
        else {
            return Err(format!("No mapped exit {}.", direction));
        };
        exit.door = state;
        if state.is_none() {
            exit.door_name = None;
        } else if let Some(name) = name {
            exit.door_name = Some(name);
        }
        Ok(())
    }

    pub fn mud_commands_for_movement(&self, command: &str) -> Vec<String> {
        let Some(direction) = is_movement_command(command) else {
            return vec![command.to_string()];
        };
        let Some(exit) = self
            .current_room
            .as_ref()
            .and_then(|room_id| self.rooms.get(room_id))
            .and_then(|room| room.exits.get(&direction))
        else {
            return vec![command.to_string()];
        };
        if let Some(name) = non_empty_text(exit.door_name.as_deref().unwrap_or_default()) {
            match exit.door {
                Some(DoorState::Closed) => {
                    return vec![format!("open {name} {direction}"), direction];
                }
                Some(DoorState::Pickable) => {
                    return vec![format!("pick {name} {direction}"), direction];
                }
                Some(DoorState::Locked) => {
                    return vec![
                        format!("unlock {name} {direction}"),
                        format!("open {name} {direction}"),
                        direction,
                    ];
                }
                _ => {}
            }
        }
        vec![command.to_string()]
    }

    pub fn read_from_path(&mut self, path: impl AsRef<Path>) -> Result<(), String> {
        let path = path.as_ref();
        let raw = fs::read_to_string(path).map_err(|error| error.to_string())?;
        let loaded: Self = toml::from_str(&raw).map_err(|error| error.to_string())?;
        *self = loaded;
        Ok(())
    }

    pub fn write_to_path(&self, path: impl AsRef<Path>) -> Result<(), String> {
        let path = path.as_ref();
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let raw = toml::to_string_pretty(self).map_err(|error| error.to_string())?;
        fs::write(path, raw).map_err(|error| error.to_string())
    }

    fn find_path(&self, query: &str) -> Result<Vec<String>, String> {
        if query.trim().is_empty() {
            return Err("usage: /map find <vnum|name>".to_string());
        }
        self.shortest_path_to(query)
            .ok_or_else(|| format!("No path found to `{}`.", query))
    }

    fn list(&self, query: &str) -> String {
        let query = query.to_ascii_lowercase();
        let mut lines = Vec::new();
        for room in self.rooms.values() {
            let haystack = format!(
                "{} {} {} {} {} {}",
                room.id, room.name, room.area, room.description, room.note, room.terrain
            )
            .to_ascii_lowercase();
            if query.is_empty() || haystack.contains(&query) {
                lines.push(format!(
                    "{} {} [{}] exits:{}",
                    room.id,
                    room.name_or_default(),
                    room.area,
                    room.exits.keys().cloned().collect::<Vec<_>>().join(",")
                ));
            }
            if lines.len() >= 12 {
                lines.push("...".to_string());
                break;
            }
        }
        if lines.is_empty() {
            "No matching rooms.".to_string()
        } else {
            lines.join("\n")
        }
    }

    fn info(&self) -> String {
        let Some(current) = self.current_room.as_ref() else {
            return "Map inactive.".to_string();
        };
        let Some(room) = self.rooms.get(current) else {
            return format!("Map room {} is missing.", current);
        };
        format!(
            "Room {}: {}\nCoords: {},{},{}\nArea: {}\nTerrain: {}\nWeight: {}\nExits: {}",
            room.id,
            room.name_or_default(),
            room.x,
            room.y,
            room.z,
            room.area,
            room.terrain_or_default(),
            room.weight,
            room.exits.keys().cloned().collect::<Vec<_>>().join(", ")
        )
    }

    fn help(&self) -> MapCommandResult {
        self.message("# Map Commands\n\n## Display\n- `/map map` - show a full MUD-output-pane map centered on the current room\n- `/map get` / `/map info` - show current room details\n- `/map list [query]` - list mapped rooms\n\n## Movement and Rooms\n- `/map create` - create a new map\n- `/map goto <vnum|name> [dig]` - select a room\n- `/map move <direction>` - move only the mapper\n- `/map dig <direction> [new|vnum]` - create and link a room\n- `/map link <direction> <vnum> [both]` - link rooms\n- `/map unlink <direction>` - remove an exit link\n- `/map delete <direction|vnum>` - delete an exit or room\n- `/map undo` - undo the last mapper-created move\n- `/map return` - restore the previous mapper room\n- `/map leave` - leave the map while remembering the previous room\n\n## Metadata and Flags\n- `/map set <option> <value>` - set room metadata\n- `/map flag <name> [on|off]` - toggle mapper-wide flags\n- `/map roomflag [flag[;flag...] [on|off|get <variable>]]` - list or update current-room flags\n- `/map exitflag <direction> <flag> [on|off]` - toggle an exit flag\n- `/map door <direction> [state|none] [name]` - mark or clear an exit door\n\n## Paths and Files\n- `/map find <vnum|name>` - show a weighted path\n- `/map run <vnum|name>` - send a weighted path\n- `/map read <file>` - load map TOML\n- `/map write <file>` - save map TOML\n\nUse `/help map` for the complete command reference.")
    }

    fn message(&self, message: impl Into<String>) -> MapCommandResult {
        MapCommandResult {
            message: message.into(),
            commands: Vec::new(),
            show_map: false,
            variable: None,
        }
    }

    fn current_label(&self) -> String {
        self.current_room
            .as_deref()
            .and_then(|id| self.rooms.get(id))
            .map(|room| format!("{} ({})", room.name_or_default(), room.id))
            .unwrap_or_else(|| "none".to_string())
    }

    fn resolve_room(&self, target: &str) -> Option<String> {
        if self.rooms.contains_key(target) {
            return Some(target.to_string());
        }
        let query = target.to_ascii_lowercase();
        self.rooms
            .values()
            .find(|room| room.name.to_ascii_lowercase() == query)
            .map(|room| room.id.clone())
            .or_else(|| {
                self.rooms
                    .values()
                    .find(|room| room.name.to_ascii_lowercase().contains(&query))
                    .map(|room| room.id.clone())
            })
    }

    fn new_room_with_id(&mut self, id: &str) -> String {
        self.rooms.entry(id.to_string()).or_insert_with(|| Room {
            id: id.to_string(),
            ..Room::default()
        });
        if let Ok(number) = id.parse::<u64>() {
            self.next_generated_id = self.next_generated_id.max(number.saturating_add(1));
        }
        id.to_string()
    }

    fn next_id(&mut self) -> String {
        while self.rooms.contains_key(&self.next_generated_id.to_string()) {
            self.next_generated_id = self.next_generated_id.saturating_add(1);
        }
        let id = self.next_generated_id.to_string();
        self.next_generated_id = self.next_generated_id.saturating_add(1);
        id
    }

    fn set_exit(&mut self, from: &str, direction: &str, to: Option<String>) {
        if let Some(room) = self.rooms.get_mut(from) {
            room.exits.insert(
                direction.to_string(),
                Exit {
                    direction: direction.to_string(),
                    to,
                    ..room.exits.get(direction).cloned().unwrap_or_default()
                },
            );
        }
    }

    fn room_has_flag(&self, id: &str, flag: RoomFlag) -> bool {
        self.rooms
            .get(id)
            .is_some_and(|room| room.flags.contains(&flag))
    }

    fn set_current_name(&mut self, name: &str) {
        if let Some(current) = &self.current_room
            && let Some(room) = self.rooms.get_mut(current)
        {
            room.name = name.to_string();
        }
    }

    fn next_coordinate_from_current(&self) -> Option<(i32, i32, i32)> {
        let room = self.rooms.get(self.current_room.as_ref()?)?;
        Some((room.x, room.y, room.z))
    }
}

impl Room {
    pub fn name_or_default(&self) -> &str {
        if self.name.is_empty() {
            "Unnamed"
        } else {
            &self.name
        }
    }

    fn terrain_or_default(&self) -> &str {
        if self.terrain.is_empty() {
            "(none)"
        } else {
            &self.terrain
        }
    }

    fn set_terrain(&mut self, terrain: String) {
        self.weight = default_room_weight_for_terrain(&terrain).unwrap_or(self.weight);
        self.terrain = terrain;
    }

    fn path_weight(&self) -> f32 {
        if self.flags.contains(&RoomFlag::Void) {
            return 0.000_001;
        }
        if self.weight.is_finite() && self.weight > 0.0 {
            self.weight
        } else {
            1.0
        }
    }
}

fn default_room_weight_for_terrain(terrain: &str) -> Option<f32> {
    match normalized_terrain_name(terrain).as_str() {
        "floor" => Some(1.0),
        "road" => Some(2.0),
        "field" => Some(3.0),
        "forest" => Some(4.0),
        "hills" => Some(5.0),
        "denseforest" => Some(6.0),
        "mountain" | "mountains" => Some(7.0),
        "swamp" => Some(8.0),
        "water" => Some(9.0),
        "waternoswim" => Some(10.0),
        "underwater" => Some(11.0),
        "crack" => Some(12.0),
        _ => None,
    }
}

fn normalized_terrain_name(terrain: &str) -> String {
    terrain
        .chars()
        .filter(|value| value.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

pub fn parse_room_id(value: &MsdpValue) -> Option<String> {
    value
        .as_string()
        .map(ToOwned::to_owned)
        .or_else(|| value.as_i64().map(|number| number.to_string()))
        .filter(|value| !value.trim().is_empty())
}

pub fn parse_room_exits(value: &MsdpValue) -> Vec<String> {
    match value {
        MsdpValue::String(value) => value
            .split([',', ';', ' '])
            .filter(|part| !part.trim().is_empty())
            .map(normalize_direction)
            .collect(),
        MsdpValue::Array(values) => values
            .iter()
            .filter_map(|value| value.as_string())
            .map(normalize_direction)
            .collect(),
        MsdpValue::Table(table) => table.keys().map(normalize_direction).collect(),
    }
}

pub fn parse_room_exit_targets(value: &MsdpValue) -> Vec<String> {
    match value {
        MsdpValue::String(value) => value
            .split([',', ';', ' '])
            .filter_map(non_empty_owned)
            .collect(),
        MsdpValue::Array(values) => values
            .iter()
            .filter_map(parse_room_id)
            .filter_map(|value| non_empty_owned(&value))
            .collect(),
        MsdpValue::Table(table) => table
            .values()
            .filter_map(parse_room_id)
            .filter_map(|value| non_empty_owned(&value))
            .collect(),
    }
}

pub fn parse_room_exit_updates(
    directions: Vec<String>,
    target_value: Option<&MsdpValue>,
) -> Vec<RoomExitUpdate> {
    let Some(target_value) = target_value else {
        return merge_room_exits(directions, Vec::new());
    };
    if let MsdpValue::Table(table) = target_value {
        let targets_by_direction = table
            .iter()
            .map(|(direction, target)| {
                (
                    normalize_direction(direction),
                    parse_room_id(target).and_then(|value| non_empty_owned(&value)),
                )
            })
            .collect::<BTreeMap<_, _>>();
        if directions.is_empty() {
            return targets_by_direction
                .into_iter()
                .map(|(direction, to)| RoomExitUpdate { direction, to })
                .collect();
        }
        return directions
            .into_iter()
            .map(|direction| {
                let direction = normalize_direction(direction);
                let to = targets_by_direction.get(&direction).cloned().flatten();
                RoomExitUpdate { direction, to }
            })
            .collect();
    }
    merge_room_exits(directions, parse_room_exit_targets(target_value))
}

pub fn merge_room_exits(directions: Vec<String>, targets: Vec<String>) -> Vec<RoomExitUpdate> {
    directions
        .into_iter()
        .enumerate()
        .map(|(index, direction)| RoomExitUpdate {
            direction,
            to: targets.get(index).cloned(),
        })
        .collect()
}

fn non_empty_owned(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

pub fn is_movement_command(command: &str) -> Option<String> {
    let trimmed = command.trim();
    if trimmed.split_whitespace().count() != 1 {
        return None;
    }
    direction_by_name(trimmed).map(|direction| direction.name.to_string())
}

fn required<'a>(value: Option<&'a str>, usage: &str) -> Result<&'a str, String> {
    value.ok_or_else(|| usage.to_string())
}

fn parse_toggle(value: Option<&str>) -> Option<bool> {
    match value {
        Some(value)
            if matches!(
                value.to_ascii_lowercase().as_str(),
                "on" | "true" | "yes" | "1"
            ) =>
        {
            Some(true)
        }
        Some(value)
            if matches!(
                value.to_ascii_lowercase().as_str(),
                "off" | "false" | "no" | "0"
            ) =>
        {
            Some(false)
        }
        _ => None,
    }
}

fn set_membership<T: Ord>(set: &mut BTreeSet<T>, value: T, enabled: Option<bool>) {
    match enabled {
        Some(true) => {
            set.insert(value);
        }
        Some(false) => {
            set.remove(&value);
        }
        None if set.contains(&value) => {
            set.remove(&value);
        }
        None => {
            set.insert(value);
        }
    }
}

const ROOM_FLAGS: [RoomFlag; 10] = [
    RoomFlag::Avoid,
    RoomFlag::Block,
    RoomFlag::Curved,
    RoomFlag::Fog,
    RoomFlag::Hide,
    RoomFlag::Invis,
    RoomFlag::Leave,
    RoomFlag::NoGlobal,
    RoomFlag::Static,
    RoomFlag::Void,
];

impl RoomFlag {
    fn name(self) -> &'static str {
        match self {
            RoomFlag::Avoid => "avoid",
            RoomFlag::Block => "block",
            RoomFlag::Curved => "curved",
            RoomFlag::Fog => "fog",
            RoomFlag::Hide => "hide",
            RoomFlag::Invis => "invis",
            RoomFlag::Leave => "leave",
            RoomFlag::NoGlobal => "noglobal",
            RoomFlag::Static => "static",
            RoomFlag::Void => "void",
        }
    }
}

fn parse_room_flags(flags: &str) -> Result<Vec<RoomFlag>, String> {
    let flags = flags
        .split(';')
        .map(str::trim)
        .filter(|flag| !flag.is_empty())
        .map(parse_room_flag)
        .collect::<Result<Vec<_>, _>>()?;
    if flags.is_empty() {
        return Err("usage: /map roomflag <flag> [on|off]".to_string());
    }
    Ok(flags)
}

fn parse_room_flag(flag: &str) -> Result<RoomFlag, String> {
    let flag = flag.to_ascii_lowercase();
    let candidates = [
        ("avoid", RoomFlag::Avoid),
        ("block", RoomFlag::Block),
        ("curved", RoomFlag::Curved),
        ("fog", RoomFlag::Fog),
        ("hide", RoomFlag::Hide),
        ("invis", RoomFlag::Invis),
        ("invisible", RoomFlag::Invis),
        ("leave", RoomFlag::Leave),
        ("noglobal", RoomFlag::NoGlobal),
        ("static", RoomFlag::Static),
        ("void", RoomFlag::Void),
    ];
    let matches = candidates
        .iter()
        .filter(|(name, _)| name.starts_with(&flag))
        .map(|(_, flag)| *flag)
        .collect::<BTreeSet<_>>();
    if matches.len() == 1 {
        Ok(*matches.iter().next().expect("one flag"))
    } else if matches.is_empty() {
        Err(format!("Unknown room flag `{}`.", flag))
    } else {
        Err(format!("Ambiguous room flag `{}`.", flag))
    }
}

fn room_flag_action_matches(value: &str, expected: &str) -> bool {
    expected.starts_with(&value.to_ascii_lowercase())
}

fn room_flag_update_message(updates: &[(RoomFlag, bool)]) -> String {
    let mut message = "# Room Flags".to_string();
    for (flag, enabled) in updates {
        message.push_str(&format!(
            "\n- `{}`: {}",
            flag.name(),
            if *enabled { "ON" } else { "OFF" }
        ));
    }
    message
}

fn parse_exit_flag(flag: &str) -> Result<ExitFlag, String> {
    match flag {
        "avoid" => Ok(ExitFlag::Avoid),
        "block" => Ok(ExitFlag::Block),
        "hide" => Ok(ExitFlag::Hide),
        "invis" => Ok(ExitFlag::Invis),
        "teleport" | "tp" => Ok(ExitFlag::Teleport),
        _ => Err(format!(
            "Unknown exit flag `{}`. Use avoid, block, hide, invis, or teleport.",
            flag
        )),
    }
}

fn parse_door_state(state: &str) -> Result<Option<DoorState>, String> {
    match state.trim().to_ascii_lowercase().as_str() {
        "none" | "clear" | "off" => Ok(None),
        "open" => Ok(Some(DoorState::Open)),
        "closed" => Ok(Some(DoorState::Closed)),
        "pickable" => Ok(Some(DoorState::Pickable)),
        "locked" => Ok(Some(DoorState::Locked)),
        "trigger" => Ok(Some(DoorState::Trigger)),
        "unknown" => Ok(Some(DoorState::Unknown)),
        _ => Err(format!(
            "Unknown door state `{}`. Use trigger, unknown, open, closed, pickable, locked, or none.",
            state
        )),
    }
}

fn parse_door_args(values: &[&str]) -> Result<(Option<DoorState>, Option<String>), String> {
    let Some(first) = values.first() else {
        return Ok((Some(DoorState::Closed), None));
    };
    match parse_door_state(first) {
        Ok(state) => Ok((state, non_empty_text(&values[1..].join(" ")))),
        Err(_) => Ok((Some(DoorState::Closed), non_empty_text(&values.join(" ")))),
    }
}

fn non_empty_text(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn normalize_direction(value: impl AsRef<str>) -> String {
    match value.as_ref().trim().to_ascii_lowercase().as_str() {
        "north" => "n",
        "northeast" => "ne",
        "east" => "e",
        "southeast" => "se",
        "south" => "s",
        "southwest" => "sw",
        "west" => "w",
        "northwest" => "nw",
        "up" => "u",
        "down" => "d",
        value => value,
    }
    .to_string()
}

fn direction_by_name(value: &str) -> Option<Direction> {
    let direction = normalize_direction(value);
    DIRECTIONS
        .iter()
        .copied()
        .find(|candidate| candidate.name == direction)
}

fn format_path(path: &[String]) -> String {
    if path.is_empty() {
        "Already there.".to_string()
    } else {
        format!("Path: {}", path.join(" "))
    }
}

const DIRECTIONS: &[Direction] = &[
    Direction {
        name: "n",
        reverse: "s",
        dx: 0,
        dy: -1,
        dz: 0,
    },
    Direction {
        name: "ne",
        reverse: "sw",
        dx: 1,
        dy: -1,
        dz: 0,
    },
    Direction {
        name: "e",
        reverse: "w",
        dx: 1,
        dy: 0,
        dz: 0,
    },
    Direction {
        name: "se",
        reverse: "nw",
        dx: 1,
        dy: 1,
        dz: 0,
    },
    Direction {
        name: "s",
        reverse: "n",
        dx: 0,
        dy: 1,
        dz: 0,
    },
    Direction {
        name: "sw",
        reverse: "ne",
        dx: -1,
        dy: 1,
        dz: 0,
    },
    Direction {
        name: "w",
        reverse: "e",
        dx: -1,
        dy: 0,
        dz: 0,
    },
    Direction {
        name: "nw",
        reverse: "se",
        dx: -1,
        dy: -1,
        dz: 0,
    },
    Direction {
        name: "u",
        reverse: "d",
        dx: 0,
        dy: 0,
        dz: 1,
    },
    Direction {
        name: "d",
        reverse: "u",
        dx: 0,
        dy: 0,
        dz: -1,
    },
];

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn map_command_requests_full_output_snapshot() {
        let mut map = MapState::default();

        let result = map.execute("map").expect("map command should be accepted");

        assert!(result.show_map);
        assert!(result.message.is_empty());
        assert!(result.commands.is_empty());
    }

    #[test]
    fn bare_map_help_uses_markdown_structure() {
        let mut map = MapState::default();

        let result = map.execute("").expect("map help should be returned");

        assert!(result.message.starts_with("# Map Commands\n"));
        assert!(result.message.contains("## Display"));
        assert!(result.message.contains("- `/map map`"));
        assert!(!result.show_map);
    }

    #[test]
    fn movement_creates_and_links_rooms() {
        let mut map = MapState::default();
        map.create();

        map.move_direction("n").unwrap();

        assert_eq!(map.current_room.as_deref(), Some("2"));
        assert_eq!(
            map.rooms
                .get("1")
                .and_then(|room| room.exits.get("n"))
                .and_then(|exit| exit.to.as_deref()),
            Some("2")
        );
        assert_eq!(
            map.rooms
                .get("2")
                .and_then(|room| room.exits.get("s"))
                .and_then(|exit| exit.to.as_deref()),
            Some("1")
        );
    }

    #[test]
    fn undo_removes_created_room() {
        let mut map = MapState::default();
        map.create();
        map.move_direction("e").unwrap();

        map.execute("undo").unwrap();

        assert_eq!(map.current_room.as_deref(), Some("1"));
        assert!(!map.rooms.contains_key("2"));
        assert!(!map.rooms["1"].exits.contains_key("e"));
    }

    #[test]
    fn shortest_path_uses_mapped_exits() {
        let mut map = MapState::default();
        map.create();
        map.move_direction("n").unwrap();
        map.move_direction("e").unwrap();

        map.execute("goto 1").unwrap();

        assert_eq!(
            map.shortest_path_to("3"),
            Some(vec!["n".to_string(), "e".to_string()])
        );
    }

    #[test]
    fn roomflag_lists_and_sets_tintin_flags() {
        let mut map = MapState::default();
        map.create();

        let list = map.execute("roomflag").unwrap();
        assert!(list.message.contains("`curved`: OFF"));
        assert!(list.message.contains("`fog`: OFF"));

        let result = map.execute("roomflag avoid;fog;invisible on").unwrap();

        assert!(result.message.contains("`avoid`: ON"));
        assert!(result.message.contains("`fog`: ON"));
        assert!(result.message.contains("`invis`: ON"));
        let flags = &map.rooms["1"].flags;
        assert!(flags.contains(&RoomFlag::Avoid));
        assert!(flags.contains(&RoomFlag::Fog));
        assert!(flags.contains(&RoomFlag::Invis));
    }

    #[test]
    fn roomflag_get_returns_runtime_variable_update() {
        let mut map = MapState::default();
        map.create();
        map.execute("roomflag block on").unwrap();

        let result = map.execute("roomflag block get blocked").unwrap();

        assert_eq!(
            result.variable,
            Some(("blocked".to_string(), "1".to_string()))
        );
    }

    #[test]
    fn link_command_creates_one_way_exit_to_target_room() {
        let mut map = MapState::default();
        map.create();

        let result = map.execute("link e 12").unwrap();

        assert_eq!(result.message, "Linked e to 12.");
        assert!(map.rooms.contains_key("12"));
        assert_eq!(
            map.rooms["1"].exits["e"],
            Exit {
                direction: "e".to_string(),
                to: Some("12".to_string()),
                ..Exit::default()
            }
        );
        assert!(!map.rooms["12"].exits.contains_key("w"));
    }

    #[test]
    fn link_command_with_both_creates_reverse_exit() {
        let mut map = MapState::default();
        map.create();

        map.execute("link south 12 both").unwrap();

        assert_eq!(map.rooms["1"].exits["s"].to.as_deref(), Some("12"));
        assert_eq!(map.rooms["12"].exits["n"].to.as_deref(), Some("1"));
    }

    #[test]
    fn shortest_path_prefers_lower_room_weight() {
        let mut map = MapState {
            current_room: Some("1".to_string()),
            ..MapState::default()
        };
        for id in ["1", "2", "3", "4", "5"] {
            map.rooms.insert(
                id.to_string(),
                Room {
                    id: id.to_string(),
                    weight: 1.0,
                    ..Room::default()
                },
            );
        }
        map.rooms.get_mut("2").unwrap().weight = 12.0;
        map.rooms.get_mut("1").unwrap().exits.insert(
            "e".to_string(),
            Exit {
                direction: "e".to_string(),
                to: Some("2".to_string()),
                ..Exit::default()
            },
        );
        map.rooms.get_mut("2").unwrap().exits.insert(
            "e".to_string(),
            Exit {
                direction: "e".to_string(),
                to: Some("5".to_string()),
                ..Exit::default()
            },
        );
        map.rooms.get_mut("1").unwrap().exits.insert(
            "n".to_string(),
            Exit {
                direction: "n".to_string(),
                to: Some("3".to_string()),
                ..Exit::default()
            },
        );
        map.rooms.get_mut("3").unwrap().exits.insert(
            "e".to_string(),
            Exit {
                direction: "e".to_string(),
                to: Some("4".to_string()),
                ..Exit::default()
            },
        );
        map.rooms.get_mut("4").unwrap().exits.insert(
            "s".to_string(),
            Exit {
                direction: "s".to_string(),
                to: Some("5".to_string()),
                ..Exit::default()
            },
        );

        assert_eq!(
            map.shortest_path_to("5"),
            Some(vec!["n".to_string(), "e".to_string(), "s".to_string()])
        );
    }

    #[test]
    fn void_room_has_tiny_path_weight() {
        let room = Room {
            flags: BTreeSet::from([RoomFlag::Void]),
            weight: 100.0,
            ..Room::default()
        };

        assert_eq!(room.path_weight(), 0.000_001);
    }

    #[test]
    fn shortest_path_tunnels_through_void_rooms() {
        let mut map = MapState {
            current_room: Some("1".to_string()),
            ..MapState::default()
        };
        map.rooms
            .insert("1".to_string(), room_with_exits("1", [("e", "2")]));
        map.rooms.insert(
            "2".to_string(),
            Room {
                flags: BTreeSet::from([RoomFlag::Void]),
                ..room_with_exits("2", [("w", "1"), ("e", "3")])
            },
        );
        map.rooms.insert("3".to_string(), room_with_exits("3", []));

        assert_eq!(map.shortest_path_to("3"), Some(vec!["e".to_string()]));
    }

    fn room_with_exits<const N: usize>(id: &str, exits: [(&str, &str); N]) -> Room {
        Room {
            id: id.to_string(),
            exits: exits
                .into_iter()
                .map(|(direction, to)| {
                    (
                        direction.to_string(),
                        Exit {
                            direction: direction.to_string(),
                            to: Some(to.to_string()),
                            ..Exit::default()
                        },
                    )
                })
                .collect(),
            ..Room::default()
        }
    }

    #[test]
    fn leave_flag_exits_map_after_movement_enters_room() {
        let mut map = MapState::default();
        map.create();
        map.execute("dig e 2").unwrap();
        map.rooms
            .get_mut("2")
            .unwrap()
            .flags
            .insert(RoomFlag::Leave);

        map.move_direction("e").unwrap();

        assert_eq!(map.current_room, None);
        assert_eq!(map.previous_room.as_deref(), Some("2"));
    }

    #[test]
    fn map_run_uses_weight_optimized_path() {
        let mut map = MapState {
            current_room: Some("1".to_string()),
            ..MapState::default()
        };
        for id in ["1", "2", "3", "4", "5"] {
            map.rooms.insert(
                id.to_string(),
                Room {
                    id: id.to_string(),
                    weight: 1.0,
                    ..Room::default()
                },
            );
        }
        map.rooms.get_mut("2").unwrap().weight = 12.0;
        map.rooms.get_mut("1").unwrap().exits.insert(
            "e".to_string(),
            Exit {
                direction: "e".to_string(),
                to: Some("2".to_string()),
                ..Exit::default()
            },
        );
        map.rooms.get_mut("2").unwrap().exits.insert(
            "e".to_string(),
            Exit {
                direction: "e".to_string(),
                to: Some("5".to_string()),
                ..Exit::default()
            },
        );
        map.rooms.get_mut("1").unwrap().exits.insert(
            "n".to_string(),
            Exit {
                direction: "n".to_string(),
                to: Some("3".to_string()),
                ..Exit::default()
            },
        );
        map.rooms.get_mut("3").unwrap().exits.insert(
            "e".to_string(),
            Exit {
                direction: "e".to_string(),
                to: Some("4".to_string()),
                ..Exit::default()
            },
        );
        map.rooms.get_mut("4").unwrap().exits.insert(
            "s".to_string(),
            Exit {
                direction: "s".to_string(),
                to: Some("5".to_string()),
                ..Exit::default()
            },
        );

        let result = map.execute("run 5").unwrap();

        assert_eq!(
            result.commands,
            vec!["n".to_string(), "e".to_string(), "s".to_string()]
        );
        assert_eq!(result.message, "Path: n e s");
    }

    #[test]
    fn terrain_sets_default_room_weight() {
        let mut map = MapState::default();
        map.create();

        map.execute("set roomterrain Dense Forest").unwrap();

        assert_eq!(map.rooms["1"].terrain, "Dense Forest");
        assert_eq!(map.rooms["1"].weight, 6.0);
    }

    #[test]
    fn msdp_terrain_sync_sets_room_weight() {
        let mut map = MapState::default();
        map.sync_room(Some("100".to_string()), None, Vec::new(), true);

        map.sync_current_room_terrain(Some("Water Noswim".to_string()));

        assert_eq!(map.rooms["100"].terrain, "Water Noswim");
        assert_eq!(map.rooms["100"].weight, 10.0);
    }

    #[test]
    fn terrain_weight_aliases_match_rots_names() {
        assert_eq!(default_room_weight_for_terrain("Floor"), Some(1.0));
        assert_eq!(default_room_weight_for_terrain("Road"), Some(2.0));
        assert_eq!(default_room_weight_for_terrain("Field"), Some(3.0));
        assert_eq!(default_room_weight_for_terrain("Forest"), Some(4.0));
        assert_eq!(default_room_weight_for_terrain("Hills"), Some(5.0));
        assert_eq!(default_room_weight_for_terrain("Dense Forest"), Some(6.0));
        assert_eq!(default_room_weight_for_terrain("Mountains"), Some(7.0));
        assert_eq!(default_room_weight_for_terrain("Swamp"), Some(8.0));
        assert_eq!(default_room_weight_for_terrain("Water"), Some(9.0));
        assert_eq!(default_room_weight_for_terrain("Water Noswim"), Some(10.0));
        assert_eq!(default_room_weight_for_terrain("Underwater"), Some(11.0));
        assert_eq!(default_room_weight_for_terrain("Crack"), Some(12.0));
    }

    #[test]
    fn map_get_includes_terrain_and_weight() {
        let mut map = MapState::default();
        map.create();
        map.execute("set roomterrain Forest").unwrap();
        map.execute("set roomweight 2.5").unwrap();

        let result = map.execute("get").unwrap();

        assert!(result.message.contains("Terrain: Forest"));
        assert!(result.message.contains("Weight: 2.5"));
    }

    #[test]
    fn door_command_sets_exit_door_state() {
        let mut map = MapState::default();
        map.create();
        map.execute("dig e").unwrap();

        map.execute("door e trigger").unwrap();

        assert_eq!(map.rooms["1"].exits["e"].door, Some(DoorState::Trigger));
    }

    #[test]
    fn door_command_stores_directional_name() {
        let mut map = MapState::default();
        map.create();
        map.execute("dig e").unwrap();
        map.execute("goto 2").unwrap();
        map.execute("link w 1").unwrap();
        map.execute("goto 1").unwrap();

        map.execute("door e closed oak gate").unwrap();
        map.execute("goto 2").unwrap();
        map.execute("door w closed iron hatch").unwrap();

        assert_eq!(
            map.rooms["1"].exits["e"].door_name.as_deref(),
            Some("oak gate")
        );
        assert_eq!(
            map.rooms["2"].exits["w"].door_name.as_deref(),
            Some("iron hatch")
        );
    }

    #[test]
    fn closed_named_door_expands_movement_commands() {
        let mut map = MapState::default();
        map.create();
        map.execute("dig w").unwrap();
        map.execute("door w closed stone door").unwrap();

        assert_eq!(
            map.mud_commands_for_movement("west"),
            vec!["open stone door w".to_string(), "w".to_string()]
        );

        map.execute("door w open stone door").unwrap();
        assert_eq!(
            map.mud_commands_for_movement("west"),
            vec!["west".to_string()]
        );
    }

    #[test]
    fn pickable_named_door_picks_before_movement() {
        let mut map = MapState::default();
        map.create();
        map.execute("dig w").unwrap();
        map.execute("door w pickable stone door").unwrap();

        assert_eq!(
            map.mud_commands_for_movement("west"),
            vec!["pick stone door w".to_string(), "w".to_string()]
        );
    }

    #[test]
    fn locked_named_door_unlocks_and_opens_before_movement() {
        let mut map = MapState::default();
        map.create();
        map.execute("dig w").unwrap();
        map.execute("door w locked stone door").unwrap();

        assert_eq!(
            map.mud_commands_for_movement("west"),
            vec![
                "unlock stone door w".to_string(),
                "open stone door w".to_string(),
                "w".to_string()
            ]
        );
    }

    #[test]
    fn door_command_defaults_to_closed_when_state_is_omitted() {
        let mut map = MapState::default();
        map.create();
        map.execute("dig w").unwrap();

        map.execute("door w").unwrap();

        assert_eq!(map.rooms["1"].exits["w"].door, Some(DoorState::Closed));
    }

    #[test]
    fn door_command_accepts_close_and_none() {
        let mut map = MapState::default();
        map.create();
        map.execute("dig e").unwrap();

        map.execute("door e closed").unwrap();
        assert_eq!(map.rooms["1"].exits["e"].door, Some(DoorState::Closed));

        map.execute("door e none").unwrap();
        assert_eq!(map.rooms["1"].exits["e"].door, None);
    }

    #[test]
    fn exitflag_command_accepts_teleport() {
        let mut map = MapState::default();
        map.create();
        map.execute("dig e").unwrap();

        map.execute("exitflag e teleport on").unwrap();
        assert!(
            map.rooms["1"].exits["e"]
                .flags
                .contains(&ExitFlag::Teleport)
        );

        map.execute("exitflag e teleport off").unwrap();
        assert!(
            !map.rooms["1"].exits["e"]
                .flags
                .contains(&ExitFlag::Teleport)
        );
    }

    #[test]
    fn msdp_sync_updates_room_metadata() {
        let mut map = MapState::default();

        map.sync_room(
            Some("100".to_string()),
            Some("Grassy Scrubland".to_string()),
            merge_room_exits(vec!["n".to_string(), "east".to_string()], Vec::new()),
            true,
        );

        let room = &map.rooms["100"];
        assert_eq!(map.current_room.as_deref(), Some("100"));
        assert_eq!(room.name, "Grassy Scrubland");
        assert!(room.exits.contains_key("n"));
        assert!(room.exits.contains_key("e"));
    }

    #[test]
    fn msdp_sync_links_exits_to_reported_target_rooms() {
        let mut map = MapState::default();

        map.sync_room(
            Some("100".to_string()),
            Some("Grassy Scrubland".to_string()),
            merge_room_exits(
                vec!["n".to_string(), "e".to_string()],
                vec!["101".to_string(), "102".to_string()],
            ),
            true,
        );

        assert_eq!(map.rooms["100"].exits["n"].to.as_deref(), Some("101"));
        assert_eq!(map.rooms["100"].exits["e"].to.as_deref(), Some("102"));
        assert_eq!(map.rooms["101"].exits["s"].to.as_deref(), Some("100"));
        assert_eq!(map.rooms["102"].exits["w"].to.as_deref(), Some("100"));
        assert_eq!((map.rooms["101"].x, map.rooms["101"].y), (0, -1));
        assert_eq!((map.rooms["102"].x, map.rooms["102"].y), (1, 0));
        assert!(map.rooms.contains_key("101"));
        assert!(map.rooms.contains_key("102"));
    }

    #[test]
    fn msdp_sync_replaces_stale_current_room_exits() {
        let mut map = MapState::default();

        map.sync_room(
            Some("2811".to_string()),
            Some("Grassland".to_string()),
            merge_room_exits(
                vec![
                    "e".to_string(),
                    "n".to_string(),
                    "s".to_string(),
                    "w".to_string(),
                ],
                Vec::new(),
            ),
            true,
        );

        map.sync_room(
            Some("2811".to_string()),
            Some("Grassland".to_string()),
            merge_room_exits(
                vec!["e".to_string(), "s".to_string(), "w".to_string()],
                Vec::new(),
            ),
            true,
        );

        assert!(map.rooms["2811"].exits.contains_key("e"));
        assert!(!map.rooms["2811"].exits.contains_key("n"));
        assert!(map.rooms["2811"].exits.contains_key("s"));
        assert!(map.rooms["2811"].exits.contains_key("w"));
        assert_eq!(
            map.rooms["2811"].exit_order,
            vec!["e".to_string(), "s".to_string(), "w".to_string()]
        );
    }

    #[test]
    fn msdp_table_targets_do_not_add_unreported_exit_directions() {
        let mut targets = HashMap::new();
        targets.insert("n".to_string(), MsdpValue::String("100".to_string()));
        targets.insert("s".to_string(), MsdpValue::String("101".to_string()));

        let exits =
            parse_room_exit_updates(vec!["s".to_string()], Some(&MsdpValue::Table(targets)));

        assert_eq!(exits.len(), 1);
        assert_eq!(exits[0].direction, "s");
        assert_eq!(exits[0].to.as_deref(), Some("101"));
    }

    #[test]
    fn msdp_metadata_sync_does_not_replace_current_room_exits() {
        let mut map = MapState::default();

        map.sync_room(
            Some("2800".to_string()),
            Some("Dirt Trail".to_string()),
            merge_room_exits(
                vec!["n".to_string(), "s".to_string(), "w".to_string()],
                Vec::new(),
            ),
            true,
        );

        map.sync_room(
            Some("2800".to_string()),
            Some("Dirt Trail".to_string()),
            Vec::new(),
            false,
        );

        assert!(map.rooms["2800"].exits.contains_key("n"));
        assert!(map.rooms["2800"].exits.contains_key("s"));
        assert!(map.rooms["2800"].exits.contains_key("w"));
    }

    #[test]
    fn msdp_sync_adopts_speculative_move_room_id() {
        let mut map = MapState::default();
        map.create();

        map.move_direction("n").unwrap();
        assert_eq!(map.current_room.as_deref(), Some("2"));

        map.sync_room(
            Some("2906".to_string()),
            Some("Northern Room".to_string()),
            merge_room_exits(Vec::new(), Vec::new()),
            true,
        );

        assert!(!map.rooms.contains_key("2"));
        assert_eq!(map.current_room.as_deref(), Some("2906"));
        assert_eq!(map.rooms["1"].exits["n"].to.as_deref(), Some("2906"));
        assert!(!map.rooms["2906"].exits.contains_key("s"));
        assert_eq!((map.rooms["2906"].x, map.rooms["2906"].y), (0, -1));
    }

    #[test]
    fn write_and_read_round_trips_map() {
        let path =
            std::env::temp_dir().join(format!("mud-client-map-test-{}.toml", std::process::id()));
        let mut map = MapState::default();
        map.create();
        map.move_direction("n").unwrap();
        map.execute("set roomname Northern Room").unwrap();

        map.write_to_path(path.to_str().unwrap()).unwrap();
        let mut loaded = MapState::default();
        loaded.read_from_path(path.to_str().unwrap()).unwrap();
        let _ = std::fs::remove_file(path);

        assert_eq!(loaded.current_room.as_deref(), Some("2"));
        assert_eq!(loaded.rooms["2"].name, "Northern Room");
        assert_eq!(loaded.rooms["1"].exits["n"].to.as_deref(), Some("2"));
    }
}
