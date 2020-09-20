use andrew::{
    shapes::rectangle,
    text::{self, fontconfig},
    Canvas,
};

use smithay_client_toolkit::{
    default_environment,
    environment::SimpleGlobal,
    init_default_environment,
    output::{with_output_info, OutputInfo},
    reexports::{
        calloop,
        client::protocol::{
            wl_output,
            wl_pointer::{self, ButtonState},
            wl_shm, wl_surface, wl_touch,
        },
        client::{Attached, Main},
        protocols::wlr::unstable::layer_shell::v1::client::{
            zwlr_layer_shell_v1, zwlr_layer_surface_v1,
        },
    },
    seat,
    shm::DoubleMemPool,
    WaylandSource,
};

use std::{
    cell::{Cell, RefCell},
    env,
    io::{self, Read, Seek, SeekFrom, Write},
    process::{self, Command},
    rc::Rc,
};

mod config;
use config::Config;

const FONT_COLOR: [u8; 4] = [255, 255, 255, 255];

default_environment!(Env,
    fields = [
        layer_shell: SimpleGlobal<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    ],
    singles = [
        zwlr_layer_shell_v1::ZwlrLayerShellV1 => layer_shell
    ],
);

#[derive(PartialEq, Copy, Clone)]
enum RenderEvent {
    Configure { width: u32, height: u32 },
    Closed,
}

struct Surface {
    cfg: Config,
    surface: wl_surface::WlSurface,
    layer_surface: Main<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1>,
    next_render_event: Rc<Cell<Option<RenderEvent>>>,
    pools: DoubleMemPool,
    dimensions: (u32, u32),
    /// X, Y coordinates of current cursor position
    pointer_location: Option<(f64, f64)>,
    pointer_engaged: bool,
    /// User requested exit
    should_exit: bool,
    click_targets: Vec<ClickTarget>,
    font_data: Vec<u8>,
}

struct ClickTarget {
    position: (usize, usize),
    size: (usize, usize),
    handler: ClickHandler,
}

#[derive(Clone)]
enum ClickHandler {
    /// Run command
    RunCommand(String),
}

impl Surface {
    fn new(
        cfg: Config,
        output: &wl_output::WlOutput,
        surface: wl_surface::WlSurface,
        layer_shell: &Attached<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
        pools: DoubleMemPool,
    ) -> Self {
        let layer_surface = layer_shell.get_layer_surface(
            &surface,
            Some(&output),
            zwlr_layer_shell_v1::Layer::Overlay,
            "panel".to_owned(),
        );

        let height = cfg.height;
        layer_surface.set_size(0, height);
        layer_surface.set_anchor(
            zwlr_layer_surface_v1::Anchor::Bottom
                | zwlr_layer_surface_v1::Anchor::Left
                | zwlr_layer_surface_v1::Anchor::Right,
        );
        layer_surface.set_exclusive_zone(height as i32);

        let next_render_event = Rc::new(Cell::new(None::<RenderEvent>));
        let next_render_event_handle = Rc::clone(&next_render_event);
        layer_surface.quick_assign(move |layer_surface, event, _| {
            match (event, next_render_event_handle.get()) {
                (zwlr_layer_surface_v1::Event::Closed, _) => {
                    next_render_event_handle.set(Some(RenderEvent::Closed));
                }
                (
                    zwlr_layer_surface_v1::Event::Configure {
                        serial,
                        width,
                        height,
                    },
                    next,
                ) if next != Some(RenderEvent::Closed) => {
                    layer_surface.ack_configure(serial);
                    next_render_event_handle.set(Some(RenderEvent::Configure { width, height }));
                }
                (_, _) => {}
            }
        });

        // Commit so that the server will send a configure event
        surface.commit();

        let mut font_data = Vec::new();
        std::fs::File::open("./assets/panel.ttf")
            .unwrap()
            .read_to_end(&mut font_data)
            .unwrap();

        Self {
            cfg,
            surface,
            layer_surface,
            next_render_event,
            pools,
            dimensions: (0, 0),
            pointer_location: None,
            pointer_engaged: false,
            should_exit: false,
            click_targets: vec![],
            font_data,
        }
    }

    /// Handles any events that have occurred since the last call, redrawing if needed.
    /// Returns true if the surface should be dropped.
    fn handle_events(&mut self) -> bool {
        match self.next_render_event.take() {
            Some(RenderEvent::Closed) => true,
            Some(RenderEvent::Configure { width, height }) => {
                self.dimensions = (width, height);
                self.draw();
                false
            }
            None => self.should_exit,
        }
    }

