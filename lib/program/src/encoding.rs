use redb::{Key, TypeName, Value};

use crate::{Derivative, GroupId, ProgramId, ProgramInfo, RomHash, RomId, Variant};

#[doc(hidden)]
#[macro_export]
macro_rules! database_version {
    () => {
        0
    };
}

pub const DATABASE_VERSION: u32 = database_version!();

#[macro_export]
macro_rules! versioned_table_name {
    ($name:literal) => {
        concat!($name, "_v", $crate::database_version!())
    };
}

/// This macro implements [`redb::Value`] for your type by using `serde` and `rmp_serde::to_vec`
///
/// Adding, or reordering any fields will break parsing or produce garbage
macro_rules! messagepack_value {
    ($type:ty, $name:literal) => {
        impl Value for $type {
            type AsBytes<'a> = Vec<u8>;
            type SelfType<'a> = Self;

            fn fixed_width() -> Option<usize> {
                None
            }

            fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
            where
                Self: 'a,
            {
                rmp_serde::from_slice(data).unwrap()
            }

            fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
            where
                Self: 'b,
            {
                rmp_serde::to_vec(value).unwrap()
            }

            fn type_name() -> TypeName {
                TypeName::new($name)
            }
        }

        impl Key for $type {
            fn compare(data1: &[u8], data2: &[u8]) -> std::cmp::Ordering {
                data1.cmp(data2)
            }
        }
    };
}

messagepack_value!(ProgramId, "program_id");
messagepack_value!(GroupId, "group_id");
messagepack_value!(ProgramInfo, "program_info");
messagepack_value!(Variant, "variant");
messagepack_value!(Derivative, "derivative");
messagepack_value!(RomHash, "rom_hash");

impl Value for RomId {
    type AsBytes<'a> = [u8; 32];
    type SelfType<'a> = Self;

    fn fixed_width() -> Option<usize> {
        Some(32)
    }

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a,
    {
        RomId(data.try_into().unwrap())
    }

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
    where
        Self: 'b,
    {
        value.0
    }

    fn type_name() -> TypeName {
        TypeName::new("rom_id")
    }
}

impl Key for RomId {
    fn compare(data1: &[u8], data2: &[u8]) -> std::cmp::Ordering {
        data1.cmp(data2)
    }
}
