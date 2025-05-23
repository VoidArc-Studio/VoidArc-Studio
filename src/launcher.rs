use egui::{CentralPanel, Context, Ui, Vec2, Color32, RichText, ImageButton, Sense, Rounding, TextureHandle, epaint::Shadow};
use std::process::{Command, Child};
use toml::Value;
use std::fs;
use std::collections::HashMap;
use image::DynamicImage;
use std::time::{SystemTime, UNIX_EPOCH};
use std::sync::mpsc;
use std::thread;

struct BlueLauncher {
    config: Value,
    textures: HashMap<String, TextureHandle>,
    running_apps: HashMap<String, Child>,
    distro: String,
    brightness: f32,
    volume: f32,
    wifi_enabled: bool,
    bluetooth_enabled: bool,
    battery_status: String,
    time: String,
    clipboard_content: String,
    notifications: Vec<String>,
}

impl BlueLauncher {
    fn new(ctx: &egui::Context) -> Self {
        // Load configuration
        let config_str = fs::read_to_string("/etc/blue-environment/config.toml")
        .unwrap_or_else(|_| include_str!("../config/config.toml").to_string());
        let config = config_str.parse::<Value>().expect("Invalid config format");

        // Detect distribution
        let distro = fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|content| {
            content.lines()
            .find(|line| line.starts_with("ID="))
            .map(|line| line.strip_prefix("ID=").unwrap_or("unknown").to_string())
        })
        .unwrap_or("unknown".to_string());

        // Load textures for icons
        let mut textures = HashMap::new();
        let icon_paths = [
            ("browser", "browser_icon"),
            ("game_launcher", "game_launcher_icon"),
            ("software_center", "software_center_icon"),
            ("terminal", "terminal_icon"),
        ];

        for (key, config_key) in icon_paths.iter() {
            if let Some(path) = config["appearance"].get(config_key).and_then(|v| v.as_str()) {
                if let Ok(image) = image::open(path) {
                    let rgba = image.to_rgba8();
                    let size = [rgba.width() as usize, rgba.height() as usize];
                    let texture = ctx.load_texture(
                        key,
                        egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_flat_samples().as_slice()),
                                                   Default::default(),
                    );
                    textures.insert(key.to_string(), texture);
                }
            }
        }

        // Initialize system state
        let battery_status = Self::get_battery_status();
        let time = Self::get_current_time();

        BlueLauncher {
            config,
            textures,
            running_apps: HashMap::new(),
            distro,
            brightness: 0.5,
            volume: 0.5,
            wifi_enabled: Self::get_wifi_status(),
            bluetooth_enabled: Self::get_bluetooth_status(),
            battery_status,
            time,
            clipboard_content: String::new(),
            notifications: Vec::new(),
        }
    }

    fn launch_app(&mut self, app: &str) -> bool {
        if self.running_apps.contains_key(app) {
            return false; // App already running
        }

        if let Some(app_path) = self.config["apps"][app].as_str() {
            match Command::new(app_path).spawn() {
                Ok(child) => {
                    self.running_apps.insert(app.to_string(), child);
                    self.notifications.push(format!("Launched {}", app));
                    true
                }
                Err(e) => {
                    self.notifications.push(format!("Failed to launch {}: {}", app, e));
                    false
                }
            }
        } else {
            self.notifications.push(format!("No path for app {} in config", app));
            false
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
            .find(|line| line.contains("state:") || line.contains("percentage:"))
            .map(|line| line.trim().to_string())
            .unwrap_or("Unknown battery status".to_string())
        })
        .unwrap_or("Battery not detected".to_string())
    }

    fn get_current_time() -> String {
        let since_epoch = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        let datetime = chrono::DateTime::<chrono::Local>::from_timestamp(since_epoch.as_secs() as i64, 0)
        .unwrap_or(chrono::Local::now());
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

    fn read_clipboard(&mut self) {
        if let Ok(output) = Command::new("wl-paste").output() {
            self.clipboard_content = String::from_utf8_lossy(&output.stdout).to_string();
            self.notifications.push("Clipboard content updated".to_string());
        }
    }
}

impl eframe::App for BlueLauncher {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // Clean up finished processes
        self.running_apps.retain(|_, child| child.try_wait().unwrap_or(None).is_none());

        // Update time
        self.time = Self::get_current_time();

        // Apply custom styling
        let mut style = (*ctx.style()).clone();
        style.visuals.panel_fill = Color32::from_rgb(20, 20, 30);
        style.visuals.window_shadow = Shadow::big_dark();
        style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(70, 70, 80);
        style.visuals.widgets.active.bg_fill = Color32::from_rgb(90, 90, 100);
        style.visuals.widgets.noninteractive.rounding = Rounding::same(10.0);
        ctx.set_style(style);

