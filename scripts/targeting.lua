-- RoTS color-based targeting. Bindings live in config.toml; see docs/targeting.md.
-- State is intentionally ephemeral: reload clears sightings and manual targets.
local targets, counts, manual_target = {}, {}, nil
local max_targets = 100
local overflow_warned = false
local actions = { k = "kill", l = "exam", p = "p", f = "f", c = "c",
  a = "a", b = "b", q = "q", x = "x", h = "h", i = "i", y = "y" }
local excluded = {}
for word in ("purplish a an the plays and grazes outline leans looking of is runs " ..
  "gnaws approaches sitting stands patrols bearing walks light form turns covered " ..
  "from in pulsing"):gmatch("%S+") do
  excluded[word] = true
end

-- Explicit exceptions take precedence over the generic TinTin keyword heuristic.
local special = {
  { "a writhing mass of swamp bugs is buzzing through the air here.", "bugs" },
  { "a long, purplish vine lies on the ground.", "vine" },
  { "a mordwight hovers here, transluscent and grey", "mordwight" },
  { "a strong hunched orc bearing the crest of the black tower stands here.", "orc" },
  { "an olog-hai stands here, growling angrily at its discovery.", "olog" },
  { "a blood covered troll stands here grimacing in delight.", "troll" },
  { "a tall, powerful uruk-hai stands here, ordering about his troops.", "uruk" },
  { "a large, powerful uruk-hai stands here, brandishing his weapon.", "uruk" },
  { "a massive redbacked spider fills the cave here.", "spider" },
  { "some poisonous ivy grows here, its greenish-red fronds snaking towards you.", "ivy" },
  { "the skeletal, wight, from of the lord of durgurth sits here.", "wight" },
  { "karugh the orcish captain is here, keeping his troops in line.", "karugh" },
  { "two reptilian eyes observe the surroundings from out of the mud.", "lizard" },
  { "some twisted vines hang from the trees.", "vine" },
  { "dark shadows shift and twist here into different forms. (shadow)", "shifting" },
}

local function warn(message)
  client.echo("Targeting: " .. message, { foreground = "yellow" })
end

local function valid_keyword(word)
  return word ~= nil and #word <= 64 and word:match("^[a-z][a-z%-']*$")
    and not excluded[word]
end

local function keyword_for(line)
  local text = line:lower():match("^%s*(.-)%s*$")
  -- Never turn server-controlled command delimiters/expansions into actions.
  if text:find("[;%c${}]") then return nil end
  for _, rule in ipairs(special) do
    if text:sub(1, #rule[1]) == rule[1] then return rule[2] end
  end
  local article, rest = text:match("^(%a+)%s+(.+)$")
  if article == "a" or article == "an" or article == "the" then
    local words = {}
    for word in rest:gmatch("[a-z][a-z%-']*") do
      words[#words + 1] = word
      if #words == 3 then break end
    end
    -- Skip a hyphenated second descriptor; commas are ignored as in the source rules.
    local preferred = words[2]
    if preferred and preferred:find("-", 1, true) then preferred = words[3] end
    if valid_keyword(preferred) then return preferred end
    if valid_keyword(words[1]) then return words[1] end
  else
    local first = text:match("^([a-z][a-z%-']*)[%s,.!]")
    if valid_keyword(first) then return first end
  end
  return nil
end

function targeting_reset(ctx)
  targets, counts, overflow_warned = {}, {}, false
end

function targeting_observe(ctx)
  if #targets >= max_targets then
    if not overflow_warned then warn("100-target limit reached; refresh with look.") end
    overflow_warned = true
    return
  end
  local keyword = keyword_for(ctx.line or "")
  local entry = { keyword = keyword }
  if keyword then
    counts[keyword] = (counts[keyword] or 0) + 1
    entry.command_target = counts[keyword] .. "." .. keyword
  end
  targets[#targets + 1] = entry
  -- Rewrite this output line, retaining the original ANSI content and ordering.
  client.output.replace("(" .. #targets .. ") " .. (ctx.raw_line or ctx.line or ""))
end

function targeting_list(ctx)
  local lines = { "Targets (manual: " .. (manual_target or "none") .. ")" }
  for index, entry in ipairs(targets) do
    lines[#lines + 1] = index .. ": " .. (entry.command_target or "unrecognized; use target <keyword>")
  end
  if #targets == 0 then lines[#lines + 1] = "No sightings; use look to refresh." end
  client.echo(table.concat(lines, "\n"))
end

function targeting_manual(ctx)
  local input = ctx.input or ""
  local value = input:match("^target%s+(.+)$")
  if not value then
    if input == "target" then manual_target = nil; warn("manual target cleared.") end
    return
  end
  value = value:match("^%s*(.-)%s*$")
  -- A single keyword, optionally numbered. No whitespace, expansions or commands.
  local number, keyword = value:match("^(%d+)%.([A-Za-z][A-Za-z%-']*)$")
  keyword = keyword or value:match("^([A-Za-z][A-Za-z%-']*)$")
  if not keyword or not valid_keyword(keyword:lower())
    or (number and (#number > 3 or tonumber(number) < 1 or tonumber(number) > max_targets)) then
    warn("use target <keyword> or target <1..100>.<keyword>; target alone clears it.")
    return
  end
  manual_target = value
  warn("manual target set to " .. value .. ".")
end

function targeting_action(ctx)
  local action, selector = (ctx.input or ""):match("^([pflcabkqxhiy])([0-9]+)$")
  if not action then action, selector = (ctx.input or ""):match("^([pflcabkqxhiy])(t)$") end
  if not actions[action] then return end
  local target
  if selector == "t" and manual_target then
    target = manual_target
  else
    local index = selector == "t" and 1 or tonumber(selector)
    if not index or index < 1 or index > max_targets or not targets[index] then
      warn("no such target; use vt or look.")
      return
    end
    target = targets[index].command_target
  end
  if not target then warn("unrecognized description; set target <keyword> explicitly."); return end
  client.execute(actions[action] .. " " .. target)
end
