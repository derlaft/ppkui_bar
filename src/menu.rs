mod libwaylandsfpanel;

use smithay_client_toolkit::reexports::protocols::wlr::unstable::layer_shell::v1::client::{
    zwlr_layer_shell_v1, zwlr_layer_surface_v1,
};

use andrew::{shapes::rectangle, text, Canvas};

use std::{
    cmp, env,
    io::{self, Read, Write},
    process,
};

struct Menu {
    /// current scroll offset
    list_offset: i32,
    /// pointer tracking
    pointer_start: Option<libwaylandsfpanel::PointerPosition>,
    pointer_current: Option<libwaylandsfpanel::PointerPosition>,
    pointer_engaged: bool,
    /// registered buttons
    click_targets: Vec<ClickTarget>,
    /// static config stuff
    font_data: Vec<u8>,
    cfg: Config,
    launcher_config: LauncherConfig,
    colors: ColorConfig,
    options: Vec<String>,
}

mod config;
use config::{ColorConfig, Config, LauncherConfig};

impl Clone for Menu {
    fn clone(&self) -> Self {
        Menu {
            list_offset: 0,
            pointer_engaged: false,
            pointer_start: None,
            pointer_current: None,
            click_targets: vec![],
            font_data: self.font_data.clone(),
            cfg: self.cfg.clone(),
            launcher_config: self.launcher_config.clone(),
            colors: self.colors.clone(),
            options: self.options.clone(),
        }
    }
}

impl Menu {
    fn check_execute_click(&mut self) -> bool {
        let mut matching_click_handler = None;

        if self.pointer_engaged {
            for click_target in &self.click_targets {
                if is_clicking(
                    self.pointer_start,
                    self.pointer_current,
                    self.pointer_engaged,
                ) {
                    if let Some(handler) = click_target.process_click(self.pointer_current.unwrap())
                    {
                        matching_click_handler = Some(handler);
                    }
                }
            }

            match matching_click_handler {
                Some(ClickHandler::Selected(cmd)) => {
                    io::stdout().write_all(cmd.as_bytes()).unwrap();
                    io::stdout().write_all("\n".as_bytes()).unwrap();
                    io::stdout().flush().unwrap();
                    return true;
                }
                None => {}
            }
        };

        false
    }
}

impl libwaylandsfpanel::Application for Menu {
    fn new() -> Self {
        let cfg = match config::parse_menu(env::args()) {
            Ok(args) => args,
            Err(message) => {
                eprintln!("{}", message);

                process::exit(1);
            }
        };

        let colors = cfg.get_color_config();

        let launcher_config = cfg.clone().launcher.unwrap();

        let options = launcher_config.get_options();

        let mut font_data = Vec::new();
        std::fs::File::open(launcher_config.font.clone())
            .unwrap()
            .read_to_end(&mut font_data)
            .unwrap();

        Menu {
            list_offset: 0,
            pointer_engaged: false,
            pointer_start: None,
            pointer_current: None,
            click_targets: vec![],
            font_data,
            cfg,
            launcher_config,
            colors,
            options,
        }
    }

    fn settings(&self) -> libwaylandsfpanel::ApplicationSettings {
        let want_height = cmp::min(
            self.launcher_config.line_height * self.launcher_config.max_lines,
            (self.options.len() as u32) * self.launcher_config.line_height,
        );

        libwaylandsfpanel::ApplicationSettings {
            namespace: String::from("ppkui_bar"),
            layer: zwlr_layer_shell_v1::Layer::Overlay,
            size: libwaylandsfpanel::WindowSize(0, want_height),
            exclusive_zone: 0,
            margins: (0, 0, 0, 0),
            anchor: zwlr_layer_surface_v1::Anchor::Bottom
                | zwlr_layer_surface_v1::Anchor::Left
                | zwlr_layer_surface_v1::Anchor::Right,
        }
    }

