//! Stable schematic positions, independent of server coordinates and pathfinding.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};

use super::{Exit, ExitFlag, MapState, RoomFlag, direction_by_name};

fn visible_exit(map: &MapState, source: &str, exit: &Exit) -> bool {
    if exit.flags.contains(&ExitFlag::Hide) || exit.flags.contains(&ExitFlag::Boundary) {
        return false;
    }
    let reverse_hidden = exit
        .to
        .as_ref()
        .and_then(|target| map.rooms.get(target))
        .is_some_and(|target| {
            direction_by_name(&exit.direction).is_some_and(|direction| {
                target.exits.values().any(|candidate| {
                    super::normalize_direction(&candidate.direction) == direction.reverse
                        && candidate.to.as_deref() == Some(source)
                        && (candidate.flags.contains(&ExitFlag::Hide)
                            || candidate.flags.contains(&ExitFlag::Boundary))
                })
            })
        });
    !reverse_hidden
}

const MAX_COORDINATE: i32 = 1_000_000;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct DisplayLayout {
    pub positions: BTreeMap<String, DisplayPosition>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct DisplayPosition {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl MapState {
    pub fn refresh_display(&mut self) {
        let mut occupied = BTreeSet::new();
        let mut next_free = BTreeMap::new();
        self.display.positions.retain(|id, p| {
            self.rooms.get(id).is_some_and(|room| room.z == p.z)
                && p.x.abs_diff(0) <= MAX_COORDINATE as u32
                && p.y.abs_diff(0) <= MAX_COORDINATE as u32
                && occupied.insert((p.x, p.y, p.z))
        });
        let mut roots: Vec<_> = self.display.positions.keys().cloned().collect();
        if let Some(id) = &self.current_room {
            roots.push(id.clone());
        }
        roots.extend(self.rooms.keys().cloned());
        let mut visited = BTreeSet::new();
        for root in roots {
            if !self.rooms.contains_key(&root) || visited.contains(&root) {
                continue;
            }
            let z = self.rooms[&root].z;
            if !self.display.positions.contains_key(&root) {
                let position = free_position(0, 0, z, &occupied, &mut next_free);
                occupied.insert((position.x, position.y, z));
                self.display.positions.insert(root.clone(), position);
            }
            let mut queue = VecDeque::from([root]);
            while let Some(id) = queue.pop_front() {
                if !visited.insert(id.clone()) {
                    continue;
                }
                let origin = self.display.positions[&id];
                for exit in self.rooms[&id].exits.values() {
                    let Some(target) = exit.to.as_ref().filter(|id| self.rooms.contains_key(*id))
                    else {
                        continue;
                    };
                    let Some(direction) = direction_by_name(&exit.direction) else {
                        continue;
                    };
                    if !self.display.positions.contains_key(target) {
                        let target_z = self.rooms[target].z;
                        let position = free_position(
                            origin.x.saturating_add(direction.dx),
                            origin.y.saturating_add(direction.dy),
                            target_z,
                            &occupied,
                            &mut next_free,
                        );
                        occupied.insert((position.x, position.y, position.z));
                        self.display.positions.insert(target.clone(), position);
                    }
                    queue.push_back(target.clone());
                }
            }
        }
    }
}

fn free_position(
    x: i32,
    y: i32,
    z: i32,
    occupied: &BTreeSet<(i32, i32, i32)>,
    next_free: &mut BTreeMap<(i32, i32, i32), i32>,
) -> DisplayPosition {
    let x = x.clamp(-MAX_COORDINATE, MAX_COORDINATE);
    let mut y = y.clamp(-MAX_COORDINATE, MAX_COORDINATE);
    let mut searched = Vec::new();
    while occupied.contains(&(x, y, z)) {
        searched.push(y);
        y = next_free.get(&(x, y, z)).copied().unwrap_or_else(|| {
            if y == MAX_COORDINATE {
                -MAX_COORDINATE
            } else {
                y + 1
            }
        });
    }
    for old_y in searched {
        next_free.insert((x, old_y, z), y);
    }
    DisplayPosition { x, y, z }
}

/// Include incoming links so a one-way journey retains its surrounding region.
pub fn visible_positions(map: &MapState, current: &str) -> BTreeMap<String, (i32, i32)> {
    // Supports maps assembled directly by callers, including legacy snapshots.
    let prepared;
    let map = if map.display.positions.len() != map.rooms.len()
        || !map.display.positions.contains_key(current)
    {
        prepared = {
            let mut snapshot = map.clone();
            snapshot.refresh_display();
            snapshot
        };
        &prepared
    } else {
        map
    };
    let Some(origin) = map.display.positions.get(current) else {
        return BTreeMap::new();
    };
    let mut incoming: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (id, room) in &map.rooms {
        if room.flags.contains(&RoomFlag::Hide)
            || room.flags.contains(&RoomFlag::Fog) && id != current
        {
            continue;
        }
        for exit in room.exits.values() {
            if visible_exit(map, id, exit)
                && let Some(target) = exit.to.as_deref()
            {
                incoming.entry(target).or_default().push(id);
            }
        }
    }
    let mut result = BTreeMap::new();
    let mut queue = VecDeque::from([current]);
    while let Some(id) = queue.pop_front() {
        let Some(room) = map.rooms.get(id) else {
            continue;
        };
        let Some(p) = map.display.positions.get(id) else {
            continue;
        };
        if room.z != origin.z || room.flags.contains(&RoomFlag::Hide) || result.contains_key(id) {
            continue;
        }
        result.insert(id.to_string(), (p.x - origin.x, p.y - origin.y));
        if room.flags.contains(&RoomFlag::Fog) && id != current {
            continue;
        }
        for exit in room.exits.values() {
            if visible_exit(map, id, exit)
                && let Some(target) = exit.to.as_deref()
            {
                queue.push_back(target);
            }
        }
        if let Some(sources) = incoming.get(id) {
            queue.extend(sources.iter().copied());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::RoomExitUpdate;

    fn report(map: &mut MapState, room: u32, direction: &str, target: u32) {
        map.sync_room(
            Some(room.to_string()),
            None,
            vec![RoomExitUpdate {
                direction: direction.into(),
                to: Some(target.to_string()),
            }],
            true,
        );
    }

    #[test]
    fn alternating_maze_keeps_every_room_and_reported_destination() {
        let mut map = MapState::default();
        for id in 1101..1111 {
            report(&mut map, id, if id % 2 == 1 { "e" } else { "w" }, id + 1);
        }
        let saved = map.display.clone();
        for id in 1101..1111 {
            report(&mut map, id, if id % 2 == 1 { "e" } else { "w" }, id + 1);
            let positions = visible_positions(&map, &id.to_string());
            assert_eq!(positions.len(), 11);
            assert_eq!(positions.values().collect::<BTreeSet<_>>().len(), 11);
            assert_eq!(map.display, saved);
        }
        assert_eq!(map.rooms["1102"].exits["w"].to.as_deref(), Some("1103"));
        assert!(!map.rooms["1103"].exits.contains_key("w"));
        assert_eq!(map.shortest_path_to("1111").unwrap(), vec!["w"]);
    }

    #[test]
    fn incoming_return_links_do_not_bypass_hidden_exit() {
        let mut map = MapState::default();
        map.execute("create").unwrap();
        map.execute("link e 2 both").unwrap();
        map.execute("exitflag e hide on").unwrap();
        assert!(!visible_positions(&map, "1").contains_key("2"));
        map.execute("exitflag e hide off").unwrap();
        assert!(visible_positions(&map, "1").contains_key("2"));
    }

    #[test]
    fn large_disconnected_map_gets_distinct_stable_lanes() {
        let mut map = MapState::default();
        for n in 0..6000 {
            let id = n.to_string();
            map.rooms.insert(
                id.clone(),
                crate::map::Room {
                    id,
                    ..Default::default()
                },
            );
        }
        map.refresh_display();
        assert_eq!(
            map.display
                .positions
                .values()
                .map(|p| (p.x, p.y, p.z))
                .collect::<BTreeSet<_>>()
                .len(),
            6000
        );
        let saved = map.display.clone();
        map.refresh_display();
        assert_eq!(map.display, saved);
    }

    #[test]
    fn save_reload_and_new_loop_preserve_positions() {
        let mut map = MapState::default();
        report(&mut map, 1101, "e", 1102);
        report(&mut map, 1102, "w", 1103);
        let saved = map.display.clone();
        let raw = toml::to_string(&map).unwrap();
        let mut loaded: MapState = toml::from_str(&raw).unwrap();
        report(&mut loaded, 1103, "e", 1101);
        assert_eq!(loaded.display, saved);
        loaded.execute("delete 1102").unwrap();
        assert!(!loaded.display.positions.contains_key("1102"));
        assert_eq!(loaded.display.positions["1101"], saved.positions["1101"]);
    }

    #[test]
    fn legacy_and_invalid_layout_are_repaired_without_losing_rooms() {
        let mut map = MapState::default();
        report(&mut map, 1101, "e", 1102);
        let legacy = {
            let mut value = toml::Value::try_from(&map).unwrap();
            value.as_table_mut().unwrap().remove("display");
            toml::to_string(&value).unwrap()
        };
        let mut loaded: MapState = toml::from_str(&legacy).unwrap();
        loaded.refresh_display();
        assert_eq!(loaded.display.positions.len(), loaded.rooms.len());
        let duplicate = loaded.display.positions["1101"];
        loaded.display.positions.insert("1102".into(), duplicate);
        loaded.refresh_display();
        assert_ne!(
            loaded.display.positions["1101"],
            loaded.display.positions["1102"]
        );
        loaded.display.positions.get_mut("1102").unwrap().x = i32::MAX;
        loaded.refresh_display();
        assert!(loaded.display.positions["1102"].x < MAX_COORDINATE);
    }
}
