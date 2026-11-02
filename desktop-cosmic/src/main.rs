use cosmic::applet::{menu_button, padded_control};
use cosmic::iced::{window::Id, Length, Limits, Rectangle, Vector};
use cosmic::prelude::*;
use cosmic::surface::action::{app_popup, destroy_popup};
use cosmic::widget;
use ha_core::client;
use ha_core::config::{init_config, load_config};
use ha_core::model::{fmt_temp, group_by_room, parse_temperature, step_temperature, Data, Entity, Room};
use std::collections::HashMap;

const WARM: cosmic::iced::Color = cosmic::iced::Color::from_rgb(0.93, 0.45, 0.15);
const COOL: cosmic::iced::Color = cosmic::iced::Color::from_rgb(0.30, 0.62, 0.95);

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

async fn fetch_all() -> Result<Data, String> {
    client::fetch_all(&load_config()?).await
}

async fn set_state(id: String, on: bool) -> Result<Data, String> {
    client::set_state(&load_config()?, &id, on).await
}

async fn set_temperature(id: String, t: f64) -> Result<Data, String> {
    client::set_temperature(&load_config()?, &id, t).await
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
        let t = widget::toggler(e.is_on()).on_toggle(move |_| Message::Toggle(id.clone()));
        widget::settings::item(e.name.clone(), t).into()
    }

    fn room_cells<'a>(&'a self, room: &Room<'a>) -> [Element<'a, Message>; 3] {
        [
            cell(room.lights.iter().copied().map(|e| self.toggle_item(e)).collect()),
            cell(room.switches.iter().copied().map(|e| self.toggle_item(e)).collect()),
            cell(room.climate.iter().copied().map(|e| self.climate_row(e)).collect()),
        ]
    }

    fn apply_temperature(&mut self, id: String, t: f64) -> Task<cosmic::Action<Message>> {
        self.edits.remove(&id);
        let Some(e) = self.data.find_mut(&id) else {
            return Task::none();
        };
        let t = e.clamp_temperature(t);
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
                if let Some(e) = self.data.find_mut(&id) {
                    on = !e.is_on();
                    e.state = if on { "on".into() } else { "off".into() };
                }
                return Task::perform(set_state(id, on), |r| cosmic::Action::App(Message::Loaded(r)));
            }
            Message::TempEdit(id, text) => {
                self.edits.insert(id, text);
            }
            Message::TempSubmit(id) => {
                let parsed = self.edits.remove(&id).and_then(|t| parse_temperature(&t));
                if let Some(t) = parsed {
                    return self.apply_temperature(id, t);
                }
            }
            Message::TempStep(id, dir) => {
                let current = self.data.find_mut(&id).and_then(|e| e.target);
                if let Some(t) = current {
                    return self.apply_temperature(id, step_temperature(t, dir));
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
