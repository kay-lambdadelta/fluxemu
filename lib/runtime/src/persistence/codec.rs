use crate::{component::Component, persistence::AutoSerializableComponent};
use std::{
    any::Any,
    fmt::Debug,
    io::{Read, Write},
    marker::PhantomData,
};
use thiserror::Error;

pub trait Codec: Send + Sync + Debug + 'static {
    type Component: Component;
    type Error: std::error::Error;

    fn serialize(
        &self,
        component: &Self::Component,
        write: &mut dyn Write,
    ) -> Result<(), Self::Error>;

    fn deserialize(
        &self,
        component: &mut Self::Component,
        read: &mut dyn Read,
    ) -> Result<(), Self::Error>;
}

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

impl<C: AutoSerializableComponent> Codec for MessagePackCodec<C> {
    type Component = C;
    type Error = MessagePackError;

    fn serialize(
        &self,
        component: &Self::Component,
        write: &mut dyn Write,
    ) -> Result<(), Self::Error> {
        let save = component.read_save();
        rmp_serde::encode::write_named(write, &save)?;

        Ok(())
    }

    fn deserialize(
        &self,
        component: &mut Self::Component,
        read: &mut dyn Read,
    ) -> Result<(), Self::Error> {
        let save = rmp_serde::decode::from_read(read)?;
        component.write_save(save);

        Ok(())
    }
}

pub(crate) trait ErasedCodec: Send + Sync + Debug + 'static {
    fn serialize(
        &self,
        component: &dyn Component,
        write: &mut dyn Write,
    ) -> Result<(), Box<dyn std::error::Error>>;

    fn deserialize(
        &self,
        component: &mut dyn Component,
        read: &mut dyn Read,
    ) -> Result<(), Box<dyn std::error::Error>>;
}

#[derive(Debug)]
pub(crate) struct ErasedCodecWrapper<C: Codec>(C);
impl<C: Codec> ErasedCodec for ErasedCodecWrapper<C> {
    fn serialize(
        &self,
        component: &dyn Component,
        write: &mut dyn Write,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let component = (component as &dyn Any).downcast_ref().unwrap();

        self.0.serialize(component, write)?;

        Ok(())
    }

    fn deserialize(
        &self,
        component: &mut dyn Component,
        read: &mut dyn Read,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let component = (component as &mut dyn Any).downcast_mut().unwrap();

        self.0.deserialize(component, read)?;

        Ok(())
    }
}
