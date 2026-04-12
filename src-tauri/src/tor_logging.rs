//! Custom tracing Layer that forwards Tor/Arti log events to the frontend.
//!
//! The AppHandle is set lazily after the Tauri app starts, so the layer
//! can be registered before the app is ready.

use std::sync::OnceLock;
use tauri::Emitter;
use tracing::field::{Field, Visit};
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;

/// Global AppHandle, set once during Tauri setup.
static APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();

/// Call this in .setup() to enable log forwarding.
pub fn set_app_handle(handle: tauri::AppHandle) {
    let _ = APP_HANDLE.set(handle);
}

struct MessageVisitor {
    message: Option<String>,
}

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = Some(format!("{:?}", value));
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        }
    }
}

/// Forwards tor_*/arti_* log events to the Svelte frontend via Tauri events.
pub struct TorLogForwarder;

impl<S> Layer<S> for TorLogForwarder
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let handle = match APP_HANDLE.get() {
            Some(h) => h,
            None => return, // App not ready yet
        };

        let target = event.metadata().target();

        let is_tor = target.starts_with("tor_")
            || target.starts_with("arti")
            || target.starts_with("app_lib::tor");

        if !is_tor {
            return;
        }

        let mut visitor = MessageVisitor { message: None };
        event.record(&mut visitor);

        let message = match visitor.message {
            Some(m) => m,
            None => return,
        };

        let level = event.metadata().level().to_string();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let secs = now.as_secs();
        let h = (secs / 3600) % 24;
        let m = (secs / 60) % 60;
        let s = secs % 60;
        let ms = now.subsec_millis();
        let ts = format!("{h:02}:{m:02}:{s:02}.{ms:03}");

        let entry = serde_json::json!({
            "level": level,
            "target": target,
            "message": message,
            "time": ts,
        });

        let _ = handle.emit("tor-log", &entry);
    }
}
