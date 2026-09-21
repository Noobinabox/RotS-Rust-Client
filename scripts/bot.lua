-- Opt-in bot controller. Configure [lua] entrypoint = "bot.lua" before using it.
-- Add game-specific RoomChanged and flee trigger handlers to config.toml.

bot_config = {
  tick_ms = 750,
  restart_paths = true,
  cycle_paths = true,
  recovery_destination = "Town",
  paths = {
    {
      name = "example-hunt",
      waypoints = { "Town", "Hunting Grounds" },
      creatures = { "goblin", "orc" },
      start_room = "Town",
      commands = nil, -- Optional TinTin-style semicolon-separated route.
    },
  },
}

bot_state = {
  active = false, path = 1, waypoint = 1, command = 1, recovering = false,
  route_pending = false, last_attack = 0, failed_step = 0, engaged = 0,
  paused = false, resetting = false, hard_mode = false,
}

local function current_room_name()
  local room = client.room.current()
  return room and room.name or nil
end

local function current_path()
  return bot_config.paths[bot_state.path]
end

function bot_start(_)
  if #bot_config.paths == 0 then client.notify("Bot has no paths configured.") return end
  bot_state.active = true
  bot_state.path, bot_state.waypoint, bot_state.command = 1, 1, 1
  bot_state.recovering, bot_state.route_pending, bot_state.failed_step = false, false, 0
  bot_state.paused, bot_state.engaged = false, 0
  client.timer.set("bot-controller", bot_config.tick_ms, "bot_tick", true)
  client.notify("Bot started.")
end

function bot_load_area(ctx)
  local name = (ctx.input or ""):match("%S+%s+(%S+)") or (ctx.input or ""):match("^%S+$")
  local area = name and bot_areas and bot_areas[name]
  if not area then client.notify("Unknown bot area: " .. tostring(name)) return end
  local path = {}
  for key, value in pairs(area) do path[key] = value end
  if path.commands and #path.commands == 0 then path.commands = nil end
  bot_config.paths = { path }
  bot_config.cycle_paths = area.cycle == true
  client.notify("Loaded bot area: " .. path.title)
end

function bot_stop(_)
  bot_state.active = false
  client.timer.cancel("bot-controller")
  client.notify("Bot stopped.")
end