    fn draw(&mut self, size: libwaylandsfpanel::WindowSize, buf: &mut [u8]) {
        let width = size.0 as i32;
        let height = size.1 as i32;

        let mut canvas = andrew::Canvas::new(
            buf,
            width as usize,
            height as usize,
            4 * width as usize,
            andrew::Endian::native(),
        );

        // Draw buttons
        let button_height = self.launcher_config.line_height as usize;
        let text_h = (button_height as f32 / 1.5).ceil();

        let swipe_dist = swipe_distance(
            self.pointer_start,
            self.pointer_current,
            self.pointer_engaged,
        )
        .unwrap_or(0);
        let current_offset = self.list_offset + swipe_dist;

        let mut next_draw_at = -current_offset;

        let mut create_button = move |colors: &ColorConfig,
                                      label: String,
                                      font_data: &[u8],
                                      canvas: &mut Canvas,
                                      pointer_engaged: bool,
                                      pointer: Option<libwaylandsfpanel::PointerPosition>,
                                      next_draw_at: &mut i32| {
            // first button? nice

            // ugh, I don't think we can currently draw stuff out of bounds
            // that's a bit disappointing
            if *next_draw_at < 0 || *next_draw_at > height {
                *next_draw_at += button_height as i32;
                return None;
            }
            let current_draw_at = *next_draw_at as usize;

            let mut text = text::Text::new(
                (0, 0),
                colors.text_color,
                font_data,
                text_h,
                1.0,
                label.clone(),
            );
            let text_width = text.get_width();
            // button should take the whole screen
            let button_width = width as usize;
            let block_pos = (0, current_draw_at);
            let text_pos = (
                (button_width - text_width) / 2,
                current_draw_at + ((button_height as f32 - text_h) / 2.) as usize,
            );
            text.pos = text_pos;
            let size = (button_width as usize, button_height as usize);

            // create a click target
            let click_target = ClickTarget {
                position: block_pos,
                size,
                handler: ClickHandler::Selected(label),
            };

            // a very ugly way to check if this click target is hovered
            let hovered = {
                let mut retval = false;
                if let Some(click_position) = pointer {
                    let adj_pos = libwaylandsfpanel::PointerPosition(
                        click_position.0,
                        click_position.1 - swipe_dist as f64,
                    );
                    if let Some(..) = click_target.process_click(adj_pos) {
                        retval = true;
                    }
                };
                retval && pointer_engaged
            };

            let color = match hovered {
                false => colors.button_color,
                true => colors.button_hover_color,
            };

            let block = rectangle::Rectangle::new(block_pos, size, None, Some(color));
            canvas.draw(&block);
            canvas.draw(&text);

            *next_draw_at += button_height as i32;

            Some(click_target)
        };

        for button in self.options.iter().cloned() {
            let click_target = create_button(
                &self.colors,
                button,
                &self.font_data,
                &mut canvas,
                self.pointer_engaged,
                self.pointer_start,
                &mut next_draw_at,
            );

            if click_target.is_some() {
                self.click_targets.push(click_target.unwrap());
            }
        }
    }

    fn input_start_gesture(
        &mut self,
        pos: libwaylandsfpanel::PointerPosition,
    ) -> Option<libwaylandsfpanel::RenderEvent> {
        self.pointer_engaged = true;
        self.input_movement(pos)
    }

    fn input_stop_gesture(&mut self) -> Option<libwaylandsfpanel::RenderEvent> {
        self.pointer_engaged = false;
        self.pointer_start = None;
        self.pointer_current = None;

        Some(libwaylandsfpanel::RenderEvent::Render)
    }

    fn input_movement(
        &mut self,
        pos: libwaylandsfpanel::PointerPosition,
    ) -> Option<libwaylandsfpanel::RenderEvent> {
        if self.pointer_start.is_none() && self.pointer_engaged {
            self.pointer_start = Some(pos);
        };
        self.pointer_current = Some(pos);

        Some(libwaylandsfpanel::RenderEvent::Render)
    }

    fn input_commit_gesture(&mut self) -> Option<libwaylandsfpanel::RenderEvent> {
        if self.check_execute_click() {
            return Some(libwaylandsfpanel::RenderEvent::Closed);
        }

        if self.pointer_engaged {
            self.list_offset += swipe_distance(
                self.pointer_start,
                self.pointer_current,
                self.pointer_engaged,
            )
            .unwrap_or(0);

            // limit scrolling up
            self.list_offset = std::cmp::max(0, self.list_offset);

            let draw_lines = std::cmp::min(
                self.options.len() as i32,
                self.launcher_config.max_lines as i32,
            );

            // limit scrolling down
            self.list_offset = std::cmp::min(
                self.list_offset,
                (self.options.len() as i32 - draw_lines)
                    * (self.launcher_config.line_height as i32),
            );

            self.pointer_engaged = false;
        }

        self.pointer_start = None;

        Some(libwaylandsfpanel::RenderEvent::Render)
    }
}

fn swipe_distance(
    start: Option<libwaylandsfpanel::PointerPosition>,
    current: Option<libwaylandsfpanel::PointerPosition>,
    engaged: bool,
) -> Option<i32> {
    if start.is_none() || current.is_none() || !engaged {
        None
    } else if start.unwrap() != current.unwrap() {
        Some((start.unwrap().1 - current.unwrap().1) as i32)
    } else {
        None
    }
}

#[derive(Clone)]
enum ClickHandler {
    /// Run command
    Selected(String),
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

fn is_clicking(
    start: Option<libwaylandsfpanel::PointerPosition>,
    current: Option<libwaylandsfpanel::PointerPosition>,
    engaged: bool,
) -> bool {
    if start.is_none() || current.is_none() || !engaged {
        false
    } else if start.unwrap() == current.unwrap() {
        true
    } else {
        false
    }
}

fn main() {
    libwaylandsfpanel::run_application::<Menu>();
}
