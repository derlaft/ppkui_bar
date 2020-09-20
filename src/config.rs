use serde_derive::Deserialize;

use toml;

use std::io::Read;

use std::str::FromStr;

use css_color::Rgba;

#[derive(Deserialize, Clone)]
pub struct Config {
    pub height: u32,
    pub font: String,
    button_color: String,
    button_hover_color: String,
    text_color: String,
    pub buttons: Vec<Button>,
}

#[derive(Debug)]
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

pub fn parse(args: impl Iterator<Item = String>) -> Result<Config, String> {
    let mut config_file = String::from("/etc/pepekroll/bar.conf");

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

    if config.font == "" {
        config.font = String::from("./assets/panel.ttf");
    };

    if config.buttons.len() == 0 {
        return Err("no buttons defined".into());
    }

    Ok(config)
}
