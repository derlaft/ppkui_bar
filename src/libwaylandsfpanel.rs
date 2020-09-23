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
    io::{self, Seek, SeekFrom, Write},
    rc::Rc,
};

default_environment!(Env,
    fields = [
        layer_shell: SimpleGlobal<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    ],
    singles = [
        zwlr_layer_shell_v1::ZwlrLayerShellV1 => layer_shell
    ],
);

#[derive(Clone)]
pub struct ApplicationSettings {
    pub namespace: String,
    pub layer: zwlr_layer_shell_v1::Layer,
    pub size: WindowSize,
    pub exclusive_zone: i32,
    pub margins: (u32, u32, u32, u32),
    pub anchor: zwlr_layer_surface_v1::Anchor,
}

#[derive(Clone, Copy, PartialEq)]
pub struct PointerPosition(pub f64, pub f64);

#[derive(Clone, Copy)]
pub struct WindowSize(pub u32, pub u32);

pub trait Application: Sized + Clone {
    fn new() -> Self;
    fn settings(&self) -> ApplicationSettings;
    fn draw(&mut self, size: WindowSize, buffer: &mut [u8]);

    fn input_start_gesture(&mut self, pos: PointerPosition) -> Option<RenderEvent>;
    fn input_stop_gesture(&mut self) -> Option<RenderEvent>;
    fn input_movement(&mut self, pos: PointerPosition) -> Option<RenderEvent>;
    fn input_commit_gesture(&mut self) -> Option<RenderEvent>;
}

#[derive(PartialEq, Copy, Clone)]
pub enum RenderEvent {
    Render,
    Configure { width: u32, height: u32 },
    Closed,
}

struct Surface<T: Application> {
    app: T,
    surface: wl_surface::WlSurface,
    layer_surface: Main<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1>,
    next_render_event: Rc<Cell<Option<RenderEvent>>>,
    pools: DoubleMemPool,
    dimensions: WindowSize,
    /// User requested exit
    should_exit: bool,
    last_pointer_location: Option<PointerPosition>,
}

impl<T: Application> Surface<T> {
    fn new(
        app: T,
        output: &wl_output::WlOutput,
        surface: wl_surface::WlSurface,
        layer_shell: &Attached<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
        pools: DoubleMemPool,
    ) -> Self {
        let settings = app.settings();

        let layer_surface = layer_shell.get_layer_surface(
            &surface,
            Some(&output),
            settings.layer,
            settings.namespace.to_owned(),
        );

        layer_surface.set_size(settings.size.0, settings.size.1);
        layer_surface.set_exclusive_zone(settings.exclusive_zone);
        layer_surface.set_anchor(settings.anchor);

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

        Self {
            app: app,
            surface,
            layer_surface,
            next_render_event,
            pools,
            dimensions: WindowSize(0, 0),
            should_exit: false,
            last_pointer_location: None,
        }
    }

    /// Handles any events that have occurred since the last call, redrawing if needed.
    /// Returns true if the surface should be dropped.
    fn handle_events(&mut self) -> bool {
        match self.next_render_event.take() {
            Some(RenderEvent::Render) => {
                self.draw();
                false
            }
            Some(RenderEvent::Closed) => true,
            Some(RenderEvent::Configure { width, height }) => {
                self.dimensions = WindowSize(width, height);
                self.draw();
                false
            }
            None => self.should_exit,
        }
    }

    fn update_event(&mut self, result: Option<RenderEvent>) {
        if let Some(..) = result {
            self.next_render_event.set(result);
        }
    }

    fn input_stop_gesture(&mut self) {
        let result = self.app.input_stop_gesture();
        self.update_event(result);
    }

    fn input_start_gesture(&mut self, pos: PointerPosition) {
        let result = self.app.input_start_gesture(pos);
        self.update_event(result);
    }

    fn input_movement(&mut self, pos: PointerPosition) {
        let result = self.app.input_movement(pos);
        self.update_event(result);
    }

    fn input_commit_gesture(&mut self) {
        let result = self.app.input_commit_gesture();
        self.update_event(result);
    }

    fn handle_touch_event(&mut self, event: &wl_touch::Event) {
        match event {
            wl_touch::Event::Cancel => self.input_stop_gesture(),
            wl_touch::Event::Down { x, y, .. } => self.input_start_gesture(PointerPosition(*x, *y)),
            wl_touch::Event::Motion { x, y, .. } => self.input_movement(PointerPosition(*x, *y)),
            wl_touch::Event::Up { .. } => self.input_commit_gesture(),
            _ => {}
        }
    }

    fn handle_pointer_event(&mut self, event: &wl_pointer::Event) {
        match event {
            wl_pointer::Event::Leave { .. } => {
                self.input_stop_gesture();
                self.last_pointer_location = None;
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
                let pos = PointerPosition(*surface_x, *surface_y);
                self.last_pointer_location = Some(pos);
                self.input_movement(pos);
            }
            wl_pointer::Event::Button {
                state: ButtonState::Pressed,
                ..
            } => self.input_start_gesture(
                self.last_pointer_location
                    // TODO: maybe there's a better way
                    // should be fine for now
                    .unwrap_or(PointerPosition(0., 0.)),
            ),
            wl_pointer::Event::Button {
                state: ButtonState::Released,
                ..
            } => self.input_commit_gesture(),
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

        // First make sure the pool is the right size
        pool.resize((stride * height) as usize).unwrap();

        let mut buf: Vec<u8> = vec![0; (4 * width * height) as usize];

        self.app.draw(self.dimensions, &mut buf);

        pool.seek(SeekFrom::Start(0)).unwrap();
        pool.write_all(buf.as_slice()).unwrap();
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

impl<T: Application> Drop for Surface<T> {
    fn drop(&mut self) {
        self.layer_surface.destroy();
        self.surface.destroy();
    }
}

pub fn run_application<A>()
where
    A: Application + 'static,
{
    let (env, display, queue) =
        init_default_environment!(Env, fields = [layer_shell: SimpleGlobal::new(),])
            .expect("Initial roundtrip failed!");

    let surfaces = Rc::new(RefCell::new(Vec::new()));

    let layer_shell = env.require_global::<zwlr_layer_shell_v1::ZwlrLayerShellV1>();

    let env_handle = env.clone();
    let surfaces_handle = Rc::clone(&surfaces);
    let template = A::new();

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
            let app = template.clone();
            (*surfaces_handle.borrow_mut()).push((
                info.id,
                Surface::new(app, &output, surface, &layer_shell.clone(), pools),
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
