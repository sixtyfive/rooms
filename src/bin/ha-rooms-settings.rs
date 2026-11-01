use cosmic::iced::{Length, Size};
use cosmic::prelude::*;
use cosmic::widget;
use ha_rooms::config::{init_config, save_config, Config};

const TITLE: &str = "HomeAssistant Applet Settings";

struct App {
    core: cosmic::Core,
    form: Config,
    show_token: bool,
    error: Option<String>,
}

#[derive(Debug, Clone)]
enum Message {
    Url(String),
    Token(String),
    LightLabel(String),
    SwitchLabel(String),
    ClimateLabel(String),
    ToggleToken,
}

impl cosmic::Application for App {
    type Executor = cosmic::executor::Default;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = "dev.local.HaRoomsSettings";

    fn core(&self) -> &cosmic::Core {
        &self.core
    }
    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    fn init(core: cosmic::Core, _flags: ()) -> (Self, Task<cosmic::Action<Message>>) {
        let (form, _, error) = init_config();
        let mut app = Self { core, form, show_token: false, error };
        app.set_header_title(TITLE.into());
        let task = match app.core.main_window_id() {
            Some(id) => app.set_window_title(TITLE.into(), id),
            None => Task::none(),
        };
        (app, task)
    }

    fn view(&self) -> Element<'_, Message> {
        let input_width = Length::Fixed(300.0);
        let connection = widget::settings::section()
            .title("Home Assistant")
            .add(widget::settings::item(
                "URL",
                widget::text_input("http://homeassistant.local:8123", &self.form.url)
                    .on_input(Message::Url)
                    .width(input_width),
            ))
            .add(widget::settings::item(
                "Long-lived access token",
                widget::secure_input(
                    "Token",
                    &self.form.token,
                    Some(Message::ToggleToken),
                    !self.show_token,
                )
                .on_input(Message::Token)
                .width(input_width),
            ));
        let filters = widget::settings::section()
            .title("Panel filters")
            .add(widget::settings::item(
                "Light label",
                widget::text_input("all lights", &self.form.light_label)
                    .on_input(Message::LightLabel)
                    .width(input_width),
            ))
            .add(widget::settings::item(
                "Switch label",
                widget::text_input("all switches", &self.form.switch_label)
                    .on_input(Message::SwitchLabel)
                    .width(input_width),
            ))
            .add(widget::settings::item(
                "Climate label",
                widget::text_input("all thermostats", &self.form.climate_label)
                    .on_input(Message::ClimateLabel)
                    .width(input_width),
            ));

        let mut children: Vec<Element<'_, Message>> = vec![connection.into(), filters.into()];
        children.push(
            widget::text::caption(
                "Only entities carrying the given Home Assistant label are shown. Leave a label empty to show everything.",
            )
            .into(),
        );
        if let Some(e) = &self.error {
            children.push(widget::text(e.as_str()).into());
        }
        widget::scrollable(widget::container(widget::settings::view_column(children)).padding(16)).into()
    }

    fn update(&mut self, message: Message) -> Task<cosmic::Action<Message>> {
        match message {
            Message::Url(s) => self.form.url = s,
            Message::Token(s) => self.form.token = s,
            Message::LightLabel(s) => self.form.light_label = s,
            Message::SwitchLabel(s) => self.form.switch_label = s,
            Message::ClimateLabel(s) => self.form.climate_label = s,
            Message::ToggleToken => {
                self.show_token = !self.show_token;
                return Task::none();
            }
        }
        self.error = save_config(&self.form).err();
        Task::none()
    }
}

fn main() -> cosmic::iced::Result {
    cosmic::app::run::<App>(
        cosmic::app::Settings::default().size(Size::new(620.0, 480.0)),
        (),
    )
}
