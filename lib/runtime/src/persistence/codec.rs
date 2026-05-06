use std::{
    any::Any,
    fmt::Debug,
    io::{Read, Write},
    marker::PhantomData,
};

use crate::{
    component::Component,
    persistence::{AutoSerializableComponent, PersistanceFormatVersion},
};

pub trait Codec: Send + Sync + Debug + 'static {
    type Component: Component;
    type SerializationError: std::error::Error;
    type DeserializationError: std::error::Error;

    const VERSION: PersistanceFormatVersion;

    fn serialize<W: Write + ?Sized>(
        &self,
        component: &Self::Component,
        write: &mut W,
    ) -> Result<(), Self::SerializationError>;

    fn deserialize<R: Read + ?Sized>(
        &self,
        component: &mut Self::Component,
        read: &mut R,
    ) -> Result<(), Self::DeserializationError>;
}

#[derive(Debug)]
pub struct MessagePackCodec<C> {
    _phantom: PhantomData<C>,
}

impl<C> Default for MessagePackCodec<C> {
    fn default() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

impl<C: AutoSerializableComponent> Codec for MessagePackCodec<C> {
    type Component = C;
    type DeserializationError = rmp_serde::decode::Error;
    type SerializationError = rmp_serde::encode::Error;

    const VERSION: PersistanceFormatVersion = C::VERSION;

    fn deserialize<R: Read + ?Sized>(
        &self,
        component: &mut Self::Component,
        read: &mut R,
    ) -> Result<(), Self::DeserializationError> {
        let state = rmp_serde::decode::from_read(read)?;

        component.write_save(state);

        Ok(())
    }

    fn serialize<W: Write + ?Sized>(
        &self,
        component: &Self::Component,
        write: &mut W,
    ) -> Result<(), Self::SerializationError> {
        let state = component.read_save();

        rmp_serde::encode::write(write, &state)?;

        Ok(())
    }
}

pub(crate) trait ErasedCodec: Send + Sync + Debug + 'static {
    fn version(&self) -> PersistanceFormatVersion;

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
pub(crate) struct ErasedCodecWrapper<C>(C);

impl<C> ErasedCodecWrapper<C> {
    pub fn new(codec: C) -> Self {
        Self(codec)
    }
}

impl<C: Codec> ErasedCodec for ErasedCodecWrapper<C> {
    fn version(&self) -> PersistanceFormatVersion {
        C::VERSION
    }

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
