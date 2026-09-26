---@meta
---@diagnostic disable: lowercase-global, undefined-global

---@alias MudColor string Named terminal color, bright color name, `index:N`, or `#RRGGBB`.
---@alias MudOutputCategory "Normal"|"Combat"|"Communication"|"System"|"Error"|"Prompt"|"Triggered"|"Snapshot"
---@alias MudDirection "n"|"e"|"s"|"w"|"u"|"d"|"north"|"east"|"south"|"west"|"up"|"down"|string
---@alias MudDoorState "trigger"|"unknown"|"open"|"closed"|"pickable"|"locked"|"none"|"clear"|"off"
---@alias MudPanelName "group"|"opponent"|"character"|string
---@alias MudToggleState "on"|"off"|string

---@class MudOutputOptions
---@field foreground? MudColor Foreground color for local output.
---@field background? MudColor Background color for local output.

---Context passed to Lua functions called by alias rules.
---@class MudAliasContext
---@field kind "alias"
---@field input string Original command input that matched the alias.
---@field captures string[] Regex captures. Lua arrays are 1-based.

---ANSI colors active on a trigger match.
---@class MudTriggerColors
---@field foregrounds string[] Active foreground colors, lower-case debug names from the client color parser.
---@field backgrounds string[] Active background colors, lower-case debug names from the client color parser.

---Context passed to Lua functions called by trigger rules.
---@class MudTriggerContext
---@field kind "trigger"
---@field line string Normalized visible line.
---@field raw_line string Original MUD line with ANSI/control content preserved by the client.
---@field category MudOutputCategory Output category.
---@field captures string[] Regex captures. Lua arrays are 1-based.
---@field colors? MudTriggerColors ANSI colors seen on the line, when available.

---Context passed to Lua functions called by event handlers.
---@class MudEventContext
---@field kind "event"
---@field event string Event name.
---@field source string Event source, such as `manual`, `trigger`, or `lua`.
---@field captures string[] Regex captures from an event handler match. Lua arrays are 1-based.

---Context passed by `/lua call <function>`.
---@class MudManualContext
---@field kind "manual"
---@field input string Manual input, when supplied by the caller.

---@alias MudHookContext MudAliasContext|MudTriggerContext|MudEventContext|MudManualContext
---@alias MudHookFunction fun(ctx: MudHookContext)

---Character snapshot. Numeric values are strings because they come from MSDP/config state.
---@class MudCharacter
---@field name? string Character name.
---@field race? string Character race from MSDP.
---@field level? string Character level.
---@field health? string Current health.
---@field health_max? string Maximum health.
---@field mana? string Current mana.
---@field mana_max? string Maximum mana.
---@field movement? string Current movement/stamina.
---@field movement_max? string Maximum movement/stamina.
---@field experience? string Current experience or remaining TNL value as reported by MSDP.
---@field experience_max? string Experience maximum, when reported.
---@field ob? string Offensive bonus.
---@field db? string Dodge bonus.
---@field pb? string Parry bonus.
---@field attack_speed? string Attack speed.
---@field strength? string Strength.
---@field intelligence? string Intelligence.
---@field will? string Will.
---@field dexterity? string Dexterity.
---@field constitution? string Constitution.
---@field learning? string Learning.
---@field willpower? string Willpower.
---@field spell_save? string Spell save.
---@field spirit? string Spirit.
---@field spell_power? string Spell power.
---@field spell_pen? string Spell penetration.
---@field warrior_level? integer Warrior class level.
---@field ranger_level? integer Ranger class level.
---@field mystic_level? integer Mystic class level.
---@field mage_level? integer Mage class level.
---@field health_regeneration? integer Health regeneration.
---@field stamina_regeneration? integer Mana regeneration, reported by RoTS as STAMINA_REGENERATION.
---@field movement_regeneration? integer Movement regeneration.

---Opponent snapshot. Numeric values are strings because they come from MSDP state.
---@class MudOpponent
---@field name? string Opponent name.
---@field level? string Opponent level.
---@field health? string Opponent health or health percent.
---@field health_max? string Opponent max health, when reported.

---Group member snapshot. Percent values are strings because they come from MSDP state.
---@class MudGroupMember
---@field name string Group member name.
---@field health_percent? string Health percent.
---@field mana_percent? string Mana percent.
---@field movement_percent? string Movement percent.

---Current map room snapshot.
---@class MudRoom
---@field id string Room id/vnum.
---@field name string Room name.
---@field terrain string Room terrain.
---@field weight string Pathing weight.
---@field exits string[] Exit directions in the current room's map order.
---@field x integer Local x coordinate.
---@field y integer Local y coordinate.
---@field z integer Local z layer.

