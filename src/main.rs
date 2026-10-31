use cosmic::applet::{menu_button, padded_control};
use cosmic::iced::{window::Id, Length, Limits, Rectangle, Vector};
use cosmic::prelude::*;
use cosmic::surface::action::{app_popup, destroy_popup};
use cosmic::widget;
use ha_rooms::config::{err, init_config, load_config, Config};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Deserialize)]
struct Entity {
    id: String,
    name: String,
    state: String,
    area: String,
}

#[derive(Debug, Clone, Default)]
struct Data {
    lights: Vec<Entity>,
    switches: Vec<Entity>,
}

const TEMPLATE: &str = r#"{%- set ns = namespace(l=[]) -%}
{%- set allowed = ALLOWED -%}
{%- for s in states.DOMAIN | list -%}
{%- if area_name(s.entity_id) and not is_hidden_entity(s.entity_id) and (allowed is none or s.entity_id in allowed) -%}
{%- set ns.l = ns.l + [{'id': s.entity_id, 'name': s.name, 'state': s.state, 'area': area_name(s.entity_id)}] -%}
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
    let label = (if domain == "switch" { &cfg.switch_label } else { &cfg.light_label }).trim();
    let allowed = if !label.is_empty() {
        format!("label_entities({})", serde_json::to_string(label).map_err(err)?)
    } else {
        "none".to_string()
    };
    let template = TEMPLATE.replace("DOMAIN", domain).replace("ALLOWED", &allowed);
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
    let (lights, switches) = tokio::try_join!(fetch(&cfg, "light"), fetch(&cfg, "switch"))?;
    Ok(Data { lights, switches })
}

async fn set_state(entity_id: String, on: bool) -> Result<Data, String> {
    let cfg = load_config()?;
    let service = if on { "turn_on" } else { "turn_off" };
    reqwest::Client::new()
        .post(format!("{}/api/services/homeassistant/{service}", cfg.url.trim_end_matches('/')))
        .bearer_auth(&cfg.token)
        .json(&serde_json::json!({ "entity_id": entity_id }))
        .send()
        .await
        .map_err(err)?
        .error_for_status()
        .map_err(err)?;
    let want = if on { "on" } else { "off" };
    let mut last = fetch_all().await?;
    for _ in 0..10 {
        if last.lights.iter().chain(&last.switches).any(|e| e.id == entity_id && e.state == want) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        last = fetch_all().await?;
    }
    Ok(last)
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
}

#[derive(Debug, Clone)]
enum Message {
    TogglePopup(Vector, Rectangle),
    PopupClosed(Id),
    Loaded(Result<Data, String>),
    Toggle(String),
    OpenSettings,
    Monitor(Option<cosmic::iced::Size>),
}

fn open_settings() {
    let _ = std::process::Command::new("ha-rooms-settings").spawn();
}

fn popup_limits() -> Limits {
    Limits::NONE
        .min_height(1.0)
        .min_width(640.0)
        .max_width(640.0)
        .max_height(4000.0)
}

impl AppModel {
    fn entity_column<'a>(&'a self, title: &'a str, ents: &'a [Entity]) -> Element<'a, Message> {
        let mut col = widget::column(Vec::<Element<'a, Message>>::new())
            .spacing(8)
            .width(Length::Fill)
            .push(widget::text::title4(title));

        let mut rooms: BTreeMap<&str, Vec<&Entity>> = BTreeMap::new();
        for e in ents {
            rooms.entry(e.area.as_str()).or_default().push(e);
        }
        for (room, list_ents) in rooms {
            let mut list = widget::list_column();
            for e in list_ents {
                let id = e.id.clone();
                let t = widget::toggler(e.state == "on").on_toggle(move |_| Message::Toggle(id.clone()));
                list = list.add(widget::settings::item(e.name.clone(), t));
            }
            col = col.push(widget::text::heading(room)).push(list);
        }
        col.into()
    }

    fn list_max_height(&self) -> f32 {
        let screen = self.screen_height.unwrap_or(1080.0);
        let bar = if self.core.applet.is_horizontal() {
            self.core.applet.suggested_window_size().1.get() as f32
        } else {
            0.0
        };
        (screen - bar - 110.0).max(200.0)
    }

    fn list_view(&self) -> Element<'_, Message> {
        let cols = widget::row(Vec::<Element<'_, Message>>::new())
            .spacing(16)
            .padding(8)
            .push(self.entity_column("Lights", &self.data.lights))
            .push(self.entity_column("Switches", &self.data.switches));

        let mut body = widget::column(Vec::<Element<'_, Message>>::new());
        if let Some(e) = &self.error {
            body = body.push(padded_control(widget::text(e.as_str())));
        }
        body.push(widget::container(widget::scrollable(cols)).max_height(self.list_max_height()))
            .push(padded_control(widget::divider::horizontal::default()))
            .push(
                menu_button(widget::text::body("HomeAssistant Applet Settings..."))
                    .on_press(Message::OpenSettings),
            )
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
                let all = self.data.lights.iter_mut().chain(self.data.switches.iter_mut());
                if let Some(e) = all.into_iter().find(|e| e.id == id) {
                    on = e.state != "on";
                    e.state = if on { "on".into() } else { "off".into() };
                }
                return Task::perform(set_state(id, on), |r| cosmic::Action::App(Message::Loaded(r)));
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