    fn handle_touch_event(&mut self, event: &wl_touch::Event) {
        match event {
            wl_touch::Event::Cancel => {
                self.pointer_engaged = false;
                self.draw();
            }
            wl_touch::Event::Down { x, y, .. } => {
                self.pointer_engaged = true;
                self.pointer_location = Some((*x, *y));
                self.draw();
            }
            wl_touch::Event::Motion { x, y, .. } => {
                self.pointer_location = Some((*x, *y));
                self.draw();
            }
            wl_touch::Event::Up { .. } => {
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
                        match Command::new("/bin/sh").arg("-c").arg(cmd).spawn() {
                            Ok(_) => (),
                            Err(e) => eprintln!("{:?}", e),
                        }
                    }
                    None => {}
                }

                self.pointer_engaged = false;
                self.draw();
            }
            _ => {}
        }
    }

    fn handle_pointer_event(&mut self, event: &wl_pointer::Event) {
        match event {
            wl_pointer::Event::Leave { .. } => {
                self.pointer_location = None;
                self.pointer_engaged = false;
                self.draw();
            }
            wl_pointer::Event::Enter {
                surface_x,
                surface_y,
                ..
            }
            | wl_pointer::Event::Motion {
                surface_x,
                surface_y,
                ..
            } => {
                self.pointer_location = Some((*surface_x, *surface_y));
                self.draw();
            }
            wl_pointer::Event::Button {
                state: ButtonState::Pressed,
                ..
            } => {
                self.pointer_engaged = true;
                self.draw();
            }
            wl_pointer::Event::Button {
                state: ButtonState::Released,
                ..
            } => {
                let mut matching_click_handler = None;

                if self.pointer_engaged {
                    for click_target in &self.click_targets {
                        if let Some(click_position) = self.pointer_location {
                            if let Some(handler) = click_target.process_click(click_position) {
                                matching_click_handler = Some(handler);
                            }
                        }
                    }

                    match matching_click_handler {
                        Some(ClickHandler::RunCommand(cmd)) => {
                            match Command::new("/bin/sh").arg("-c").arg(cmd).spawn() {
                                Ok(_) => (),
                                Err(e) => eprintln!("{:?}", e),
                            }
                        }
                        None => {}
                    }
                };

                self.pointer_engaged = false;

                self.draw();
            }
            _ => {}
        }
    }

    fn draw(&mut self) {
        let pool = match self.pools.pool() {
            Some(pool) => pool,
            None => return,
        };

        let stride = 4 * self.dimensions.0 as i32;
        let width = self.dimensions.0 as i32;
        let height = self.dimensions.1 as i32;

        let text_h = height as f32 / 2.;

        // First make sure the pool is the right size
        pool.resize((stride * height) as usize).unwrap();

        let mut buf: Vec<u8> = vec![255; (4 * width * height) as usize];
        let mut canvas = andrew::Canvas::new(
            &mut buf,
            width as usize,
            height as usize,
            4 * width as usize,
            andrew::Endian::native(),
        );

        // Draw buttons
        let mut next_draw_at = 0;
        let per_button = (width as usize) / self.cfg.buttons.len();

        let mut create_button = move |text: String,
                                      action: String,
                                      font_data: &[u8],
                                      canvas: &mut Canvas,
                                      pointer_engaged: bool,
                                      pointer: Option<(f64, f64)>| {
            let mut text = text::Text::new((0, 0), FONT_COLOR, font_data, text_h, 1.0, text);
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
            let mut color = Some([255, 0, 0, 0]);
            if hovered {
                color = Some([255, 64, 64, 64]);
            };

            let block = rectangle::Rectangle::new(block_pos, size, None, color);
            canvas.draw(&block);
            canvas.draw(&text);

            next_draw_at += per_button;

            click_target
        };

        for button in self.cfg.buttons.iter().cloned() {
            let click_target = create_button(
                button.text,
                button.command,
                &self.font_data,
                &mut canvas,
                self.pointer_engaged,
                self.pointer_location,
            );

            self.click_targets.push(click_target);
        }

        pool.seek(SeekFrom::Start(0)).unwrap();
        pool.write_all(canvas.buffer).unwrap();
        pool.flush().unwrap();

        // Create a new buffer from the pool
        let buffer = pool.buffer(0, width, height, stride, wl_shm::Format::Argb8888);

        // Attach the buffer to the surface and mark the entire surface as damaged
        self.surface.attach(Some(&buffer), 0, 0);
        self.surface
            .damage_buffer(0, 0, width as i32, height as i32);

        // Finally, commit the surface
        self.surface.commit();
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        self.layer_surface.destroy();
        self.surface.destroy();
    }
}

