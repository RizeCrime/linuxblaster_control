//! System tray icon (StatusNotifierItem, shown by KDE Plasma, most other
//! desktops, and GNOME with the AppIndicator extension).
//!
//! Hiding works by closing the window and opening a new one when it's shown
//! again: Wayland doesn't let a client hide its own window, but eframe can
//! run its event loop again after the window was closed. While no window
//! is open, the tray's commands are handled here on the main thread, so the
//! device is never accessed from two threads at once.

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui::{self, Vec2, ViewportCommand};
use ksni::blocking::{Handle, TrayMethods};
use ksni::menu::{CheckmarkItem, StandardItem};
use ksni::{MenuItem, OfflineReason};
use linuxblaster_control::{BlasterXG6, FeatureId};
use tracing::{debug, error, info, warn};

use crate::app::Launcher;

const TITLE: &str = "Linuxblaster Control";

/// How often device events are processed while the window is hidden,
/// so the tray's lighting checkmark follows the hardware.
const HIDDEN_POLL_INTERVAL: Duration = Duration::from_millis(500);
/// How long the window may stay hidden while no tray is there to show it
/// again, e.g. on login before the panel has started.
const TRAY_GRACE_PERIOD: Duration = Duration::from_secs(10);

/// Command line flags for the tray.
#[derive(Debug, Clone, Copy, Default)]
pub struct TrayOptions {
    /// `--tray` / `--minimized`: start hidden in the tray.
    pub start_hidden: bool,
    /// `--no-tray`: no tray icon, closing the window quits.
    pub disabled: bool,
    /// `--minimize-to-tray`: minimizing hides the window in the tray.
    pub minimize_to_tray: bool,
}

