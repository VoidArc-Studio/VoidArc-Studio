use smithay::{
    backend::{
        input::{InputEvent, KeyboardKeyEvent, PointerButtonEvent, ButtonState},
        renderer::{
            utils::on_commit_buffer_handler,
            gles2::Gles2Renderer,
            ImportAll, Frame, Renderer,
        },
        winit::{WinitEvent, WinitGraphicsBackend},
    },
    desktop::{space::Space, Window, WindowSurfaceType},
    reexports::{
        calloop::{EventLoop, LoopHandle, RegistrationToken},
        wayland_server::{Display, DisplayHandle},
    },
    utils::{Logical, Point, Rectangle, Transform},
    wayland::{
        compositor::{CompositorState, CompositorClientState},
        output::Output,
        shell::xdg::{XdgShellState, XdgToplevelSurfaceData},
        seat::{Seat, SeatState, KeyboardHandle, PointerHandle, XkbConfig},
        data_device::DataDeviceState,
        xwayland::{XWayland, XWaylandEvent},
    },
};
use std::process::{Command, Child};
use xkbcommon::xkb;
use toml::Value;
use std::fs;
use std::path::Path;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use chrono::Local;

struct BlueEnvironment {
    display: DisplayHandle,
    compositor_state: CompositorState,
    xdg_shell_state: XdgShellState,
    seat_state: SeatState,
    data_device_state: DataDeviceState,
    space: Space<Window>,
    keyboard: Option<KeyboardHandle>,
    pointer: Option<PointerHandle>,
    config: Value,
    outputs: Vec<Output>,
    background_texture: Option<smithay::backend::renderer::Texture>,
    xwayland: Option<XWayland>,
    running_apps: HashMap<String, Child>,
    notifications: Vec<String>,
    brightness: f32,
    volume: f32,
    wifi_enabled: bool,
    bluetooth_enabled: bool,
    battery_status: String,
    time: String,
}

impl BlueEnvironment {
    fn new(display: DisplayHandle, event_loop: &LoopHandle<Self>) -> Self {
        let compositor_state = CompositorState::new::<Self, CompositorClientState>(&display, None);
        let xdg_shell_state = XdgShellState::new::<Self>(&display);
        let mut seat_state = SeatState::new();
        let data_device_state = DataDeviceState::new::<Self>(&display);
        let seat = seat_state.new_wl_seat(&display, "blue_seat");
        let keyboard = seat_state.add_keyboard(&seat, XkbConfig::default()).ok();
        let pointer = seat_state.add_pointer(&seat).ok();

        // Load configuration
        let config_str = fs::read_to_string("/etc/blue-environment/config.toml")
        .unwrap_or_else(|_| include_str!("../config/config.toml").to_string());
        let config = config_str.parse::<Value>().expect("Invalid config format");

        // Initialize XWayland
        let xwayland = XWayland::new(&display, event_loop.handle()).ok();

        // Initialize system state
        let battery_status = Self::get_battery_status();
        let time = Self::get_current_time();

        BlueEnvironment {
            display,
            compositor_state,
            xdg_shell_state,
            seat_state,
            data_device_state,
            space: Space::new(None),
            keyboard,
            pointer,
            config,
            outputs: Vec::new(),
            background_texture: None,
            xwayland,
            running_apps: HashMap::new(),
            notifications: Vec::new(),
            brightness: 0.5,
            volume: 0.5,
            wifi_enabled: Self::get_wifi_status(),
            bluetooth_enabled: Self::get_bluetooth_status(),
            battery_status,
            time,
        }
    }

    fn launch_app(&mut self, app: &str) {
        let app_path = self.config["apps"].get(app).and_then(|v| v.as_str()).unwrap_or(app);
        match Command::new(app_path).spawn() {
            Ok(child) => {
                self.running_apps.insert(app.to_string(), child);
                self.notifications.push(format!("Launched {}", app));
            }
            Err(e) => {
                self.notifications.push(format!("Failed to launch {}: {}", app, e));
            }
        }
    }

