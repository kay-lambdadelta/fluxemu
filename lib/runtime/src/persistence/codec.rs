use std::{
    io::{Read, Write},
    marker::PhantomData,
};

use thiserror::Error;

use crate::persistence::{AutoSerializableComponent, SaveCodec};

#[derive(Error, Debug)]
pub enum MessagePackError {
    #[error("{0}")]
    Encode(#[from] rmp_serde::encode::Error),
    #[error("{0}")]
    Decode(#[from] rmp_serde::decode::Error),
}

#[derive(Debug)]
pub struct MessagePackCodec<C> {
    _phantom: PhantomData<C>,
}

impl<C: AutoSerializableComponent> SaveCodec for MessagePackCodec<C> {
    type Component = C;
    type Error = MessagePackError;

    fn serialize(
        &mut self,
        component: &Self::Component,
        write: &mut dyn Write,
    ) -> Result<(), Self::Error> {
        let save = component.read_save();
        rmp_serde::encode::write_named(write, &save)?;

        Ok(())
    }

    fn deserialize(
        &mut self,
        component: &mut Self::Component,
        read: &mut dyn Read,
    ) -> Result<(), Self::Error> {
        let save = rmp_serde::decode::from_read(read)?;
        component.write_save(save);

        Ok(())
    }
}
