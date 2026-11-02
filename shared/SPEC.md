# Behavior spec

Frontends (COSMIC, Windows, Android) should behave identically.

## Config
- Location: `$XDG_CONFIG_HOME/ha-rooms/config.json` (Linux), `%APPDATA%\ha-rooms\config.json` (Windows).
- Fields: `url`, `token` (long-lived access token), `light_label`, `switch_label`, `climate_label`.
- All values are trimmed on load. An empty label means "show all". Complete = URL and token non-empty.
- If missing, create with defaults (`switch_label` = `panel`) and open settings.
- Settings are instant-apply: every edit is persisted immediately.

## Data
- One `POST /api/template` per domain (`light`, `switch`, `climate`) rendering `template.j2`.
  `DOMAIN`, `EXTRA` (`climate_extra.j2`, climate only) and `ALLOWED` (`label_entities("<label>")`
  or `none`) are substituted. Always substitute `ALLOWED` last.
- Server side: entity needs an area, must not be hidden, must carry the label if one is set.
- Client side (`tidy`): drop `unavailable`; drop names starting with a 4-char hex word
  containing at least one digit followed by a space; sort by lowercase name.
- Group by area name, rooms sorted alphabetically, three cells per room: lights, switches, climate.

## Actions
- Switch/light: optimistic flip, `homeassistant/turn_on|turn_off`, then poll `fetch_all`
  every 500 ms (max 10 tries) until the entity reports the wanted state; the last fetch wins.
- Climate target: `climate/set_temperature`, same polling until `temperature` is within 0.05.
- Stepping: up = `floor(t) + 1`, down = `ceil(t) - 1`; clamp to `min_temp..max_temp` (default 5..35).
- Typed value: comma or dot as decimal separator; invalid input discarded; applied on Enter.
- Current temperature color: warm when `hvac_action` is `heating`, cool when `cooling`.
- Display: integers without decimals, otherwise one decimal; always with `°C`.