    fn handle_input(&mut self, event: InputEvent<WinitGraphicsBackend>) {
        match event {
            InputEvent::Keyboard { event } => {
                if let Some(keyboard) = &self.keyboard {
                    let keycode = event.key_code();
                    let state = event.state();
                    let modifiers = keyboard.modifier_state();

                    // Super+Esc to return to desktop
                    if keycode == xkb::KEY_Escape as u32 && modifiers.logo && state == smithay::input::keyboard::KeyState::Pressed {
                        self.space.windows().for_each(|window| {
                            window.toplevel().configure(&self.display, WindowSurfaceType::NONE, None);
                        });
                    }
                    // Super+B for Brave
                    if keycode == xkb::KEY_B as u32 && modifiers.logo && state == smithay::input::keyboard::KeyState::Pressed {
                        self.launch_app("browser");
                    }
                    // Super+G for Game Launcher
                    if keycode == xkb::KEY_G as u32 && modifiers.logo && state == smithay::input::keyboard::KeyState::Pressed {
                        self.launch_app("game_launcher");
                    }
                    // Super+T for Terminal
                    if keycode == xkb::KEY_T as u32 && modifiers.logo && state == smithay::input::keyboard::KeyState::Pressed {
                        self.launch_app("terminal");
                    }
                    // Super+S for Software Center
                    if keycode == xkb::KEY_S as u32 && modifiers.logo && state == smithay::input::keyboard::KeyState::Pressed {
                        self.launch_app("software_center");
                    }
                    // Super+W for Wi-Fi toggle
                    if keycode == xkb::KEY_W as u32 && modifiers.logo && state == smithay::input::keyboard::KeyState::Pressed {
                        self.toggle_wifi();
                    }
                    // Super+L for Bluetooth toggle
                    if keycode == xkb::KEY_L as u32 && modifiers.logo && state == smithay::input::keyboard::KeyState::Pressed {
                        self.toggle_bluetooth();
                    }
                    // Super+V for volume up
                    if keycode == xkb::KEY_V as u32 && modifiers.logo && state == smithay::input::keyboard::KeyState::Pressed {
                        self.adjust_volume(0.1);
                    }
                    // Super+Shift+V for volume down
                    if keycode == xkb::KEY_V as u32 && modifiers.logo && modifiers.shift && state == smithay::input::keyboard::KeyState::Pressed {
                        self.adjust_volume(-0.1);
                    }
                    // Super+K for KDE Wallet
                    if keycode == xkb::KEY_K as u32 && modifiers.logo && state == smithay::input::keyboard::KeyState::Pressed {
                        self.launch_app("kwalletmanager5");
                    }
                }
            }
            InputEvent::PointerButton { event } => {
                if let Some(pointer) = &self.pointer {
                    if event.button() == smithay::input::pointer::Button::Left && event.state() == ButtonState::Pressed {
                        let pos = pointer.current_location();
                        if let Some(window) = self.space.window_under(pos).cloned() {
                            self.space.raise_window(&window, true);
                            self.toggle_fullscreen(&window);
                        } else {
                            self.launch_app("blue-launcher");
                        }
                    }
                }
            }
            _ => (),
        }
    }

    fn toggle_fullscreen(&mut self, window: &Window) {
        if let Some(toplevel) = window.toplevel() {
            let current_state = toplevel.current_state();
            if current_state.states.contains(smithay::wayland::shell::xdg::ToplevelState::Fullscreen) {
                toplevel.configure(&self.display, WindowSurfaceType::TOPLEVEL, None);
            } else {
                toplevel.configure(&self.display, WindowSurfaceType::FULLSCREEN, None);
            }
        }
    }

    fn add_output(&mut self, output: Output) {
        self.space.map_output(&output, Point::from((0, 0)), 1.0, None);
        self.outputs.push(output);
    }

    fn load_background(&mut self, renderer: &mut Gles2Renderer, path: &str) {
        if let Ok(image) = image::open(path) {
            let texture = renderer.import_texture(image.as_rgba8().unwrap().as_raw().as_slice()).ok();
            self.background_texture = texture;
            self.notifications.push("Background loaded".to_string());
        } else {
            self.notifications.push(format!("Failed to load background: {}", path));
        }
    }

    fn render_background(&self, renderer: &mut Gles2Renderer, frame: &mut Frame) {
        if let Some(texture) = &self.background_texture {
            let size = self.outputs[0].current_mode().unwrap().size;
            let rect = Rectangle::from_loc_and_size(Point::from((0, 0)), size);
            renderer.render_texture(texture, rect, 1.0, Some(Transform::Normal)).unwrap();
        } else {
            // Fallback gradient
            let size = self.outputs[0].current_mode().unwrap().size;
            let rect = Rectangle::from_loc_and_size(Point::from((0, 0)), size);
            renderer.clear(frame, [0.1, 0.1, 0.2, 1.0], rect).unwrap();
        }
    }

    fn adjust_brightness(&mut self, delta: f32) {
        self.brightness = (self.brightness + delta).clamp(0.0, 1.0);
        let brightness_percent = (self.brightness * 100.0) as u32;
        Command::new("brightnessctl")
        .arg("set")
        .arg(format!("{}%", brightness_percent))
        .spawn()
        .ok();
        self.notifications.push(format!("Brightness set to {}%", brightness_percent));
    }

