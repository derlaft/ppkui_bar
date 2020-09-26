use serde_derive::Deserialize;

use std::str::FromStr;

use css_color::Rgba;

#[derive(Deserialize, Clone)]
pub struct Config {
    button_color: String,
    button_hover_color: String,
    text_color: String,
    pub bar: Option<BarConfig>,
    pub launcher: Option<LauncherConfig>,
}

#[derive(Deserialize, Clone)]
pub struct BarConfig {
    pub height: u32,
    pub font: String,
    pub buttons: Vec<Button>,
}

#[derive(Deserialize, Clone)]
pub struct LauncherConfig {
    pub prompt: Option<String>,
    pub line_height: u32,
    pub max_lines: u32,
    pub font: String,
}

#[derive(Debug, Clone)]
pub struct ColorConfig {
    pub text_color: [u8; 4],
    pub button_color: [u8; 4],
    pub button_hover_color: [u8; 4],
}

#[derive(Deserialize, Clone)]
pub struct Button {
    pub text: String,
    pub command: String,
}

fn rgba_to_color(i: Rgba) -> [u8; 4] {
    return [
        (i.alpha * 255.) as u8,
        (i.red * 255.) as u8,
        (i.green * 255.) as u8,
        (i.blue * 255.) as u8,
    ];
}

impl Config {
    pub fn get_color_config(&self) -> ColorConfig {
        ColorConfig {
            text_color: rgba_to_color(Rgba::from_str(&self.text_color).unwrap()),
            button_color: rgba_to_color(Rgba::from_str(&self.button_color).unwrap()),
            button_hover_color: rgba_to_color(Rgba::from_str(&self.button_hover_color).unwrap()),
        }
    }
}
