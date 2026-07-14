use tokio::sync::broadcast;
use tracing::Subscriber;
use tracing_subscriber::Layer;

/// A tracing layer that broadcasts formatted log lines to WebSocket subscribers.
#[derive(Clone)]
pub struct BroadcastLayer {
    tx: broadcast::Sender<String>,
}

impl BroadcastLayer {
    pub fn new(tx: broadcast::Sender<String>) -> Self {
        Self { tx }
    }
}

impl<S: Subscriber> Layer<S> for BroadcastLayer {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        use std::fmt::Write;
        use tracing::field::Visit;

        struct Visitor {
            message: String,
            fields: String,
        }

        impl Visit for Visitor {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    let _ = write!(&mut self.message, "{:?}", value);
                } else {
                    if !self.fields.is_empty() {
                        self.fields.push(' ');
                    }
                    let _ = write!(&mut self.fields, "{}={:?}", field.name(), value);
                }
            }
        }

        let metadata = event.metadata();
        let mut visitor = Visitor {
            message: String::new(),
            fields: String::new(),
        };
        event.record(&mut visitor);

        let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ");
        let level = metadata.level().as_str();
        let target = metadata.target();
        let message = if visitor.fields.is_empty() {
            visitor.message
        } else {
            format!("{} {}", visitor.message, visitor.fields)
        };

        let line = format!("{} {} {} {}", timestamp, level, target, message);

        // Ignore send failures — no subscribers is fine
        let _ = self.tx.send(line);
    }
}