impl TrayOptions {
    /// Other arguments are left alone, so they can be used elsewhere.
    pub fn from_args() -> Self {
        let mut options = Self::default();
        for arg in std::env::args().skip(1) {
            match arg.as_str() {
                "--tray" | "--minimized" => options.start_hidden = true,
                "--no-tray" => options.disabled = true,
                "--minimize-to-tray" => options.minimize_to_tray = true,
                _ => {}
            }
        }
        options
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrayCommand {
    Show,
    Hide,
    ToggleLighting,
    Quit,
}

/// The tray icon itself, run by ksni on its own thread.
/// It only sends commands, the device is handled on the main thread.
struct G6Tray {
    commands: Sender<TrayCommand>,
    /// Context of the open window, to wake it up for a command.
    window: Arc<Mutex<Option<egui::Context>>>,
    /// Whether a tray (StatusNotifierWatcher) is there to show the icon.
    online: Arc<AtomicBool>,
    window_open: bool,
    /// `None` while the G6 isn't connected.
    lighting: Option<bool>,
    icon: Vec<ksni::Icon>,
}

impl G6Tray {
    fn send(&self, command: TrayCommand) {
        debug!("Tray: {:?}", command);
        let _ = self.commands.send(command);
        if let Some(ctx) = &*self.window.lock().unwrap() {
            ctx.request_repaint();
        }
    }

    fn toggle_window(&self) {
        self.send(if self.window_open {
            TrayCommand::Hide
        } else {
            TrayCommand::Show
        });
    }
}

impl ksni::Tray for G6Tray {
    fn id(&self) -> String {
        "linuxblaster-control".into()
    }

    fn title(&self) -> String {
        TITLE.into()
    }

    fn category(&self) -> ksni::Category {
        ksni::Category::Hardware
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        self.icon.clone()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: TITLE.into(),
            description: match self.lighting {
                Some(_) => "Sound BlasterX G6".into(),
                None => "G6 not connected".into(),
            },
            ..Default::default()
        }
    }

    // left click
    fn activate(&mut self, _x: i32, _y: i32) {
        self.toggle_window();
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            StandardItem {
                label: if self.window_open {
                    "Hide Window".into()
                } else {
                    "Show Window".into()
                },
                activate: Box::new(|tray: &mut Self| tray.toggle_window()),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: "Lighting".into(),
                enabled: self.lighting.is_some(),
                checked: self.lighting.unwrap_or(false),
                activate: Box::new(|tray: &mut Self| {
                    tray.send(TrayCommand::ToggleLighting)
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit".into(),
                icon_name: "application-exit".into(),
                activate: Box::new(|tray: &mut Self| {
                    tray.send(TrayCommand::Quit)
                }),
                ..Default::default()
            }
            .into(),
        ]
    }

    fn watcher_online(&self) {
        info!("System tray available");
        self.online.store(true, Ordering::Relaxed);
    }

    fn watcher_offline(&self, reason: OfflineReason) -> bool {
        warn!("System tray not available: {:?}", reason);
        self.online.store(false, Ordering::Relaxed);
        // keep running, the tray may (re)appear later
        true
    }
}

/// The main thread's side of the tray.
struct TrayLink {
    handle: Handle<G6Tray>,
    commands: Receiver<TrayCommand>,
    window: Arc<Mutex<Option<egui::Context>>>,
    online: Arc<AtomicBool>,
    /// Last lighting state sent to the tray.
    lighting: Cell<Option<bool>>,
}

impl TrayLink {
    fn spawn() -> Option<Self> {
        let (sender, commands) = mpsc::channel();
        let window = Arc::new(Mutex::new(None));
        let online = Arc::new(AtomicBool::new(true));

        let tray = G6Tray {
            commands: sender,
            window: window.clone(),
            online: online.clone(),
            window_open: false,
            lighting: None,
            icon: tray_icon(),
        };

        // A missing tray only marks it offline instead of failing,
        // so starting before the panel (autostart) still works.
        match tray.assume_sni_available(true).spawn() {
            Ok(handle) => Some(Self {
                handle,
                commands,
                window,
                online,
                lighting: Cell::new(None),
            }),
            Err(error) => {
                warn!("Couldn't create the tray icon: {}", error);
                None
            }
        }
    }

    fn online(&self) -> bool {
        self.online.load(Ordering::Relaxed)
    }

    fn set_window(&self, ctx: Option<egui::Context>) {
        let open = ctx.is_some();
        *self.window.lock().unwrap() = ctx;
        self.handle.update(|tray| tray.window_open = open);
    }

    /// Updates the lighting checkmark, if it changed.
    fn sync_lighting(&self, device: Option<&BlasterXG6>) {
        let lighting = device
            .map(|device| device.feature(FeatureId::LightingToggle))
            .filter(|feature| !feature.value().is_nan())
            .map(|feature| feature.as_bool());

        if self.lighting.replace(lighting) != lighting {
            self.handle.update(|tray| tray.lighting = lighting);
        }
    }
}

/// The bundled app icon in the tray's ARGB format.
fn tray_icon() -> Vec<ksni::Icon> {
    let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!(
        "../packaging/linuxblaster-control.png"
    )) else {
        error!("Failed to decode the tray icon");
        return Vec::new();
    };

    let data = icon
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|pixel| [pixel[3], pixel[0], pixel[1], pixel[2]])
        .collect();

    vec![ksni::Icon {
        width: icon.width as i32,
        height: icon.height as i32,
        data,
    }]
}

fn toggle_lighting(launcher: &mut Launcher) {
    let Some(device) = launcher.device() else {
        warn!("Can't toggle the lighting, the G6 isn't connected");
        return;
    };
    if let Err(error) = device.set_feature(FeatureId::LightingToggle, None) {
        error!("Failed to toggle the lighting: {}", error);
    }
}

/// Wraps the app while the window is open, handling the tray's commands.
struct TrayWindow<'a> {
    launcher: &'a mut Launcher,
    tray: &'a TrayLink,
    minimize_to_tray: bool,
    quit: &'a Cell<bool>,
    size: &'a Cell<Option<Vec2>>,
}

