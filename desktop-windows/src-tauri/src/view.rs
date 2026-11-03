//! View model sent to the popup webview. All formatting/grouping comes from ha-core
//! so every frontend behaves the same (see shared/SPEC.md).

use ha_core::model::{fmt_temp, group_by_room, Data};
use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
pub struct Toggle {
    pub id: String,
    pub name: String,
    pub on: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct Climate {
    pub id: String,
    pub name: String,
    /// "20.5°C" or "–"
    pub cur: String,
    /// "heating" | "cooling" | other
    pub action: Option<String>,
    /// Target without unit, e.g. "21"
    pub target: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct RoomView {
    pub name: String,
    pub lights: Vec<Toggle>,
    pub switches: Vec<Toggle>,
    pub climate: Vec<Climate>,
}

#[derive(Serialize, Clone, Debug)]
pub struct View {
    pub rooms: Vec<RoomView>,
    pub error: Option<String>,
}

fn toggle(e: &ha_core::model::Entity) -> Toggle {
    Toggle { id: e.id.clone(), name: e.name.clone(), on: e.is_on() }
}

pub fn build(data: &Data, error: Option<&str>) -> View {
    let rooms = group_by_room(data)
        .into_iter()
        .map(|r| RoomView {
            name: r.name.to_string(),
            lights: r.lights.into_iter().map(toggle).collect(),
            switches: r.switches.into_iter().map(toggle).collect(),
            climate: r
                .climate
                .into_iter()
                .map(|e| Climate {
                    id: e.id.clone(),
                    name: e.name.clone(),
                    cur: e.cur.map_or("–".to_string(), |c| format!("{}°C", fmt_temp(c))),
                    action: e.action.clone(),
                    target: e.target.map(fmt_temp).unwrap_or_default(),
                })
                .collect(),
        })
        .collect();
    View { rooms, error: error.map(str::to_string) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ha_core::model::{tidy, Entity};

    fn sample(file: &str) -> Vec<Entity> {
        let path = format!("{}/../../shared/fixtures/{file}", env!("CARGO_MANIFEST_DIR"));
        let mut v: Vec<Entity> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        tidy(&mut v);
        v
    }

    #[test]
    fn builds_rooms_from_fixtures() {
        let data = Data { lights: sample("lights.json"), switches: vec![], climate: sample("climate.json") };
        let v = build(&data, Some("boom"));
        assert_eq!(v.error.as_deref(), Some("boom"));
        let names: Vec<_> = v.rooms.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["Kitchen", "Living room"]);
        let kitchen = &v.rooms[0];
        assert_eq!(kitchen.lights.len(), 1);
        assert!(!kitchen.lights[0].on);
        assert_eq!(kitchen.climate[0].cur, "20.5°C");
        assert_eq!(kitchen.climate[0].target, "21");
        assert_eq!(kitchen.climate[0].action.as_deref(), Some("heating"));
        assert!(v.rooms[1].lights[0].on);
    }
}
