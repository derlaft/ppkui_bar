use serde_derive::Deserialize;

use toml;

use std::io;
use std::io::prelude::*;
use std::io::Read;

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

impl LauncherConfig {
    pub fn get_options(&self) -> Vec<String> {
        let stdin = io::stdin();
        stdin.lock().lines().collect::<Result<_, _>>().unwrap()
    }
}

pub fn parse_bar(args: impl Iterator<Item = String>) -> Result<Config, String> {
    let mut config_file = String::from("/etc/ppkui/bar.conf");

    // skip the binary name
    let mut args = args.skip(1);

    // parse cmdline arguments
    loop {
        match args.next().as_deref() {
            // config file location
            Some("-c") | Some("--config") => {
                let arg = args.next();

                if arg.is_some() {
                    config_file = arg.unwrap();
                }
            }

            Some(arg) => return Err(format!("invalid arg '{}'", arg)),

            None => break,
        }
    }

    let mut config_data = Vec::new();
    std::fs::File::open(config_file)
        .unwrap()
        .read_to_end(&mut config_data)
        .unwrap();

    let mut config: Config = toml::from_slice(config_data.as_slice()).unwrap();

    let mut bar_config = match config.bar {
        None => return Err(format!("Bar section is not present")),
        Some(x) => x,
    };

    if bar_config.font == "" {
        bar_config.font = String::from("./assets/panel.ttf");
    };

    if bar_config.buttons.len() == 0 {
        return Err("no buttons defined".into());
    }

    config.bar = Some(bar_config);

    Ok(config)
}

pub fn parse_menu(args: impl Iterator<Item = String>) -> Result<Config, String> {
    let mut config_file = String::from("/etc/ppkui/launcher.conf");
    let mut prompt = None;

    // skip the binary name
    let mut args = args.skip(1);

    // parse cmdline arguments
    loop {
        match args.next().as_deref() {
            // config file location
            Some("-c") | Some("--config") => {
                let arg = args.next();

                if arg.is_some() {
                    config_file = arg.unwrap();
                }
            }

            // TODO
            Some("-i") => {}

            // TODO - number of lines
            Some("-l") | Some("--lines") => {
                args.next();
            }

            // TODO - font
            Some("-fn") => {
                args.next();
            }

            Some("-p") | Some("--prompt") => {
                let arg = args.next();

                if arg.is_some() {
                    prompt = Some(arg.unwrap());
                }
            }

            Some(arg) => return Err(format!("invalid arg '{}'", arg)),

            None => break,
        }
    }

    let mut config_data = Vec::new();
    std::fs::File::open(config_file)
        .unwrap()
        .read_to_end(&mut config_data)
        .unwrap();

    let mut config: Config = toml::from_slice(config_data.as_slice()).unwrap();

    let mut launcher_config = match config.launcher {
        None => return Err(format!("Launcher section is not present")),
        Some(x) => x,
    };

    if launcher_config.font == "" {
        // TODO
        launcher_config.font = String::from("sans");
    };

    if prompt.is_some() {
        launcher_config.prompt = prompt
    }

    config.launcher = Some(launcher_config);

    Ok(config)
}