impl eframe::App for TrayWindow<'_> {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        while let Ok(command) = self.tray.commands.try_recv() {
            match command {
                TrayCommand::Show => {
                    ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(ViewportCommand::Focus);
                }
                TrayCommand::Hide => {
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
                TrayCommand::ToggleLighting => toggle_lighting(self.launcher),
                TrayCommand::Quit => {
                    self.quit.set(true);
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
            }
        }

        // only X11 reports this, Wayland doesn't tell windows they're minimized
        let minimized = ctx.input(|i| i.viewport().minimized);
        if self.minimize_to_tray
            && minimized == Some(true)
            && self.tray.online()
        {
            debug!("Minimized, hiding in the tray");
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }

        if let Some(rect) = ctx.input(|i| i.viewport().inner_rect) {
            self.size.set(Some(rect.size()));
        }

        self.tray.sync_lighting(self.launcher.device());

        eframe::App::update(self.launcher, ctx, frame);
    }
}

/// Runs the app with a tray icon, replacing `eframe::run_native`.
///
/// `options` is called for every window that's opened,
/// `setup` for every new egui context.
pub fn run(
    mut launcher: Launcher,
    options: impl Fn() -> eframe::NativeOptions,
    setup: impl Fn(&eframe::CreationContext),
) -> eframe::Result<()> {
    let tray_options = TrayOptions::from_args();
    debug!("{:?}", tray_options);

    let tray = if tray_options.disabled {
        None
    } else {
        TrayLink::spawn()
    };

    let Some(tray) = tray else {
        if tray_options.start_hidden {
            warn!("No tray icon, ignoring --tray and showing the window");
        }
        return eframe::run_native(
            TITLE,
            options(),
            Box::new(|cc| {
                setup(cc);
                Ok(Box::new(launcher))
            }),
        );
    };

    let quit = Cell::new(false);
    let size = Cell::new(None);
    let mut show = !tray_options.start_hidden;

    loop {
        if show {
            let mut options = options();
            if let Some(size) = size.get() {
                // keep the size from the last time the window was open
                options.viewport = options.viewport.with_inner_size(size);
            }

            let window = TrayWindow {
                launcher: &mut launcher,
                tray: &tray,
                minimize_to_tray: tray_options.minimize_to_tray,
                quit: &quit,
                size: &size,
            };
            eframe::run_native(
                TITLE,
                options,
                Box::new(|cc| {
                    setup(cc);
                    tray.set_window(Some(cc.egui_ctx.clone()));
                    Ok(Box::new(window))
                }),
            )?;
            tray.set_window(None);

            if quit.get() {
                break;
            }
            if !tray.online() {
                info!("Window closed and no tray to show it again, quitting");
                break;
            }
        }

        info!("Window hidden in the tray");
        if !wait_hidden(&mut launcher, &tray) {
            break;
        }
        show = true;
    }

    info!("Quitting");
    Ok(())
}

/// Handles the tray while no window is open.
///
/// Returns `true` to show the window again, `false` to quit.
fn wait_hidden(launcher: &mut Launcher, tray: &TrayLink) -> bool {
    let hidden_since = Instant::now();

    loop {
        if let Some(device) = launcher.device() {
            device.process_device_events();
        }
        tray.sync_lighting(launcher.device());

        match tray.commands.recv_timeout(HIDDEN_POLL_INTERVAL) {
            Ok(TrayCommand::Show) => return true,
            Ok(TrayCommand::Quit) => return false,
            Ok(TrayCommand::Hide) => {}
            Ok(TrayCommand::ToggleLighting) => toggle_lighting(launcher),
            Err(RecvTimeoutError::Timeout) => {
                if !tray.online() && hidden_since.elapsed() >= TRAY_GRACE_PERIOD
                {
                    warn!(
                        "No system tray to show the icon, showing the window"
                    );
                    return true;
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                warn!("The tray icon stopped, showing the window");
                return true;
            }
        }
    }
}
