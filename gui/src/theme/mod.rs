pub mod color;

use iced::{
    application,
    widget::{
        self, button, container, progress_bar,
        rule::{Appearance, FillMode},
        slider, text, text_input,
    },
};

#[derive(Debug, Copy, Clone, Eq, PartialEq, Default)]
pub enum Theme {
    #[default]
    Dark,
}

impl application::StyleSheet for Theme {
    type Style = ();

    fn appearance(&self, _style: &Self::Style) -> application::Appearance {
        match self {
            Theme::Dark => application::Appearance {
                background_color: color::LIGHT_BLACK,
                text_color: color::WHITE,
            },
        }
    }
}

#[derive(Clone, Copy, Default)]
pub enum Text {
    #[default]
    Default,
    Color(iced::Color),
}

impl From<iced::Color> for Text {
    fn from(color: iced::Color) -> Self {
        Text::Color(color)
    }
}

impl text::StyleSheet for Theme {
    type Style = Text;

    fn appearance(&self, style: Self::Style) -> text::Appearance {
        match style {
            Text::Default => Default::default(),
            Text::Color(c) => text::Appearance { color: Some(c) },
        }
    }
}

#[derive(Debug, Copy, Clone, Default)]
pub enum Container {
    #[default]
    Frame,
}

impl container::StyleSheet for Theme {
    type Style = Container;
    fn appearance(&self, style: &Self::Style) -> iced::widget::container::Appearance {
        match self {
            Theme::Dark => match style {
                Container::Frame => container::Appearance {
                    border: iced::Border {
                        color: color::WHITE,
                        width: 2.0,
                        radius: 5.0.into(),
                    },
                    ..container::Appearance::default()
                },
            },
        }
    }
}

#[derive(Default)]
pub enum Button {
    #[default]
    Primary,
}

impl button::StyleSheet for Theme {
    type Style = Button;

    fn active(&self, style: &Self::Style) -> button::Appearance {
        match self {
            Theme::Dark => match style {
                Button::Primary => button::Appearance {
                    shadow_offset: iced::Vector::default(),
                    background: Some(iced::Color::TRANSPARENT.into()),
                    text_color: color::GREY_2,
                    border: iced::Border {
                        color: color::GREY_7,
                        width: 1.0,
                        radius: 25.0.into(),
                    },
                    ..button::Appearance::default()
                },
            },
        }
    }

    fn hovered(&self, style: &Self::Style) -> button::Appearance {
        match self {
            Theme::Dark => match style {
                Button::Primary => button::Appearance {
                    shadow_offset: iced::Vector::default(),
                    background: Some(color::GREEN.into()),
                    text_color: color::LIGHT_BLACK,
                    border: iced::Border {
                        color: color::TRANSPARENT,
                        width: 0.0,
                        radius: 25.0.into(),
                    },
                    ..button::Appearance::default()
                },
            },
        }
    }
}

#[derive(Debug, Copy, Clone, Default)]
pub enum Form {
    #[default]
    Simple,
}

impl text_input::StyleSheet for Theme {
    type Style = Form;
    fn active(&self, style: &Self::Style) -> text_input::Appearance {
        match style {
            Form::Simple => text_input::Appearance {
                icon_color: color::GREY_7,
                background: iced::Background::Color(iced::Color::TRANSPARENT),
                border: iced::Border {
                    color: color::GREY_7,
                    width: 1.0,
                    radius: 25.0.into(),
                },
            },
        }
    }

    fn disabled(&self, style: &Self::Style) -> text_input::Appearance {
        text_input::Appearance {
            ..self.active(style)
        }
    }

    fn focused(&self, style: &Self::Style) -> text_input::Appearance {
        text_input::Appearance {
            ..self.active(style)
        }
    }

    fn disabled_color(&self, _style: &Self::Style) -> iced::Color {
        color::GREY_7
    }

    fn placeholder_color(&self, _style: &Self::Style) -> iced::Color {
        color::GREY_7
    }

    fn value_color(&self, _style: &Self::Style) -> iced::Color {
        color::GREY_2
    }

    fn selection_color(&self, _style: &Self::Style) -> iced::Color {
        color::GREEN
    }
}

#[derive(Debug, Copy, Clone, Default)]
pub enum ProgressBar {
    #[default]
    Simple,
}

impl progress_bar::StyleSheet for Theme {
    type Style = ProgressBar;
    fn appearance(&self, _style: &Self::Style) -> progress_bar::Appearance {
        progress_bar::Appearance {
            background: color::GREY_6.into(),
            bar: color::GREEN.into(),
            border_radius: 10.0.into(),
        }
    }
}

#[derive(Debug, Copy, Clone, Default)]
pub enum Slider {
    #[default]
    Simple,
}

impl slider::StyleSheet for Theme {
    type Style = Slider;
    fn active(&self, _style: &Self::Style) -> slider::Appearance {
        let handle = slider::Handle {
            shape: slider::HandleShape::Rectangle {
                width: 8,
                border_radius: 4.0.into(),
            },
            color: color::BLACK,
            border_color: color::GREEN,
            border_width: 1.0,
        };
        slider::Appearance {
            rail: slider::Rail {
                colors: (color::GREEN, iced::Color::TRANSPARENT),
                border_radius: 4.0.into(),
                width: 2.0,
            },
            handle,
        }
    }
    fn hovered(&self, _style: &Self::Style) -> slider::Appearance {
        let handle = slider::Handle {
            shape: slider::HandleShape::Rectangle {
                width: 8,
                border_radius: 4.0.into(),
            },
            color: color::GREEN,
            border_color: color::GREEN,
            border_width: 1.0,
        };
        slider::Appearance {
            rail: slider::Rail {
                colors: (color::GREEN, iced::Color::TRANSPARENT),
                border_radius: 4.0.into(),
                width: 2.0,
            },
            handle,
        }
    }
    fn dragging(&self, _style: &Self::Style) -> slider::Appearance {
        let handle = slider::Handle {
            shape: slider::HandleShape::Rectangle {
                width: 8,
                border_radius: 4.0.into(),
            },
            color: color::GREEN,
            border_color: color::GREEN,
            border_width: 1.0,
        };
        slider::Appearance {
            rail: slider::Rail {
                colors: (color::GREEN, iced::Color::TRANSPARENT),
                border_radius: 4.0.into(),
                width: 2.0,
            },
            handle,
        }
    }
}

#[derive(Debug, Copy, Clone, Default)]
pub enum Rule {
    #[default]
    Simple,
    Light,
}

impl widget::rule::StyleSheet for Theme {
    type Style = Rule;

    fn appearance(&self, style: &Self::Style) -> Appearance {
        widget::rule::Appearance {
            color: color::WHITE,
            width: match style {
                Rule::Simple => 2,
                Rule::Light => 1,
            },
            radius: Default::default(),
            fill_mode: FillMode::Full,
        }
    }
}
