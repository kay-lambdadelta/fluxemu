use crate::{
    ComponentPath, component::ComponentRegistry, persistence::ErasedCodec, scheduler::Period,
};
use std::collections::HashMap;

pub struct SaveManager<'a> {
    save_codecs: &'a HashMap<ComponentPath, Box<dyn ErasedCodec>>,
    registry: ComponentRegistry<'a>,
    now: Period,
}
