use std::{
    fmt::Debug,
    io::{Read, Write},
};

use crate::component::Component;

pub trait SaveCodec: Debug {
    type Component: Component;
    type Error: std::error::Error;

    fn serialize(
        &mut self,
        component: &Self::Component,
        write: &mut dyn Write,
    ) -> Result<(), Self::Error>;

    fn deserialize(
        &mut self,
        component: &mut Self::Component,
        read: &mut dyn Read,
    ) -> Result<(), Self::Error>;
}
