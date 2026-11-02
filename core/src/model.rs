use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Deserialize)]
pub struct Entity {
    pub id: String,
    pub name: String,
    pub state: String,
    pub area: String,
    pub cur: Option<f64>,
    pub target: Option<f64>,
    pub action: Option<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

impl Entity {
    pub fn is_on(&self) -> bool {
        self.state == "on"
    }

    pub fn clamp_temperature(&self, t: f64) -> f64 {
        t.clamp(self.min.unwrap_or(5.0), self.max.unwrap_or(35.0))
    }
}

#[derive(Debug, Clone, Default)]
pub struct Data {
    pub lights: Vec<Entity>,
    pub switches: Vec<Entity>,
    pub climate: Vec<Entity>,
}

impl Data {
    pub fn iter(&self) -> impl Iterator<Item = &Entity> {
        self.lights.iter().chain(&self.switches).chain(&self.climate)
    }

    pub fn find_mut(&mut self, id: &str) -> Option<&mut Entity> {
        self.lights
            .iter_mut()
            .chain(self.switches.iter_mut())
            .chain(self.climate.iter_mut())
            .find(|e| e.id == id)
    }
}

pub struct Room<'a> {
    pub name: &'a str,
    pub lights: Vec<&'a Entity>,
    pub switches: Vec<&'a Entity>,
    pub climate: Vec<&'a Entity>,
}

impl<'a> Room<'a> {
    fn new(name: &'a str) -> Self {
        Self { name, lights: Vec::new(), switches: Vec::new(), climate: Vec::new() }
    }
}

pub fn group_by_room(data: &Data) -> Vec<Room<'_>> {
    let mut rooms: BTreeMap<&str, Room<'_>> = BTreeMap::new();
    for e in &data.lights {
        rooms.entry(&e.area).or_insert_with(|| Room::new(&e.area)).lights.push(e);
    }
    for e in &data.switches {
        rooms.entry(&e.area).or_insert_with(|| Room::new(&e.area)).switches.push(e);
    }
    for e in &data.climate {
        rooms.entry(&e.area).or_insert_with(|| Room::new(&e.area)).climate.push(e);
    }
    rooms.into_values().collect()
}

pub fn has_id_prefix(name: &str) -> bool {
    match name.split_once(' ') {
        Some((p, _)) => {
            p.len() == 4
                && p.chars().all(|c| c.is_ascii_hexdigit())
                && p.chars().any(|c| c.is_ascii_digit())
        }
        None => false,
    }
}

pub fn tidy(entities: &mut Vec<Entity>) {
    entities.retain(|e| e.state != "unavailable" && !has_id_prefix(&e.name));
    entities.sort_by_key(|e| e.name.to_lowercase());
}

pub fn fmt_temp(t: f64) -> String {
    if t.fract() == 0.0 {
        format!("{t:.0}")
    } else {
        format!("{t:.1}")
    }
}

pub fn parse_temperature(text: &str) -> Option<f64> {
    text.trim().replace(',', ".").parse::<f64>().ok().filter(|t| t.is_finite())
}

pub fn step_temperature(current: f64, direction: i32) -> f64 {
    if direction > 0 {
        current.floor() + 1.0
    } else {
        current.ceil() - 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(file: &str) -> Vec<Entity> {
        let path = format!("{}/../shared/fixtures/{file}", env!("CARGO_MANIFEST_DIR"));
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn id_prefix() {
        assert!(has_id_prefix("ad3b radiator valve Chld Lock"));
        assert!(!has_id_prefix("Dead lamp"));
        assert!(!has_id_prefix("Kitchen"));
        assert!(!has_id_prefix("ad3b"));
    }

    #[test]
    fn stepping_snaps_to_integers() {
        assert_eq!(step_temperature(21.0, 1), 22.0);
        assert_eq!(step_temperature(21.0, -1), 20.0);
        assert_eq!(step_temperature(21.5, 1), 22.0);
        assert_eq!(step_temperature(21.5, -1), 21.0);
        assert_eq!(step_temperature(21.3, 1), 22.0);
        assert_eq!(step_temperature(21.3, -1), 21.0);
    }

    #[test]
    fn comma_and_dot() {
        assert_eq!(parse_temperature("21,5"), Some(21.5));
        assert_eq!(parse_temperature(" 21.5 "), Some(21.5));
        assert_eq!(parse_temperature("abc"), None);
        assert_eq!(parse_temperature("NaN"), None);
    }

    #[test]
    fn formatting() {
        assert_eq!(fmt_temp(21.0), "21");
        assert_eq!(fmt_temp(21.5), "21.5");
    }

    #[test]
    fn clamping() {
        let mut v = sample("climate.json");
        tidy(&mut v);
        let e = &v[0];
        assert_eq!(e.clamp_temperature(99.0), e.max.unwrap_or(35.0));
        assert_eq!(e.clamp_temperature(-5.0), e.min.unwrap_or(5.0));
    }

    #[test]
    fn tidy_filters_and_sorts() {
        let mut v = sample("lights.json");
        tidy(&mut v);
        let names: Vec<_> = v.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["Ceiling", "Desk lamp"]);
    }

    #[test]
    fn grouping() {
        let mut data = Data {
            lights: sample("lights.json"),
            switches: Vec::new(),
            climate: sample("climate.json"),
        };
        tidy(&mut data.lights);
        tidy(&mut data.climate);
        let rooms = group_by_room(&data);
        let names: Vec<_> = rooms.iter().map(|r| r.name).collect();
        assert_eq!(names, ["Kitchen", "Living room"]);
        assert_eq!(rooms[0].lights.len(), 1);
        assert_eq!(rooms[0].climate.len(), 1);
        assert_eq!(rooms[1].lights.len(), 1);
        assert!(rooms[1].climate.is_empty());
    }
}
