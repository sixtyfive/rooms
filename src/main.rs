use cosmic::applet::{menu_button, padded_control};
use cosmic::iced::{window::Id, Length, Limits, Rectangle, Vector};
use cosmic::prelude::*;
use cosmic::surface::action::{app_popup, destroy_popup};
use cosmic::widget;
use ha_rooms::config::{err, init_config, load_config, Config};
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Deserialize)]
struct Entity {
    id: String,
    name: String,
    state: String,
    area: String,
    cur: Option<f64>,
    target: Option<f64>,
    action: Option<String>,
    min: Option<f64>,
    max: Option<f64>,
}

#[derive(Debug, Clone, Default)]
struct Data {
    lights: Vec<Entity>,
    switches: Vec<Entity>,
    climate: Vec<Entity>,
}

const CLIMATE_EXTRA: &str = ", 'cur': s.attributes.get('current_temperature'), 'target': s.attributes.get('temperature'), 'action': s.attributes.get('hvac_action'), 'min': s.attributes.get('min_temp'), 'max': s.attributes.get('max_temp')";

const WARM: cosmic::iced::Color = cosmic::iced::Color::from_rgb(0.93, 0.45, 0.15);
const COOL: cosmic::iced::Color = cosmic::iced::Color::from_rgb(0.30, 0.62, 0.95);

fn fmt_temp(t: f64) -> String {
    if t.fract() == 0.0 {
        format!("{t:.0}")
    } else {
        format!("{t:.1}")
    }
}

const TEMPLATE: &str = r#"{%- set ns = namespace(l=[]) -%}
{%- set allowed = ALLOWED -%}
{%- for s in states.DOMAIN | list -%}
{%- if area_name(s.entity_id) and not is_hidden_entity(s.entity_id) and (allowed is none or s.entity_id in allowed) -%}
{%- set ns.l = ns.l + [{'id': s.entity_id, 'name': s.name, 'state': s.state, 'area': area_name(s.entity_id)EXTRA}] -%}
{%- endif -%}
{%- endfor -%}
{{ ns.l | tojson }}"#;

fn has_id_prefix(name: &str) -> bool {
    match name.split_once(' ') {
        Some((p, _)) => {
            p.len() == 4
                && p.chars().all(|c| c.is_ascii_hexdigit())
                && p.chars().any(|c| c.is_ascii_digit())
        }
        None => false,
    }
}

async fn fetch(cfg: &Config, domain: &str) -> Result<Vec<Entity>, String> {
    let label = match domain {
        "switch" => &cfg.switch_label,
        "climate" => &cfg.climate_label,
        _ => &cfg.light_label,
    }
    .trim();
    let allowed = if !label.is_empty() {
        format!("label_entities({})", serde_json::to_string(label).map_err(err)?)
    } else {
        "none".to_string()
    };
    let extra = if domain == "climate" { CLIMATE_EXTRA } else { "" };
    let template = TEMPLATE
        .replace("DOMAIN", domain)
        .replace("EXTRA", extra)
        .replace("ALLOWED", &allowed);
    let text = reqwest::Client::new()
        .post(format!("{}/api/template", cfg.url.trim_end_matches('/')))
        .bearer_auth(&cfg.token)
        .json(&serde_json::json!({ "template": template }))
        .send()
        .await
        .map_err(err)?
        .error_for_status()
        .map_err(err)?
        .text()
        .await
        .map_err(err)?;
    let mut v: Vec<Entity> = serde_json::from_str(&text).map_err(err)?;
    v.retain(|e| e.state != "unavailable" && !has_id_prefix(&e.name));
    v.sort_by_key(|e| e.name.to_lowercase());
    Ok(v)
}

async fn fetch_all() -> Result<Data, String> {
    let cfg = load_config()?;
    let (lights, switches, climate) = tokio::try_join!(
        fetch(&cfg, "light"),
        fetch(&cfg, "switch"),
        fetch(&cfg, "climate")
    )?;
    Ok(Data { lights, switches, climate })
}

