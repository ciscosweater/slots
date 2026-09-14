use slot::frontend::Frontend;
use slot::input::HostInput;
use slot_gfx::{Compositor, HostSurface, Surface};
use slot_power::SimPlatform;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::WindowId;

struct Slot {
    gfx: Option<(HostSurface, Compositor)>,
    frontend: Frontend,
    input: HostInput,
    asset_stage: u8,
}

impl Slot {
    fn new() -> Self {
        Slot {
            gfx: None,
            frontend: Frontend::boot(Box::new(SimPlatform::new())),
            input: HostInput::new(),
            asset_stage: 0,
        }
    }
}

impl ApplicationHandler for Slot {
    fn resumed(&mut self, events: &ActiveEventLoop) {
        if self.gfx.is_some() {
            return;
        }
        let built = HostSurface::new(events).and_then(|s| Compositor::new(&s).map(|c| (s, c)));
        let (surface, compositor) = match built {
            Ok(gfx) => gfx,
            Err(e) => {
                eprintln!("slot: {e}");
                events.exit();
                return;
            }
        };
        self.gfx = Some((surface, compositor));
    }

    fn window_event(&mut self, events: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        self.input.on_window_event(&event);
        let Some((surface, compositor)) = self.gfx.as_mut() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => events.exit(),
            WindowEvent::Resized(size) => surface.resize(size),
            WindowEvent::RedrawRequested => {
                self.frontend.render(compositor, surface.window_size());
                if let Err(e) = surface.swap() {
                    eprintln!("slot: {e}");
                    events.exit();
                    return;
                }
                match self.asset_stage {
                    0 => {
                        self.asset_stage = 1;
                    }
                    1 if self.frontend.upload_next_static_faces(compositor)
                        && self.frontend.upload_next_cart_face(compositor) =>
                    {
                        self.asset_stage = 2;
                    }
                    1 => {}
                    _ => {}
                }
                surface.request_redraw();
                self.frontend.advance(&mut self.input);
                // The simulated platform ends the process outright.
                if self.frontend.restarting() {
                    self.frontend.restart();
                }
                if self.frontend.powering_off() {
                    self.frontend.poweroff();
                    events.exit();
                }
            }
            _ => {}
        }
    }
}

pub fn run() {
    let events = match EventLoop::new() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("slot: {e}");
            return;
        }
    };
    events.set_control_flow(ControlFlow::Poll);
    if let Err(e) = events.run_app(&mut Slot::new()) {
        eprintln!("slot: {e}");
    }
}
