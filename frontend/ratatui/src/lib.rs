mod event_loop;
mod external_input;
mod frontend;
mod graphics;
mod input;
mod machine_view;

pub use event_loop::run;
pub use external_input::{ExternalInputReceiver, ExternalInputSender, external_input_channel};
pub use graphics::TerminalGraphicsRuntime;

rust_i18n::i18n!("locales", fallback = "en");