        CentralPanel::default().show(ctx, |ui: &mut Ui| {
            ui.style_mut().spacing.item_spacing = Vec2::new(20.0, 20.0);

            // Header
            ui.heading(RichText::new("Blue Desktop Environment").size(40.0).color(Color32::LIGHT_BLUE));

            ui.add_space(30.0);

            // App buttons
            ui.horizontal_wrapped(|ui| {
                self.app_button(ui, "browser", "Browser", "🌐");
                self.app_button(ui, "game_launcher", "Games", "🎮");
                self.app_button(ui, "software_center", "Software", "🛒");
                self.app_button(ui, "terminal", "Terminal", "🖥️");
            });

            ui.add_space(20.0);

            // Settings panel
            ui.collapsing(RichText::new("⚙️ Settings").size(24.0), |ui| {
                // System Info
                ui.label(RichText::new(format!("Distribution: {}", self.distro)).size(16.0));
                ui.label(RichText::new(format!("Time: {}", self.time)).size(16.0));
                ui.label(RichText::new(format!("Battery: {}", self.battery_status)).size(16.0));

                // Brightness
                ui.horizontal(|ui| {
                    ui.label("Brightness:");
                    if ui.button("-").clicked() {
                        self.adjust_brightness(-0.1);
                    }
                    ui.add(egui::Slider::new(&mut self.brightness, 0.0..=1.0).show_value(false));
                    if ui.button("+").clicked() {
                        self.adjust_brightness(0.1);
                    }
                });

                // Volume
                ui.horizontal(|ui| {
                    ui.label("Volume:");
                    if ui.button("-").clicked() {
                        self.adjust_volume(-0.1);
                    }
                    ui.add(egui::Slider::new(&mut self.volume, 0.0..=1.0).show_value(false));
                    if ui.button("+").clicked() {
                        self.adjust_volume(0.1);
                    }
                });

                // Wi-Fi
                ui.horizontal(|ui| {
                    ui.label(format!("Wi-Fi: {}", if self.wifi_enabled { "On" } else { "Off" }));
                    if ui.button("Toggle Wi-Fi").clicked() {
                        self.toggle_wifi();
                    }
                });

                // Bluetooth
                ui.horizontal(|ui| {
                    ui.label(format!("Bluetooth: {}", if self.bluetooth_enabled { "On" } else { "Off" }));
                    if ui.button("Toggle Bluetooth").clicked() {
                        self.toggle_bluetooth();
                    }
                });

                // Clipboard
                ui.horizontal(|ui| {
                    ui.label("Clipboard:");
                    if ui.button("Read").clicked() {
                        self.read_clipboard();
                    }
                    ui.label(&self.clipboard_content);
                });

                // KDE Wallet
                if ui.button("Open KDE Wallet").clicked() {
                    Command::new("kwalletmanager5").spawn().ok();
                    self.notifications.push("Opened KDE Wallet".to_string());
                }

                // Package Manager
                if ui.button("Open Package Manager").clicked() {
                    let pkg_manager = match self.distro.as_str() {
                        "bazzite" | "fedora" => "dnf",
                        "ubuntu" => "apt",
                        "arch" => "pacman",
                        "opensuse" => "zypper",
                        _ => "unknown",
                    };
                    Command::new("wezterm")
                    .arg("start")
                    .arg(pkg_manager)
                    .spawn()
                    .ok();
                    self.notifications.push(format!("Opened {}", pkg_manager));
                }
            });

            // Notifications
            ui.collapsing(RichText::new("🔔 Notifications").size(24.0), |ui| {
                for notification in &self.notifications {
                    ui.label(RichText::new(notification).size(16.0).color(Color32::YELLOW));
                }
                if ui.button("Clear Notifications").clicked() {
                    self.notifications.clear();
                }
            });
        });

        // Request repaint for real-time updates
        ctx.request_repaint();
    }
}

impl BlueLauncher {
    fn app_button(&mut self, ui: &mut Ui, app_key: &str, label: &str, fallback_emoji: &str) {
        let is_running = self.running_apps.contains_key(app_key);
        let text = if is_running {
            RichText::new(format!("{} (Running)", label)).color(Color32::LIGHT_GREEN)
        } else {
            RichText::new(label)
        }.size(24.0);

        let button = if let Some(texture) = self.textures.get(app_key) {
            ImageButton::new(texture, Vec2::new(80.0, 80.0)).frame(true)
        } else {
            ImageButton::new(RichText::new(fallback_emoji).size(80.0)).frame(true)
        };

        if ui.add(button.sense(Sense::click().union(Sense::hover())))
            .on_hover_text(label)
            .on_hover_ui(|ui| {
                ui.label(RichText::new(label).size(16.0));
            })
            .clicked() && !is_running
            {
                self.launch_app(app_key);
            }
    }
}

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        initial_window_size: Some(egui::vec2(1000.0, 800.0)),
        centered: true,
        decorated: true,
        ..Default::default()
    };
    eframe::run_native(
        "Blue Launcher",
        options,
        Box::new(|cc| Box::new(BlueLauncher::new(&cc.egui_ctx))),
    )
}