impl ClickTarget {
    fn process_click(&self, click_position: (f64, f64)) -> Option<ClickHandler> {
        let (click_x, click_y) = click_position;
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
    let args = match config::parse(env::args()) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{}", message);

            process::exit(1);
        }
    };

    let (env, display, queue) =
        init_default_environment!(Env, fields = [layer_shell: SimpleGlobal::new(),])
            .expect("Initial roundtrip failed!");

    let surfaces = Rc::new(RefCell::new(Vec::new()));

    let layer_shell = env.require_global::<zwlr_layer_shell_v1::ZwlrLayerShellV1>();

    let env_handle = env.clone();
    let surfaces_handle = Rc::clone(&surfaces);
    let output_handler = move |output: wl_output::WlOutput, info: &OutputInfo| {
        if info.obsolete {
            // an output has been removed, release it
            surfaces_handle.borrow_mut().retain(|(i, _)| *i != info.id);
            output.release();
        } else {
            // an output has been created, construct a surface for it
            let surface = env_handle.create_surface().detach();
            let pools = env_handle
                .create_double_pool(|_| {})
                .expect("Failed to create a memory pool!");
            (*surfaces_handle.borrow_mut()).push((
                info.id,
                Surface::new(args.clone(), &output, surface, &layer_shell.clone(), pools),
            ));
        }
    };

    for seat in env.get_all_seats() {
        if let Some(has_ptr) = seat::with_seat_data(&seat, |seat_data| {
            !seat_data.defunct && seat_data.has_pointer
        }) {
            if has_ptr {
                let touch = seat.get_pointer();
                let surfaces_handle = surfaces.clone();
                touch.quick_assign(move |_, event, _| {
                    for surface in (*surfaces_handle).borrow_mut().iter_mut() {
                        // We should be filtering this down so we only pass
                        // the event on to the appropriate surface. TODO
                        surface.1.handle_pointer_event(&event);
                    }
                });
            }
        }

        if let Some(has_ptr) =
            seat::with_seat_data(&seat, |seat_data| !seat_data.defunct && seat_data.has_touch)
        {
            if has_ptr {
                let touch = seat.get_touch();
                let surfaces_handle = surfaces.clone();
                touch.quick_assign(move |_, event, _| {
                    for surface in (*surfaces_handle).borrow_mut().iter_mut() {
                        // We should be filtering this down so we only pass
                        // the event on to the appropriate surface. TODO
                        surface.1.handle_touch_event(&event);
                    }
                });
            }
        }
    }

    // Process currently existing outputs
    for output in env.get_all_outputs() {
        if let Some(info) = with_output_info(&output, Clone::clone) {
            output_handler(output, &info);
        }
    }

    // Setup a listener for changes
    // The listener will live for as long as we keep this handle alive
    let _listner_handle =
        env.listen_for_outputs(move |output, info, _| output_handler(output, info));

    let mut event_loop = calloop::EventLoop::<()>::new().unwrap();

    WaylandSource::new(queue)
        .quick_insert(event_loop.handle())
        .unwrap();

    loop {
        // This is ugly, let's hope that some version of drain_filter() gets stabilized soon
        // https://github.com/rust-lang/rust/issues/43244
        {
            let mut surfaces = surfaces.borrow_mut();
            let mut i = 0;
            while i != surfaces.len() {
                if surfaces[i].1.handle_events() {
                    surfaces.remove(i);
                } else {
                    i += 1;
                }
            }
        }

        // Return early here if all surface are gone, otherwise the event loop
        // dispatch will panic with an error about not handling an event.
        if surfaces.borrow().is_empty() {
            return;
        }

        display.flush().unwrap();

        match event_loop.dispatch(None, &mut ()) {
            Ok(..) => {}
            Err(err) => {
                // err interrupted somehow happens after suspend :/
                if err.kind() != io::ErrorKind::Interrupted {
                    panic!("Unexpected dispatch event: {:?}", err);
                }
            }
        }
    }
}
