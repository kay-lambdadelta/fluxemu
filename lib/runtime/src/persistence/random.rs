use std::convert::Infallible;

use rand::{SeedableRng, TryRng, rngs::ChaCha20Rng};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_with::{Bytes, DeserializeAs, SerializeAs};

#[derive(Debug, PartialEq, Eq)]
pub struct PersistentRandomGenerator(ChaCha20Rng);

impl SeedableRng for PersistentRandomGenerator {
    type Seed = <ChaCha20Rng as SeedableRng>::Seed;

    fn from_seed(seed: Self::Seed) -> Self {
        Self(ChaCha20Rng::from_seed(seed))
    }
}

impl TryRng for PersistentRandomGenerator {
    type Error = Infallible;

    #[inline]
    fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
        self.0.try_next_u32()
    }

    #[inline]
    fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
        self.0.try_next_u64()
    }

    #[inline]
    fn try_fill_bytes(&mut self, destination: &mut [u8]) -> Result<(), Self::Error> {
        self.0.try_fill_bytes(destination)
    }
}

impl Serialize for PersistentRandomGenerator {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Bytes::serialize_as(&self.0.serialize_state(), serializer)
    }
}

impl<'de> Deserialize<'de> for PersistentRandomGenerator {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let state = Bytes::deserialize_as(deserializer)?;

        Ok(Self(ChaCha20Rng::deserialize_state(&state)))
    }
}
