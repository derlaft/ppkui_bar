mod libwaylandsfpanel;

use smithay_client_toolkit::reexports::protocols::wlr::unstable::layer_shell::v1::client::{
    zwlr_layer_shell_v1, zwlr_layer_surface_v1,
};

use andrew::{shapes::rectangle, text, Canvas};
use std::env;
use std::io::Read;
use std::process;
use std::process::Command;

struct Bar {
    height: u32,
    /// X, Y coordinates of current cursor position
    pointer_location: Option<libwaylandsfpanel::PointerPosition>,
    pointer_engaged: bool,
    click_targets: Vec<ClickTarget>,
    font_data: Vec<u8>,
    cfg: Config,
    colors: ColorConfig,
}

mod config;
use config::{ColorConfig, Config};

impl Clone for Bar {
    fn clone(&self) -> Self {
        Bar {
            height: 32,
            pointer_engaged: false,
            pointer_location: None,
            click_targets: vec![],
            font_data: self.font_data.clone(),
            cfg: self.cfg.clone(),
            colors: self.colors.clone(),
        }
    }
}

impl Bar {
    fn check_execute_click(&mut self) {
        let mut matching_click_handler = None;
        for click_target in &self.click_targets {
            if let Some(click_position) = self.pointer_location {
                if let Some(handler) = click_target.process_click(click_position) {
                    matching_click_handler = Some(handler);
                }
            }
        }

        match matching_click_handler {
            Some(ClickHandler::RunCommand(cmd)) => {
                match Command::new("/usr/bin/setsid")
                    .arg("--fork")
                    .arg("/bin/sh")
                    .arg("-c")
                    .arg(cmd)
                    .spawn()
                {
                    Ok(mut child) => match child.wait() {
                        Ok(..) => (),
                        Err(e) => eprintln!("{:?}", e),
                    },
                    Err(e) => eprintln!("{:?}", e),
                }
            }
            None => {}
        }
    }
}

impl libwaylandsfpanel::Application for Bar {
    fn new() -> Self {
        let cfg = match config::parse(env::args()) {
            Ok(args) => args,
            Err(message) => {
                eprintln!("{}", message);

                process::exit(1);
            }
        };

        let colors = cfg.get_color_config();

        let mut font_data = Vec::new();
        std::fs::File::open(cfg.font.clone())
            .unwrap()
            .read_to_end(&mut font_data)
            .unwrap();

        Bar {
            height: 32,
            pointer_engaged: false,
            pointer_location: None,
            click_targets: vec![],
            font_data,
            cfg,
            colors,
        }
    }

    fn settings(&self) -> libwaylandsfpanel::ApplicationSettings {
        libwaylandsfpanel::ApplicationSettings {
            namespace: String::from("ppkui_bar"),
            layer: zwlr_layer_shell_v1::Layer::Overlay,
            size: libwaylandsfpanel::WindowSize(0, self.height),
            exclusive_zone: self.height as i32,
            margins: (0, 0, 0, 0),
            anchor: zwlr_layer_surface_v1::Anchor::Bottom
                | zwlr_layer_surface_v1::Anchor::Left
                | zwlr_layer_surface_v1::Anchor::Right,
        }
    }

    fn draw(&mut self, size: libwaylandsfpanel::WindowSize, buf: &mut [u8]) {
        let width = size.0 as i32;
        let height = size.1 as i32;

        let text_h = height as f32 / 2.;

        let mut canvas = andrew::Canvas::new(
            buf,
            width as usize,
            height as usize,
            4 * width as usize,
            andrew::Endian::native(),
        );

        // Draw buttons
        let mut next_draw_at = 0;
        let per_button = (width as usize) / self.cfg.buttons.len();

        let mut create_button =
            move |colors: &ColorConfig,
                  text: String,
                  action: String,
                  font_data: &[u8],
                  canvas: &mut Canvas,
                  pointer_engaged: bool,
                  pointer: Option<libwaylandsfpanel::PointerPosition>| {
                let mut text =
                    text::Text::new((0, 0), colors.text_color, font_data, text_h, 1.0, text);
                let text_width = text.get_width();
                let button_width = per_button;
                let block_height = height as usize;
                let block_pos = (next_draw_at, 0);
                let text_pos = (
                    block_pos.0 + (per_button - text_width) / 2,
                    ((block_height as f32 - text_h) / 2.) as usize,
                );
                text.pos = text_pos;
                let size = (button_width as usize, block_height as usize);

                // create a click target
                let click_target = ClickTarget {
                    position: block_pos,
                    size,
                    handler: ClickHandler::RunCommand(action),
                };

                // a very ugly way to check if this click target is hovered
                let hovered = {
                    let mut retval = false;
                    if let Some(click_position) = pointer {
                        if let Some(..) = click_target.process_click(click_position) {
                            retval = true;
                        }
                    };
                    retval && pointer_engaged
                };

                // TODO make colors configurable
                let color = match hovered {
                    false => colors.button_color,
                    true => colors.button_hover_color,
                };

                let block = rectangle::Rectangle::new(block_pos, size, None, Some(color));
                canvas.draw(&block);
                canvas.draw(&text);

                next_draw_at += per_button;

                click_target
            };

        for button in self.cfg.buttons.iter().cloned() {
            let click_target = create_button(
                &self.colors,
                button.text,
                button.command,
                &self.font_data,
                &mut canvas,
                self.pointer_engaged,
                self.pointer_location,
            );

            self.click_targets.push(click_target);
        }
    }

    fn input_start_gesture(
        &mut self,
        pos: libwaylandsfpanel::PointerPosition,
    ) -> Option<libwaylandsfpanel::RenderEvent> {
        self.pointer_engaged = true;
        self.pointer_location = Some(pos);

        Some(libwaylandsfpanel::RenderEvent::Render)
    }

    fn input_stop_gesture(&mut self) -> Option<libwaylandsfpanel::RenderEvent> {
        self.pointer_engaged = false;
        self.pointer_location = None; // TODO: maybe not
        Some(libwaylandsfpanel::RenderEvent::Render)
    }

    fn input_movement(
        &mut self,
        pos: libwaylandsfpanel::PointerPosition,
    ) -> Option<libwaylandsfpanel::RenderEvent> {
        self.pointer_location = Some(pos);
        Some(libwaylandsfpanel::RenderEvent::Render)
    }
    fn input_commit_gesture(&mut self) -> Option<libwaylandsfpanel::RenderEvent> {
        self.check_execute_click();
        self.pointer_engaged = false;
        Some(libwaylandsfpanel::RenderEvent::Render)
    }
}

#[derive(Clone)]
enum ClickHandler {
    /// Run command
    RunCommand(String),
}

struct ClickTarget {
    position: (usize, usize),
    size: (usize, usize),
    handler: ClickHandler,
}

impl ClickTarget {
    fn process_click(
        &self,
        click_position: libwaylandsfpanel::PointerPosition,
    ) -> Option<ClickHandler> {
        let click_x = click_position.0;
        let click_y = click_position.1;

        let (position_x, position_y) = (self.position.0 as f64, self.position.1 as f64);
        let (size_x, size_y) = (self.size.0 as f64, self.size.1 as f64);

        if click_x >= position_x
            && click_x < position_x + size_x
            && click_y >= position_y
            && click_y < position_y + size_y
        {
            Some(self.handler.clone())
        } else {
            None
        }
    }
}

fn main() {
    libwaylandsfpanel::run_application::<Bar>();
}