---@class MudEventRecord
---@field name string Event name.
---@field source string Event source.

---@alias MudMsdpValue string|MudMsdpValue[]|table<string, MudMsdpValue>

---@class MudLogApi
---@field debug fun(message: string) Queue a debug log message.
---@field info fun(message: string) Queue an info log message.
---@field warn fun(message: string) Queue a warning log message.
---@field error fun(message: string) Queue an error log message.

---@class MudVariableApi
---@field get fun(name: string): string? Read the effective configured/runtime variable value.
---@field set fun(name: string, value: string) Queue a runtime variable set.
---@field unset fun(name: string) Queue a runtime variable removal.
---@field list fun(): table<string, string> Return effective variables visible to the hook.

---@class MudMsdpApi
---@field get fun(name: string): MudMsdpValue? Return the latest stored MSDP value by raw variable name.
---@field all fun(): table<string, MudMsdpValue> Return all currently stored MSDP values keyed by raw variable name.

---@class MudCharacterApi
---@field get fun(): MudCharacter Return the latest character snapshot.

---@class MudOpponentApi
---@field get fun(): MudOpponent Return the latest opponent snapshot.

---@class MudGroupApi
---@field list fun(): MudGroupMember[] Return the latest group member snapshots.

---@class MudRoomApi
---@field current fun(): MudRoom? Return the current map room snapshot.

---@class MudOutputApi
---@field recent fun(limit?: integer): string[] Return recent normalized output lines. Default limit is 20.
---@field search fun(pattern: string): string[] Return normalized output lines containing the literal pattern.
---@field replace fun(text: string) Replace this incoming line with single-line text; incoming-line trigger hooks only.
---@field gag fun() Hide this incoming line; incoming-line trigger hooks only.

---@class MudEventApi
---@field emit fun(name: string, source?: string) Queue a script event for dispatch.
---@field recent fun(limit?: integer): MudEventRecord[] Return recent events. Default limit is 10.

---@class MudMapApi
---@field current fun(): MudRoom? Return the current map room snapshot.
---@field get fun(room_id?: string): MudRoom? Return a map room snapshot. Current implementation returns the current room even when `room_id` is supplied.
---@field find fun(query: string) Queue `/map find <query>`.
---@field goto fun(target: string) Queue `/map goto <target>`.
---@field run fun(target: string) Queue `/map run <target>`.
---@field door fun(direction: MudDirection, state: MudDoorState, name?: string) Queue `/map door <direction> <state> [name]`. Named closed, pickable, and locked doors expand movement into open, pick, or unlock/open commands.
---@field set_terrain fun(terrain: string) Queue `/map set terrain <terrain>`.
---@field set_weight fun(weight: number) Queue `/map set weight <weight>`.

---@class MudUiApi
---@field toggle fun(panel: MudPanelName, state?: MudToggleState) Queue `/toggle <panel> [state]`.
---@field reload fun() Queue `/reload`.

---@class MudTimeApi
---@field now_ms fun(): integer Return Unix time in milliseconds.

---@class MudTimerApi
---@field set fun(name: string, interval_ms: integer, callback: string, repeat_timer?: boolean) Schedule a Lua callback.
---@field cancel fun(name: string) Cancel a named timer.

---Global client API available inside configured Lua hooks.
---@class MudClientApi
---@field send fun(command: string) Queue one MUD command directly, without expanding aliases.
---@field send_all fun(commands: string[]) Queue several MUD commands directly, without expanding aliases.
---@field execute fun(command: string) Queue typed command input through aliases and local commands, subject to the current dispatch budget.
---@field echo fun(text: string, opts?: MudOutputOptions) Queue local output in the normal output pane.
---@field notify fun(text: string, opts?: MudOutputOptions) Queue a triggered local notification.
---@field log MudLogApi Logging action helpers.
---@field var MudVariableApi Runtime/configured variable helpers.
---@field msdp MudMsdpApi Raw MSDP read helpers.
---@field character MudCharacterApi Character snapshot helpers.
---@field opponent MudOpponentApi Opponent snapshot helpers.
---@field group MudGroupApi Group snapshot helpers.
---@field room MudRoomApi Current room snapshot helpers.
---@field output MudOutputApi Output scrollback read helpers.
---@field event MudEventApi Script event helpers.
---@field map MudMapApi Mapper helpers.
---@field ui MudUiApi UI local command helpers.
---@field time MudTimeApi Time helpers.
---@field timer MudTimerApi Timer helpers.

---@type MudClientApi
client = client