    fn adjust_volume(&mut self, delta: f32) {
        self.volume = (self.volume + delta).clamp(0.0, 1.0);
        let volume_percent = (self.volume * 100.0) as u32;
        Command::new("wpctl")
        .args(["set-volume", "@DEFAULT_SINK@", &format!("{}%", volume_percent)])
        .spawn()
        .ok();
        self.notifications.push(format!("Volume set to {}%", volume_percent));
    }

    fn toggle_wifi(&mut self) {
        self.wifi_enabled = !self.wifi_enabled;
        let status = if self.wifi_enabled { "on" } else { "off" };
        Command::new("nmcli")
        .args(["radio", "wifi", status])
        .spawn()
        .ok();
        self.notifications.push(format!("Wi-Fi turned {}", status));
    }

    fn toggle_bluetooth(&mut self) {
        self.bluetooth_enabled = !self.bluetooth_enabled;
        let status = if self.bluetooth_enabled { "on" } else { "off" };
        Command::new("bluetoothctl")
        .args(["power", status])
        .spawn()
        .ok();
        self.notifications.push(format!("Bluetooth turned {}", status));
    }

    fn get_battery_status() -> String {
        Command::new("upower")
        .args(["-i", "/org/freedesktop/UPower/devices/battery_BAT0"])
        .output()
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| line.contains("state:") || line.contains("percentage:"))
            .collect::<Vec<_>>()
            .join(", ")
        })
        .unwrap_or("Battery not detected".to_string())
    }

    fn get_current_time() -> String {
        let datetime = Local::now();
        datetime.format("%Y-%m-%d %H:%M:%S").to_string()
    }

    fn get_wifi_status() -> bool {
        Command::new("nmcli")
        .args(["radio", "wifi"])
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).contains("enabled"))
        .unwrap_or(false)
    }

    fn get_bluetooth_status() -> bool {
        Command::new("bluetoothctl")
        .args(["show"])
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).contains("Powered: yes"))
        .unwrap_or(false)
    }

    fn set_timezone(&mut self, timezone: &str) {
        Command::new("timedatectl")
        .args(["set-timezone", timezone])
        .spawn()
        .ok();
        self.time = Self::get_current_time();
        self.notifications.push(format!("Timezone set to {}", timezone));
    }

    fn read_clipboard(&mut self) -> String {
        Command::new("wl-paste")
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).to_string())
        .unwrap_or_else(|_| "Failed to read clipboard".to_string())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize event loop and display
    let mut event_loop = EventLoop::try_new()?;
    let mut display = Display::new()?;
    let mut state = BlueEnvironment::new(display.handle(), &event_loop.handle());

    // Initialize Winit backend
    let (mut backend, mut winit) = smithay::backend::winit::init::<WinitGraphicsBackend<_>>()?;

    // Create output
    let output = Output::new(
        "winit".to_string(),
                             Rectangle::from_loc_and_size(Point::from((0, 0)), (1920, 1080)),
                             None,
    );
    state.add_output(output);

    // Load background
    if let Some(bg_path) = state.config["appearance"]["background"].as_str() {
        state.load_background(backend.renderer(), bg_path);
    }

    // Power management (set power-saver profile)
    Command::new("powerprofilesctl")
    .arg("set")
    .arg("power-saver")
    .spawn()
    .ok();

    // Start notification daemon
    Command::new("mako")
    .spawn()
    .ok();

    // Start XWayland
    if let Some(xwayland) = state.xwayland.as_mut() {
        xwayland.start(&state.display, &event_loop.handle()).ok();
    }

    // Event loop
    loop {
        // Update time
        state.time = BlueEnvironment::get_current_time();

        // Handle XWayland events
        if let Some(xwayland) = state.xwayland.as_mut() {
            xwayland.handle_events(&mut state.space, &state.display).ok();
        }

        // Handle input events
        winit.dispatch_new_events(|event| match event {
            WinitEvent::Input(input) => state.handle_input(input),
                                  _ => (),
        })?;

        // Clean up finished processes
        state.running_apps.retain(|_, child| child.try_wait().unwrap_or(None).is_none());

        // Render the scene
        let mut renderer = backend.renderer();
        renderer.with_context(|renderer, frame| {
            state.render_background(renderer, frame);
            state.space.render(renderer, frame, None).unwrap();
        })?;
        state.space.commit();
        state.display.flush_clients()?;
    }
}
