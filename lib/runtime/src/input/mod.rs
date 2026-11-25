use std::{collections::HashMap, fmt::Debug, hash::Hash};

use fluxemu_input::{DirectionPart, InputId, InputShape, InputState};
use nalgebra::Vector2;
use ron::value::RawValue;
use rustc_hash::FxBuildHasher;
use serde::{Serialize, de::DeserializeOwned};

use crate::path::ResourcePath;

pub trait GuestInput:
    Copy + Debug + Eq + Hash + Serialize + DeserializeOwned + Send + Sync + 'static
{
    fn all() -> impl Iterator<Item = Self>;

    fn shape(&self) -> InputShape {
        InputShape::Button
    }

    fn split(self) -> (Self, DirectionPart) {
        (self, DirectionPart::All)
    }
}

#[derive(Debug, Clone)]
pub struct GuestInputDescriptor {
    pub serialized: Box<RawValue>,
    pub shape: InputShape,
}

#[derive(Debug, Clone)]
pub struct ResolvedGuestInput {
    pub whole: Box<RawValue>,
    pub part: DirectionPart,
    pub shape: InputShape,
}

/// Metadata for a guest input device
#[derive(Debug)]
pub struct GuestInputDeviceMetadata {
    /// A stable path for this device
    pub path: ResourcePath,
    pub inputs: Box<[GuestInputDescriptor]>,
    /// Some "good" default mappings the frontends and shells can use for intuitive gameplay for players who don't rebind anything
    pub default_mappings: HashMap<InputId, Box<RawValue>, FxBuildHasher>,
}

pub trait ErasedGuestInputDevice: Debug + Send + Sync {
    /// Returns a reference to the device's metadata that the component registered it with
    fn metadata(&self) -> &GuestInputDeviceMetadata;

    fn resolve(&self, input: &RawValue) -> Option<ResolvedGuestInput>;

    /// Sets the state of a whole input on this device, returning false if the device has no such input or the value has the wrong shape
    fn set_state(&self, input: &RawValue, value: InputState) -> bool;
}

/// Input state for a components registered input device
#[derive(Debug)]
pub struct GuestInputDevice<T: GuestInput> {
    state: scc::HashMap<T, InputState>,
    metadata: GuestInputDeviceMetadata,
}

impl<T: GuestInput> GuestInputDevice<T> {
    pub(crate) fn new(
        path: ResourcePath,
        default_mappings: impl IntoIterator<Item = (InputId, T)>,
    ) -> Self {
        let state = scc::HashMap::default();

        let inputs = T::all()
            .map(|input| {
                assert_eq!(
                    input.split(),
                    (input, DirectionPart::All),
                    "Input {input:?} on {path} is not a whole input"
                );

                let shape = input.shape();

                assert!(
                    state.insert_sync(input, shape.neutral()).is_ok(),
                    "Duplicate input {input:?} on {path}"
                );

                GuestInputDescriptor {
                    serialized: RawValue::from_rust(&input).unwrap(),
                    shape,
                }
            })
            .collect();

        Self {
            state,
            metadata: GuestInputDeviceMetadata {
                path,
                inputs,
                default_mappings: default_mappings
                    .into_iter()
                    .map(|(host, guest)| (host, RawValue::from_rust(&guest).unwrap()))
                    .collect(),
            },
        }
    }

    #[inline]
    pub fn get(&self, input: T) -> InputState {
        let (whole, part) = input.split();

        self.state
            .read_sync(&whole, |_, value| *value)
            .and_then(|value| value.part(part))
            .unwrap_or_else(|| panic!("Input {input:?} not found on {}", self.metadata.path))
    }

    #[inline]
    pub fn is_held(&self, input: T) -> bool {
        self.get(input).is_held()
    }

    #[inline]
    pub fn direction(&self, input: T) -> Vector2<f32> {
        self.get(input).as_direction().unwrap_or_else(|| {
            panic!(
                "Input {input:?} on {} is not a direction",
                self.metadata.path
            )
        })
    }
}

impl<T: GuestInput> ErasedGuestInputDevice for GuestInputDevice<T> {
    fn metadata(&self) -> &GuestInputDeviceMetadata {
        &self.metadata
    }

    fn resolve(&self, input: &RawValue) -> Option<ResolvedGuestInput> {
        let (whole, part) = input.into_rust::<T>().ok()?.split();

        Some(ResolvedGuestInput {
            whole: RawValue::from_rust(&whole).unwrap(),
            part,
            shape: whole.shape(),
        })
    }

    fn set_state(&self, input: &RawValue, value: InputState) -> bool {
        let Ok(input) = input.into_rust::<T>() else {
            return false;
        };

        if input.split().1 != DirectionPart::All || input.shape() != value.shape() {
            return false;
        }

        self.state
            .update_sync(&input, |_, value_ref| *value_ref = value)
            .is_some()
    }
}
