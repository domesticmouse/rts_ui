//! Runnable WGPU + Winit showcase for the `rts_ui` framework.

use std::sync::Arc;
use std::time::Instant;

use rts_ui::{
    layout_tree, Application, KeyCode, MouseButton, RtsDemoApp, RtsMessage, Size, UiRenderer,
    UiRuntime, Vec2,
};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton as WinitMouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::{Window, WindowId};

struct GpuState {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    renderer: UiRenderer,
}

struct AppHost {
    window: Option<Arc<Window>>,
    gpu: Option<GpuState>,
    app: RtsDemoApp,
    runtime: UiRuntime,
    last_frame: Instant,
    cached_layout: Option<rts_ui::LayoutNode<RtsMessage>>,
}

impl Default for AppHost {
    fn default() -> Self {
        Self {
            window: None,
            gpu: None,
            app: RtsDemoApp::new(),
            runtime: UiRuntime::new(),
            last_frame: Instant::now(),
            cached_layout: None,
        }
    }
}

impl AppHost {
    fn logical_viewport(&self) -> Size {
        if let Some(win) = &self.window {
            let scale = win.scale_factor();
            let phys = win.inner_size();
            Size::new(
                (phys.width as f64 / scale).max(1.0) as f32,
                (phys.height as f64 / scale).max(1.0) as f32,
            )
        } else {
            Size::new(1280.0, 800.0)
        }
    }

    fn rebuild_layout(&mut self) {
        let viewport = self.logical_viewport();
        if let Some(gpu) = &mut self.gpu {
            self.app.hud_wants_pointer = self.runtime.ui_wants_pointer();
            let tree = self.app.view();
            self.cached_layout = Some(layout_tree(&tree, viewport, &mut gpu.renderer.font_system));
        }
    }

    fn render_frame(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32();
        self.last_frame = now;

        self.app.update(RtsMessage::Tick(dt));
        self.app.hud_wants_pointer = self.runtime.ui_wants_pointer();

        let viewport = self.logical_viewport();
        let Some(gpu) = &mut self.gpu else {
            return;
        };

        let tree = self.app.view();
        let layout = layout_tree(&tree, viewport, &mut gpu.renderer.font_system);
        let draw_list =
            self.runtime
                .build_draw_list(&layout, viewport, &mut gpu.renderer.font_system);
        self.cached_layout = Some(layout);

        let frame = match gpu.surface.get_current_texture() {
            Ok(f) => f,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                if gpu.config.width > 0 && gpu.config.height > 0 {
                    gpu.surface.configure(&gpu.device, &gpu.config);
                }
                return;
            }
            Err(wgpu::SurfaceError::Timeout) => return,
            Err(e) => {
                log::warn!("Surface acquisition error: {e:?}");
                return;
            }
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("rts_ui_encoder"),
            });

        gpu.renderer
            .render(&gpu.device, &gpu.queue, &mut encoder, &view, viewport, &draw_list);

        gpu.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
    }
}

impl ApplicationHandler for AppHost {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attrs = Window::default_attributes()
            .with_title("RTS UI Framework (WGPU + Flutter Layout + Elm Architecture)")
            .with_inner_size(LogicalSize::new(1280.0, 800.0));
        let window = Arc::new(
            event_loop
                .create_window(attrs)
                .expect("failed to create window"),
        );

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance
            .create_surface(window.clone())
            .expect("failed to create wgpu surface");

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .expect("no compatible GPU adapter found");

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("rts_ui_device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            experimental_features: wgpu::ExperimentalFeatures::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
        }))
        .expect("failed to create wgpu device");

        let size = window.inner_size();
        let config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .expect("surface unsupported by adapter");
        surface.configure(&device, &config);

        let renderer = UiRenderer::new(&device, config.format);

        self.window = Some(window);
        self.gpu = Some(GpuState {
            surface,
            device,
            queue,
            config,
            renderer,
        });
        self.last_frame = Instant::now();
        self.rebuild_layout();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                if let Some(gpu) = &mut self.gpu {
                    if new_size.width > 0 && new_size.height > 0 {
                        gpu.config.width = new_size.width;
                        gpu.config.height = new_size.height;
                        gpu.surface.configure(&gpu.device, &gpu.config);
                    }
                }
                self.rebuild_layout();
            }
            WindowEvent::ModifiersChanged(mods) => {
                self.runtime.set_shift_held(mods.state().shift_key());
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(win) = &self.window {
                    let scale = win.scale_factor();
                    let logical = position.to_logical::<f32>(scale);
                    if self.cached_layout.is_none() {
                        self.rebuild_layout();
                    }
                    if let Some(layout) = &self.cached_layout {
                        self.runtime
                            .handle_cursor_moved(Vec2::new(logical.x, logical.y), layout);
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let btn = match button {
                    WinitMouseButton::Left => Some(MouseButton::Left),
                    WinitMouseButton::Right => Some(MouseButton::Right),
                    WinitMouseButton::Middle => Some(MouseButton::Middle),
                    _ => None,
                };
                if let Some(mb) = btn {
                    if self.cached_layout.is_none() {
                        self.rebuild_layout();
                    }
                    let pressed = state == ElementState::Pressed;
                    let msgs = if let Some(layout) = &self.cached_layout {
                        self.runtime.handle_mouse_button(mb, pressed, layout)
                    } else {
                        Vec::new()
                    };
                    for m in msgs {
                        self.app.update(m);
                    }
                    self.rebuild_layout();
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed && !event.repeat {
                    if let PhysicalKey::Code(wk) = event.physical_key {
                        if let Some(key) = KeyCode::from_winit(wk) {
                            if self.cached_layout.is_none() {
                                self.rebuild_layout();
                            }
                            let msgs = if let Some(layout) = &self.cached_layout {
                                self.runtime.handle_key_press(key, layout)
                            } else {
                                Vec::new()
                            };
                            for m in msgs {
                                self.app.update(m);
                            }
                            self.rebuild_layout();
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                self.render_frame();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(win) = &self.window {
            win.request_redraw();
        }
    }
}

fn main() {
    env_logger::init();
    let event_loop = EventLoop::new().expect("failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut host = AppHost::default();
    event_loop.run_app(&mut host).expect("event loop failed");
}
