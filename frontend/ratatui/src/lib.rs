mod event_loop;
mod external_input;
mod frontend;
mod input;
mod machine_view;
mod output;

pub use event_loop::run;
pub use external_input::{ExternalInputReceiver, ExternalInputSender, external_input_channel};

rust_i18n::i18n!("locales", fallback = "en");