async fn call(path: &str, body: serde_json::Value) -> Result<(), String> {
    let cfg = load_config()?;
    reqwest::Client::new()
        .post(format!("{}/api/services/{path}", cfg.url.trim_end_matches('/')))
        .bearer_auth(&cfg.token)
        .json(&body)
        .send()
        .await
        .map_err(err)?
        .error_for_status()
        .map_err(err)?;
    Ok(())
}

async fn settle(entity_id: String, done: impl Fn(&Entity) -> bool) -> Result<Data, String> {
    let mut last = fetch_all().await?;
    for _ in 0..10 {
        let all = last.lights.iter().chain(&last.switches).chain(&last.climate);
        if all.into_iter().any(|e| e.id == entity_id && done(e)) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        last = fetch_all().await?;
    }
    Ok(last)
}

async fn set_state(entity_id: String, on: bool) -> Result<Data, String> {
    let service = if on { "turn_on" } else { "turn_off" };
    call(&format!("homeassistant/{service}"), serde_json::json!({ "entity_id": entity_id })).await?;
    let want = if on { "on" } else { "off" };
    settle(entity_id, move |e| e.state == want).await
}

async fn set_temperature(entity_id: String, temperature: f64) -> Result<Data, String> {
    call(
        "climate/set_temperature",
        serde_json::json!({ "entity_id": entity_id, "temperature": temperature }),
    )
    .await?;
    settle(entity_id, move |e| e.target.is_some_and(|t| (t - temperature).abs() < 0.05)).await
}

struct Room<'a> {
    name: &'a str,
    lights: Vec<&'a Entity>,
    switches: Vec<&'a Entity>,
    climate: Vec<&'a Entity>,
}

impl<'a> Room<'a> {
    fn new(name: &'a str) -> Self {
        Self { name, lights: Vec::new(), switches: Vec::new(), climate: Vec::new() }
    }
}

fn group_by_room(data: &Data) -> Vec<Room<'_>> {
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

fn cell<'a>(items: Vec<Element<'a, Message>>) -> Element<'a, Message> {
    let mut col = widget::column(Vec::<Element<'a, Message>>::new()).width(Length::Fill);
    if !items.is_empty() {
        let mut list = widget::list_column();
        for item in items {
            list = list.add(item);
        }
        col = col.push(list);
    }
    col.into()
}

fn columns<'a>(cells: [Element<'a, Message>; 3]) -> Element<'a, Message> {
    let mut row = widget::row(Vec::<Element<'a, Message>>::new()).spacing(16);
    for c in cells {
        row = row.push(c);
    }
    row.into()
}

fn fetch_task() -> Task<cosmic::Action<Message>> {
    Task::perform(fetch_all(), |r| cosmic::Action::App(Message::Loaded(r)))
}

#[derive(Default)]
struct AppModel {
    core: cosmic::Core,
    popup: Option<Id>,
    configured: bool,
    data: Data,
    error: Option<String>,
    screen_height: Option<f32>,
    edits: HashMap<String, String>,
}

#[derive(Debug, Clone)]
enum Message {
    TogglePopup(Vector, Rectangle),
    PopupClosed(Id),
    Loaded(Result<Data, String>),
    Toggle(String),
    OpenSettings,
    TempEdit(String, String),
    TempSubmit(String),
    TempStep(String, i32),
    Monitor(Option<cosmic::iced::Size>),
}

fn open_settings() {
    let _ = std::process::Command::new("ha-rooms-settings").spawn();
}

fn popup_limits() -> Limits {
    Limits::NONE
        .min_height(1.0)
        .min_width(960.0)
        .max_width(960.0)
        .max_height(4000.0)
}

