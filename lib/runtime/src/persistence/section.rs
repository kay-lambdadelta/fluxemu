use std::any::{Any, TypeId};

use serde::{
    Deserializer, Serialize, Serializer,
    de::{DeserializeOwned, Error},
};

use crate::{component::Component, persistence::SnapshotError, scheduler::event::Event};

pub trait Section<C> {
    const VERSION: u32;

    fn store<S: Serializer>(component: &C, serializer: S) -> Result<S::Ok, S::Error>;

    fn load<'de, D: Deserializer<'de>>(
        component: &mut C,
        version: u32,
        deserializer: D,
    ) -> Result<(), D::Error>;
}

pub trait PersistentComponentState<C>: DeserializeOwned {
    const VERSION: u32 = 0;

    fn capture(component: &C) -> impl Serialize;

    fn apply(self, component: &mut C);
}

impl<C> PersistentComponentState<C> for () {
    fn capture(_component: &C) -> impl Serialize {}

    fn apply(self, _component: &mut C) {}
}

impl<C, T: PersistentComponentState<C>> Section<C> for T {
    const VERSION: u32 = <T as PersistentComponentState<C>>::VERSION;

    fn store<S: Serializer>(component: &C, serializer: S) -> Result<S::Ok, S::Error> {
        T::capture(component).serialize(serializer)
    }

    fn load<'de, D: Deserializer<'de>>(
        component: &mut C,
        version: u32,
        deserializer: D,
    ) -> Result<(), D::Error> {
        if version != <T as PersistentComponentState<C>>::VERSION {
            return Err(D::Error::custom("Unsupported section version"));
        }

        T::deserialize(deserializer)?.apply(component);

        Ok(())
    }
}

type EventDecoder = fn(&[u8]) -> Result<Box<dyn Event>, SnapshotError>;

#[derive(Debug, Clone, Copy)]
pub(crate) struct ComponentCodec {
    pub snapshot_version: u32,
    pub store_snapshot: fn(&dyn Component, &mut Vec<u8>) -> Result<(), SnapshotError>,
    pub load_snapshot: fn(&mut dyn Component, u32, &[u8]) -> Result<(), SnapshotError>,
    pub has_save: bool,
    pub save_version: u32,
    pub store_save: fn(&dyn Component, &mut Vec<u8>) -> Result<(), SnapshotError>,
    pub load_save: fn(&mut dyn Component, u32, &[u8]) -> Result<(), SnapshotError>,
    pub encode_event: fn(&dyn Event) -> Result<Vec<u8>, SnapshotError>,
    pub decode_event: EventDecoder,
}

impl ComponentCodec {
    pub fn new<C: Component>() -> Self {
        Self {
            snapshot_version: <C::Snapshot as Section<C>>::VERSION,
            store_snapshot: store_snapshot::<C>,
            load_snapshot: load_snapshot::<C>,
            has_save: TypeId::of::<C::Save>() != TypeId::of::<()>(),
            save_version: <C::Save as Section<C>>::VERSION,
            store_save: store_save::<C>,
            load_save: load_save::<C>,
            encode_event: encode_event::<C>,
            decode_event: decode_event::<C>,
        }
    }
}

fn store_snapshot<C: Component>(
    component: &dyn Component,
    output: &mut Vec<u8>,
) -> Result<(), SnapshotError> {
    let component = (component as &dyn Any).downcast_ref::<C>().unwrap();

    <C::Snapshot as Section<C>>::store(component, &mut rmp_serde::Serializer::new(output))?;

    Ok(())
}

fn load_snapshot<C: Component>(
    component: &mut dyn Component,
    version: u32,
    bytes: &[u8],
) -> Result<(), SnapshotError> {
    let component = (component as &mut dyn Any).downcast_mut::<C>().unwrap();

    <C::Snapshot as Section<C>>::load(
        component,
        version,
        &mut rmp_serde::Deserializer::from_read_ref(bytes),
    )?;

    Ok(())
}

fn store_save<C: Component>(
    component: &dyn Component,
    output: &mut Vec<u8>,
) -> Result<(), SnapshotError> {
    let component = (component as &dyn Any).downcast_ref::<C>().unwrap();

    <C::Save as Section<C>>::store(
        component,
        &mut rmp_serde::Serializer::new(output).with_struct_map(),
    )?;

    Ok(())
}

fn load_save<C: Component>(
    component: &mut dyn Component,
    version: u32,
    bytes: &[u8],
) -> Result<(), SnapshotError> {
    let component = (component as &mut dyn Any).downcast_mut::<C>().unwrap();

    <C::Save as Section<C>>::load(
        component,
        version,
        &mut rmp_serde::Deserializer::from_read_ref(bytes),
    )?;

    Ok(())
}

fn encode_event<C: Component>(event: &dyn Event) -> Result<Vec<u8>, SnapshotError> {
    let event = (event as &dyn Any).downcast_ref::<C::Event>().unwrap();

    Ok(rmp_serde::to_vec(event)?)
}

fn decode_event<C: Component>(bytes: &[u8]) -> Result<Box<dyn Event>, SnapshotError> {
    let event: C::Event = rmp_serde::from_slice(bytes)?;

    Ok(Box::new(event))
}
