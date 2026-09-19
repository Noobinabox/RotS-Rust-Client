-- Example Rust MUD Client Lua hooks.
--
-- Editor support:
-- - `scripts/mud-client-api.lua` contains LuaLS/EmmyLua annotations for `client`
--   and hook context tables.
-- - `.luarc.json` points LuaLS at this folder so completions work in editors
--   that support Lua Language Server.

---@param ctx MudAliasContext
function smart_kill(ctx)
  local target = ctx.captures[1]
  if target == nil or target == "" then
    client.echo("Usage: sk <target>", { foreground = "yellow" })
    return
  end

  client.var.set("last_target", target)
  client.send("target " .. target)
  client.send("kill " .. target)
end

---@param ctx MudAliasContext
function run_path(ctx)
  local destination = ctx.captures[1]
  if destination == nil or destination == "" then
    client.echo("Usage: runto <room name or vnum>", { foreground = "yellow" })
    return
  end

  client.notify("Running path to " .. destination, { foreground = "cyan" })
  client.map.run(destination)
end

---@param ctx MudTriggerContext
function enemy_arrives(ctx)
  local name = ctx.captures[1]
  if name == nil or name == "" then
    return
  end

  client.notify("Targeting " .. name, { foreground = "yellow" })
  client.send("target " .. name)
  client.event.emit("CombatStarted:" .. name, "lua")
end

---@param ctx MudTriggerContext
function remember_room(ctx)
  local room = client.room.current()
  if room == nil then
    return
  end

  client.var.set("last_room", room.name)
end

---@param ctx MudTriggerContext
function needs_warning(ctx)
  local line = ctx.line or ""
  if line:find("hungry", 1, true) then
    client.notify("You are hungry.", { foreground = "yellow" })
  elseif line:find("thirsty", 1, true) then
    client.notify("You are thirsty.", { foreground = "cyan" })
  end
end

---@param ctx MudEventContext
function low_health(ctx)
  client.notify("Low Health Activated", {foreground = "red"})
  local character = client.character.get()
  local hp = tonumber(character.health or "0") or 0
  local max_hp = tonumber(character.health_max or "0") or 0

  if max_hp > 0 and hp * 100 <= max_hp * 30 then
    client.notify("Low health: fleeing", { foreground = "red" })
    client.send("flee")
  else
    client.log.info("LowHealth event received without critical HP")
  end
end

---@param ctx MudEventContext
function combat_started(ctx)
  local target = ctx.captures[1] or client.var.get("last_target") or "unknown"
  client.var.set("last_target", target)
  client.notify("Combat started: " .. target, { foreground = "red" })
end

---@param ctx MudEventContext
function route_requested(ctx)
  local destination = ctx.captures[1]
  if destination == nil or destination == "" then
    client.echo("RouteRequested event needs a destination.", { foreground = "yellow" })
    return
  end

  client.map.find(destination)
end
