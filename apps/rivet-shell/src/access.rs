//! Rivet Shell's surfaces in the screen reader's tree. The panels, dock, pop-ups and
//! notifications are layer-shell surfaces drawn without winit, so eframe's AccessKit adapter
//! never sees them: each surface gets one of its own here. It costs nothing until a screen
//! reader asks; then egui builds the surface's tree every frame and the adapter hands it to
//! AT-SPI, and what the screen reader asks for (a click, focus) comes back as egui input.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use accesskit::{ActionHandler, ActionRequest, ActivationHandler, DeactivationHandler, TreeUpdate};
use calloop::channel::Sender;
use eframe::egui::{self, accesskit};

use crate::ui::PopupKind;
use crate::wl::Msg;

#[derive(Default)]
struct Shared {
    active: AtomicBool,
    actions: Mutex<Vec<ActionRequest>>,
}

/// The handlers run on AccessKit's own thread: they note what was asked and wake the shell.
struct Handler {
    shared: Arc<Shared>,
    tx: Sender<Msg>,
}

impl ActivationHandler for Handler {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        self.shared.active.store(true, Ordering::Relaxed);
        let _ = self.tx.send(Msg::Access);
        None
    }
}

impl ActionHandler for Handler {
    fn do_action(&mut self, request: ActionRequest) {
        self.shared.actions.lock().unwrap_or_else(|e| e.into_inner()).push(request);
        let _ = self.tx.send(Msg::Access);
    }
}

impl DeactivationHandler for Handler {
    fn deactivate_accessibility(&mut self) {
        self.shared.active.store(false, Ordering::Relaxed);
    }
}

pub struct SurfAccess {
    adapter: accesskit_unix::Adapter,
    shared: Arc<Shared>,
}

impl SurfAccess {
    pub fn new(tx: &Sender<Msg>) -> SurfAccess {
        let shared = Arc::new(Shared::default());
        let h = || Handler { shared: shared.clone(), tx: tx.clone() };
        let mut adapter = accesskit_unix::Adapter::new(h(), h(), h());
        adapter.update_window_focus_state(true);
        SurfAccess { adapter, shared }
    }

    /// Before a frame: build the tree while a screen reader listens, and pass on what it asked.
    pub fn before(&self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        if !self.shared.active.load(Ordering::Relaxed) {
            ctx.disable_accesskit();
            return;
        }
        ctx.enable_accesskit();
        let asked = std::mem::take(&mut *self.shared.actions.lock().unwrap_or_else(|e| e.into_inner()));
        raw.events.extend(asked.into_iter().map(egui::Event::AccessKitActionRequest));
    }

    /// After a frame: hand its tree to the screen reader, with the surface named.
    pub fn after(&mut self, out: &mut egui::PlatformOutput, name: &str) {
        let Some(mut update) = out.accesskit_update.take() else { return };
        if let Some(root) = update.tree.as_ref().map(|t| t.root)
            && let Some((_, node)) = update.nodes.iter_mut().find(|(id, _)| *id == root)
        {
            node.set_label(name);
        }
        self.adapter.update_if_active(|| update);
    }
}

/// What a screen reader calls a pop-up, in the words the shell uses for it.
pub fn popup_name(kind: &PopupKind) -> &'static str {
    match kind {
        PopupKind::Launcher { .. } => "Launcher",
        PopupKind::Calendar { .. } => "Calendar",
        PopupKind::Power { .. } => "Power",
        PopupKind::Control { .. } => "Quick settings",
        PopupKind::Hud { .. } => "Widgets",
        PopupKind::WidgetEdit => "Arrange widgets",
        PopupKind::DockMenu { .. } => "Dock menu",
        PopupKind::Downloads => "Downloads",
        PopupKind::TrashMenu { .. } => "Trash",
        PopupKind::Network { .. } => "Wi-Fi",
        PopupKind::Bluetooth => "Bluetooth",
        PopupKind::Ai => "AI usage",
        PopupKind::Weather => "Weather",
        PopupKind::Notifications => "Notifications",
        PopupKind::Sound { .. } => "Sound",
        PopupKind::Plugin { .. } => "Plugin menu",
        PopupKind::TrayMenu { .. } => "Tray menu",
        PopupKind::Media => "Now playing",
        PopupKind::Auth { .. } => "Authentication",
        PopupKind::Clipboard { .. } => "Clipboard history",
    }
}
