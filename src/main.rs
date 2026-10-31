use cosmic::applet::{menu_button, padded_control};
use cosmic::iced::{window::Id, Length, Limits, Rectangle, Vector};
use cosmic::prelude::*;
use cosmic::surface::action::{app_popup, destroy_popup};
use cosmic::widget;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Default)]
struct Config {
    url: String,
    token: String,
    #[serde(default)]
    switch_label: String,
}

fn config_path() -> Result<PathBuf, String> {
    let base = std::env::var("XDG_CONFIG_HOME")
        .or_else(|_| std::env::var("HOME").map(|h| format!("{h}/.config")))
        .map_err(err)?;
    Ok(PathBuf::from(base).join("ha-rooms/config.json"))
}

fn load_config() -> Result<Config, String> {
    let path = config_path()?;
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&raw).map_err(|e| format!("{}: {e}", path.display()))
}

fn save_config(cfg: &Config) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let path = config_path()?;
    std::fs::create_dir_all(path.parent().unwrap()).map_err(err)?;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)
        .map_err(err)?;
    f.write_all(serde_json::to_string_pretty(cfg).map_err(err)?.as_bytes())
        .map_err(err)
}

fn init_config() -> (Config, bool, Option<String>) {
    let default = Config {
        url: "http://homeassistant.local:8123".into(),
        token: String::new(),
        switch_label: "panel".into(),
    };
    match load_config() {
        Ok(c) => {
            let ok = !c.url.is_empty() && !c.token.is_empty();
            (c, !ok, None)
        }
        Err(_) if !config_path().map(|p| p.exists()).unwrap_or(false) => {
            let e = save_config(&default).err();
            (default, true, e)
        }
        Err(e) => (default, true, Some(e)),
    }
}

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

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

async fn fetch(cfg: &Config, domain: &str) -> Result<Vec<Entity>, String> {
    let label = cfg.switch_label.trim();
    let allowed = if domain == "switch" && !label.is_empty() {
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

#[derive(Default, PartialEq)]
enum Mode {
    #[default]
    List,
    Settings,
}

#[derive(Default)]
struct AppModel {
    core: cosmic::Core,
    popup: Option<Id>,
    mode: Mode,
    configured: bool,
    data: Data,
    error: Option<String>,
    form: Config,
    show_token: bool,
    screen_height: Option<f32>,
}

#[derive(Debug, Clone)]
enum Message {
    TogglePopup(Vector, Rectangle),
    PopupClosed(Id),
    Loaded(Result<Data, String>),
    Toggle(String),
    OpenSettings,
    CancelSettings,
    SaveSettings,
    UrlInput(String),
    TokenInput(String),
    LabelInput(String),
    ToggleTokenVisible,
    Monitor(Option<cosmic::iced::Size>),
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

    fn settings_view(&self) -> Element<'_, Message> {
        let mut col = widget::column(Vec::<Element<'_, Message>>::new())
            .spacing(12)
            .padding(16)
            .width(Length::Fill)
            .push(widget::text::title4("HomeAssistant Applet Settings"));
        if !self.configured {
            col = col.push(widget::text(
                "Enter your Home Assistant URL and a long-lived access token (HA profile page).",
            ));
        }
        if let Some(e) = &self.error {
            col = col.push(widget::text(e.as_str()));
        }
        col.push(
            widget::text_input("http://homeassistant.local:8123", &self.form.url)
                .label("URL")
                .on_input(Message::UrlInput),
        )
        .push(
            widget::secure_input(
                "Long-lived access token",
                &self.form.token,
                Some(Message::ToggleTokenVisible),
                !self.show_token,
            )
            .label("Token")
            .on_input(Message::TokenInput),
        )
        .push(
            widget::text_input("panel", &self.form.switch_label)
                .label("Switch label (empty = all switches)")
                .on_input(Message::LabelInput),
        )
        .push(
            widget::row(Vec::<Element<'_, Message>>::new())
                .spacing(8)
                .push(widget::button::standard("Cancel").on_press(Message::CancelSettings))
                .push(widget::button::suggested("Save").on_press(Message::SaveSettings)),
        )
        .into()
    }

    fn popup_view(&self) -> Element<'_, Message> {
        let content = match self.mode {
            Mode::List => self.list_view(),
            Mode::Settings => self.settings_view(),
        };
        self.core.applet.popup_container(content).limits(popup_limits()).into()
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
        let (cfg, needs_setup, error) = init_config();
        let app = Self {
            core,
            configured: !needs_setup,
            mode: if needs_setup { Mode::Settings } else { Mode::List },
            form: cfg,
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
                self.mode = if self.configured { Mode::List } else { Mode::Settings };
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
                return if self.configured {
                    Task::batch([open, monitor, fetch_task()])
                } else {
                    Task::batch([open, monitor])
                };
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
                if let Ok(c) = load_config() {
                    self.form = c;
                }
                self.show_token = false;
                self.error = None;
                self.mode = Mode::Settings;
            }
            Message::CancelSettings => {
                self.error = None;
                self.mode = Mode::List;
            }
            Message::SaveSettings => {
                self.form.url = self.form.url.trim().to_string();
                self.form.token = self.form.token.trim().to_string();
                self.form.switch_label = self.form.switch_label.trim().to_string();
                if self.form.url.is_empty() || self.form.token.is_empty() {
                    self.error = Some("URL and token are required".into());
                } else if let Err(e) = save_config(&self.form) {
                    self.error = Some(e);
                } else {
                    self.error = None;
                    self.configured = true;
                    self.mode = Mode::List;
                    return fetch_task();
                }
            }
            Message::UrlInput(s) => self.form.url = s,
            Message::TokenInput(s) => self.form.token = s,
            Message::LabelInput(s) => self.form.switch_label = s,
            Message::ToggleTokenVisible => self.show_token = !self.show_token,
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