impl AppModel {
    fn climate_row<'a>(&'a self, e: &'a Entity) -> Element<'a, Message> {
        let mut current = widget::text(e.cur.map_or("–".to_string(), |c| format!("{}°C", fmt_temp(c))));
        match e.action.as_deref() {
            Some("heating") => current = current.class(cosmic::theme::Text::Color(WARM)),
            Some("cooling") => current = current.class(cosmic::theme::Text::Color(COOL)),
            _ => {}
        }
        let shown = self
            .edits
            .get(&e.id)
            .cloned()
            .unwrap_or_else(|| e.target.map(fmt_temp).unwrap_or_default());
        let (id_edit, id_submit, id_up, id_down) = (e.id.clone(), e.id.clone(), e.id.clone(), e.id.clone());
        let input = widget::text_input("", shown)
            .on_input(move |s| Message::TempEdit(id_edit.clone(), s))
            .on_submit(move |_| Message::TempSubmit(id_submit.clone()))
            .trailing_icon(widget::text("°C").into())
            .width(Length::Fixed(68.0));
        let steppers = widget::column(Vec::<Element<'a, Message>>::new())
            .push(
                widget::button::icon(widget::icon::from_name("pan-up-symbolic").handle())
                    .on_press(Message::TempStep(id_up, 1)),
            )
            .push(
                widget::button::icon(widget::icon::from_name("pan-down-symbolic").handle())
                    .on_press(Message::TempStep(id_down, -1)),
            );
        let controls = widget::row(Vec::<Element<'a, Message>>::new())
            .spacing(8)
            .align_y(cosmic::iced::alignment::Vertical::Center)
            .push(current)
            .push(input)
            .push(steppers);
        widget::settings::item(e.name.clone(), controls).into()
    }

    fn toggle_item<'a>(&'a self, e: &'a Entity) -> Element<'a, Message> {
        let id = e.id.clone();
        let t = widget::toggler(e.state == "on").on_toggle(move |_| Message::Toggle(id.clone()));
        widget::settings::item(e.name.clone(), t).into()
    }

    fn room_cells<'a>(&'a self, room: &Room<'a>) -> [Element<'a, Message>; 3] {
        [
            cell(room.lights.iter().copied().map(|e| self.toggle_item(e)).collect()),
            cell(room.switches.iter().copied().map(|e| self.toggle_item(e)).collect()),
            cell(room.climate.iter().copied().map(|e| self.climate_row(e)).collect()),
        ]
    }

    fn entity_mut(&mut self, id: &str) -> Option<&mut Entity> {
        let d = &mut self.data;
        d.lights
            .iter_mut()
            .chain(d.switches.iter_mut())
            .chain(d.climate.iter_mut())
            .find(|e| e.id == id)
    }

    fn apply_temperature(&mut self, id: String, t: f64) -> Task<cosmic::Action<Message>> {
        self.edits.remove(&id);
        let Some(e) = self.entity_mut(&id) else {
            return Task::none();
        };
        let t = t.clamp(e.min.unwrap_or(5.0), e.max.unwrap_or(35.0));
        e.target = Some(t);
        Task::perform(set_temperature(id, t), |r| cosmic::Action::App(Message::Loaded(r)))
    }

    fn list_max_height(&self) -> f32 {
        let screen = self.screen_height.unwrap_or(1080.0);
        let bar = if self.core.applet.is_horizontal() {
            self.core.applet.suggested_window_size().1.get() as f32
        } else {
            0.0
        };
        (screen - bar - 120.0).max(200.0)
    }

    fn list_view(&self) -> Element<'_, Message> {
        let mut room_list = widget::column(Vec::<Element<'_, Message>>::new())
            .spacing(12)
            .padding(8);
        for room in group_by_room(&self.data) {
            let block = widget::column(Vec::<Element<'_, Message>>::new())
                .spacing(4)
                .push(widget::text::heading(room.name))
                .push(widget::divider::horizontal::default())
                .push(
                    widget::container(columns(self.room_cells(&room)))
                        .padding(cosmic::iced::Padding { top: 8.0, right: 0.0, bottom: 0.0, left: 0.0 }),
                );
            room_list = room_list.push(block);
        }

        let mut body = widget::column(Vec::<Element<'_, Message>>::new());
        if let Some(e) = &self.error {
            body = body.push(padded_control(widget::text(e.as_str())));
        }
        let body = body
            .push(widget::container(widget::scrollable(room_list)).max_height(self.list_max_height()))
            .push(padded_control(widget::divider::horizontal::default()))
            .push(
                menu_button(widget::text::body("HomeAssistant Applet Settings..."))
                    .on_press(Message::OpenSettings),
            );
        widget::container(body)
            .padding(cosmic::iced::Padding { top: 0.0, right: 2.0, bottom: 4.0, left: 2.0 })
            .into()
    }

    fn popup_view(&self) -> Element<'_, Message> {
        self.core.applet.popup_container(self.list_view()).limits(popup_limits()).into()
    }
}

impl cosmic::Application for AppModel {
    type Executor = cosmic::executor::Default;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = "dev.local.HaRooms";

    fn core(&self) -> &cosmic::Core {
        &self.core
    }
    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    fn init(core: cosmic::Core, _flags: ()) -> (Self, Task<cosmic::Action<Message>>) {
        let (_, needs_setup, error) = init_config();
        if needs_setup {
            open_settings();
        }
        let app = Self {
            core,
            configured: !needs_setup,
            error,
            ..Default::default()
        };
        (app, Task::none())
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn view(&self) -> Element<'_, Message> {
        self.core
            .applet
            .icon_button("ha-switches")
            .on_press_with_rectangle(Message::TogglePopup)
            .into()
    }

    fn view_window(&self, _id: Id) -> Element<'_, Message> {
        widget::text("").into()
    }

    fn update(&mut self, message: Message) -> Task<cosmic::Action<Message>> {
        match message {
            Message::TogglePopup(offset, bounds) => {
                if let Some(id) = self.popup.take() {
                    return cosmic::task::message(cosmic::Action::Surface(destroy_popup(id)));
                }
                self.edits.clear();
                self.configured = load_config().map(|c| c.is_complete()).unwrap_or(false);
                if !self.configured {
                    open_settings();
                    return Task::none();
                }
                let action = app_popup::<AppModel>(
                    |_| Default::default(),
                    move |state: &mut AppModel| {
                        let id = Id::unique();
                        state.popup = Some(id);
                        let mut s = state.core.applet.get_popup_settings(
                            state.core.main_window_id().unwrap(),
                            id,
                            None,
                            None,
                            None,
                        );
                        s.positioner.anchor_rect = Rectangle {
                            x: (bounds.x - offset.x) as i32,
                            y: (bounds.y - offset.y) as i32,
                            width: bounds.width as i32,
                            height: bounds.height as i32,
                        };
                        s.positioner.size_limits = popup_limits();
                        s
                    },
                    Some(Box::new(|state: &AppModel| {
                        Element::from(state.popup_view()).map(cosmic::Action::App)
                    })),
                );
                let open = cosmic::task::message(cosmic::Action::Surface(action));
                let monitor = cosmic::iced::window::monitor_size(self.core.main_window_id().unwrap())
                    .map(|s| cosmic::Action::App(Message::Monitor(s)));
                return Task::batch([open, monitor, fetch_task()]);
            }
            Message::PopupClosed(id) => {
                if self.popup == Some(id) {
                    self.popup = None;
                }
            }
            Message::Loaded(r) => match r {
                Ok(d) => {
                    self.data = d;
                    self.error = None;
                }
                Err(e) => self.error = Some(e),
            },
            Message::Toggle(id) => {
                let mut on = false;
                if let Some(e) = self.entity_mut(&id) {
                    on = e.state != "on";
                    e.state = if on { "on".into() } else { "off".into() };
                }
                return Task::perform(set_state(id, on), |r| cosmic::Action::App(Message::Loaded(r)));
            }
            Message::TempEdit(id, text) => {
                self.edits.insert(id, text);
            }
            Message::TempSubmit(id) => {
                let parsed = self.edits.remove(&id).and_then(|t| t.trim().replace(',', ".").parse::<f64>().ok());
                if let Some(t) = parsed {
                    return self.apply_temperature(id, t);
                }
            }
            Message::TempStep(id, dir) => {
                let current = self.entity_mut(&id).and_then(|e| e.target);
                if let Some(t) = current {
                    let next = if dir > 0 { t.floor() + 1.0 } else { t.ceil() - 1.0 };
                    return self.apply_temperature(id, next);
                }
            }
            Message::OpenSettings => {
                open_settings();
                if let Some(id) = self.popup.take() {
                    return cosmic::task::message(cosmic::Action::Surface(destroy_popup(id)));
                }
            }
            Message::Monitor(s) => self.screen_height = s.map(|s| s.height),
        }
        Task::none()
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }
}

fn main() -> cosmic::iced::Result {
    cosmic::applet::run::<AppModel>(())
}