function bot_status(_)
  local path = current_path()
  client.notify(string.format("Bot %s; path %d/%d; waypoint %d/%d; recovering=%s",
    bot_state.active and "running" or "stopped", bot_state.path, #bot_config.paths,
    bot_state.waypoint, path and #path.waypoints or 0, tostring(bot_state.recovering)))
end

function bot_tick(ctx)
  if not bot_state.active then return end
  if bot_state.paused or bot_state.engaged > 0 then return end
  local path = current_path()
  if not path then bot_stop(ctx) return end
  local room_name = current_room_name()

  if bot_state.recovering then
    if room_name == bot_config.recovery_destination then
      bot_state.recovering, bot_state.route_pending = false, false
      bot_state.waypoint = 1
      client.notify("Bot recovered; restarting the current path.")
    elseif not bot_state.route_pending then
      client.map.run(bot_config.recovery_destination)
      bot_state.route_pending = true
    end
    return
  end

  local destination = path.waypoints and path.waypoints[bot_state.waypoint]

  -- Area files may provide the original TinTin++ command route. Commands are
  -- sent one at a time and movement is checked on the next timer tick.
  if path.commands and path.commands[bot_state.command] then
    client.send(path.commands[bot_state.command])
    bot_state.command = bot_state.command + 1
    bot_state.route_pending = true
    return
  end

  if not destination then
    if path.start_room and room_name ~= path.start_room then
      client.map.run(path.start_room)
      bot_state.route_pending = true
      return
    end
    bot_state.command = 1
    bot_state.waypoint = 1
    destination = path.waypoints and path.waypoints[1]
  end
  if room_name == destination then
    bot_state.route_pending = false
    if bot_state.waypoint == #path.waypoints then
      if bot_config.cycle_paths and bot_state.path < #bot_config.paths then
        bot_state.path, bot_state.waypoint = bot_state.path + 1, 1
      elseif bot_config.restart_paths then
        bot_state.path, bot_state.waypoint = 1, 1
      else
        bot_stop(ctx)
      end
      return
    end
    bot_state.waypoint = bot_state.waypoint + 1
    destination = path.waypoints[bot_state.waypoint]
  end

  if not bot_state.route_pending then
    client.map.run(destination)
    bot_state.route_pending = true
  end

  local opponent = client.opponent.get()
  if #path.creatures > 0 and opponent and opponent.name == nil and room_name == path.waypoints[#path.waypoints] then
    local now = client.time.now_ms()
    if now - bot_state.last_attack >= 3000 then
      local creature = path.creatures[((bot_state.waypoint - 1) % #path.creatures) + 1]
      if creature then client.send("kill " .. creature) bot_state.last_attack = now end
    end
  end
end

function bot_pause(_)
  bot_state.paused = true
  client.notify("Bot paused for combat or manual recovery.")
end

function bot_resume(_)
  bot_state.paused = false
  bot_state.engaged = 0
  bot_state.route_pending = false
  client.notify("Bot resumed.")
end

function bot_reset(_)
  local path = current_path()
  if not path or not path.start_room then bot_stop(_) return end
  bot_state.resetting, bot_state.paused, bot_state.engaged = true, true, 0
  bot_state.failed_step, bot_state.route_pending = 0, false
  client.map.run(path.start_room)
end

function bot_mob_line(ctx)
  if not bot_state.active or not ctx.line then return end
  local path = current_path()
  local target
  for _, mob in ipairs(path.mob_triggers or {}) do
    if ctx.line == mob.text then
      target = mob
      break
    end
  end
  if not target then return end
  bot_state.paused = true
  bot_state.engaged = bot_state.engaged + 1
  if target.hard then bot_state.hard_mode = true end
  client.send("kill " .. target.attack)
end

function bot_combat_started(_)
  if bot_state.active then bot_state.paused = true end
  bot_state.engaged = bot_state.engaged + 1
end

function bot_combat_finished(_)
  if bot_state.engaged > 0 then bot_state.engaged = bot_state.engaged - 1 end
  if bot_state.engaged == 0 then bot_resume(_) end
end

function bot_failed_step(_)
  if not bot_state.active then return end
  bot_state.failed_step = bot_state.failed_step + 1
  bot_state.route_pending = false
  if bot_state.failed_step >= 5 then bot_reset(_) end
end

function bot_room_changed(_)
  bot_state.route_pending = false
  bot_state.failed_step = 0
  if bot_state.resetting then
    bot_state.resetting, bot_state.paused = false, false
    bot_state.command, bot_state.waypoint = 1, 1
  end
end

function bot_flee_line(_)
  if bot_state.active then
    bot_state.recovering, bot_state.route_pending = true, false
    client.notify("Bot detected a flee; recovering route.")
  end
end

function bot_add_path(ctx)
  local fields = {}
  for value in (ctx.input or ""):gmatch("%S+") do table.insert(fields, value) end
  if #fields < 3 then client.notify("Usage: /lua call bot_add_path <name> <start> <destination> [creature...]") return end
  local creatures = {}
  for index = 4, #fields do table.insert(creatures, fields[index]) end
  table.insert(bot_config.paths, { name = fields[1], waypoints = { fields[2], fields[3] }, creatures = creatures })
  client.notify("Bot path added: " .. fields[1])
end

function bot_set_restart(ctx)
  bot_config.restart_paths = (ctx.input or ""):match("on") ~= nil
  client.notify("Bot path restart=" .. tostring(bot_config.restart_paths))
end

function bot_set_cycle(ctx)
  bot_config.cycle_paths = (ctx.input or ""):match("on") ~= nil
  client.notify("Bot path cycling=" .. tostring(bot_config.cycle_paths))
end
