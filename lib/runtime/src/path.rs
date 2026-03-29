use std::{
    borrow::Cow,
    fmt::{Display, Write},
    str::FromStr,
    sync::Arc,
};

use itertools::Itertools;
use redb::{Key, TypeName, Value};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("Path segment contains whitespace")]
    Whitespace,
    #[error("Path is too short")]
    TooShort,
    #[error("Segment cannot hold this character")]
    InvalidCharacter(char),
    #[error("Component path cannot hold this character")]
    InvalidPathType(String),
}

/// Valid formats include
///
/// `":component/component_1"`
///
/// `":component/component_1/component_2"`
///
/// `":resource/component_1/component_2/resource_1"`
///
/// Item names cannot be empty or contain whitespace or `/`
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComponentPath(Arc<str>);

impl ComponentPath {
    pub fn new(component: String) -> Result<Self, Error> {
        let segments: Vec<&str> = component.split('/').collect();

        if segments.is_empty() {
            return Err(Error::TooShort);
        }

        validate_segments(segments.into_iter())?;

        Ok(ComponentPath(Arc::from(component)))
    }

    pub fn join(&self, segment: &str) -> Result<ComponentPath, Error> {
        validate_segments([segment])?;

        let path = format!("{}/{}", self.0, segment);

        Ok(ComponentPath(Arc::from(path)))
    }

    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.0.split('/')
    }

    pub fn parent(&self) -> Option<ComponentPath> {
        let segment_count = self.iter().count();

        if segment_count < 2 {
            return None;
        }

        let path = self.iter().take(segment_count - 1).join("/");

        Some(ComponentPath(Arc::from(path)))
    }

    pub fn name(&self) -> &str {
        self.iter().last().unwrap()
    }

    pub fn into_resource(
        self,
        resource: impl Into<Cow<'static, str>>,
    ) -> Result<ResourcePath, Error> {
        let resource = resource.into();

        validate_segments([resource.as_ref()])?;

        Ok(ResourcePath {
            component: Some(self),
            resource,
        })
    }
}

impl Display for ComponentPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(":component/")?;
        f.write_str(&self.0)
    }
}

impl FromStr for ComponentPath {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let segments: Vec<&str> = s.split('/').filter(|seg| !seg.is_empty()).collect();

        if segments.len() < 2 {
            return Err(Error::TooShort);
        }

        if segments[0] != ":component" {
            return Err(Error::InvalidPathType(segments[0].to_string()));
        }

        validate_segments(segments[1..].iter().copied())?;

        Ok(ComponentPath(Arc::from(segments[1..].join("/"))))
    }
}

impl Serialize for ComponentPath {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ComponentPath {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

impl Value for ComponentPath {
    type SelfType<'a> = Self;
    type AsBytes<'a> = String;

    fn fixed_width() -> Option<usize> {
        None
    }

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a,
    {
        String::from_bytes(data).parse().unwrap()
    }

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
    where
        Self: 'b,
    {
        value.to_string()
    }

    fn type_name() -> TypeName {
        TypeName::new("component_path")
    }
}

impl Key for ComponentPath {
    fn compare(data1: &[u8], data2: &[u8]) -> std::cmp::Ordering {
        String::from_utf8_lossy(data1).cmp(&String::from_utf8_lossy(data2))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResourcePath {
    component: Option<ComponentPath>,
    resource: Cow<'static, str>,
}

impl ResourcePath {
    pub fn new(
        component: Option<ComponentPath>,
        resource: impl Into<Cow<'static, str>>,
    ) -> Result<Self, Error> {
        let resource = resource.into();

        validate_segments([resource.as_ref()])?;

        Ok(ResourcePath {
            component,
            resource,
        })
    }

    /// Returns the full owning `ComponentId`.
    pub fn parent(&self) -> Option<&ComponentPath> {
        self.component.as_ref()
    }

    pub fn name(&self) -> &str {
        self.resource.as_ref()
    }
}

impl Display for ResourcePath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(":resource/")?;

        if let Some(component) = &self.component {
            f.write_str(&component.iter().join("/"))?;
            f.write_char('/')?;
        }

        f.write_str(&self.resource)
    }
}

impl FromStr for ResourcePath {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let segments: Vec<&str> = s.split('/').filter(|seg| !seg.is_empty()).collect();

        if segments.len() < 2 {
            return Err(Error::TooShort);
        }

        if segments[0] != ":resource" {
            return Err(Error::InvalidPathType(segments[0].to_string()));
        }

        validate_segments(segments[1..].iter().copied())?;

        let resource = segments.last().unwrap().to_string();

        let component = if segments.len() == 2 {
            None
        } else {
            Some(ComponentPath(Arc::from(
                segments[1..segments.len() - 1].join("/"),
            )))
        };

        Ok(ResourcePath {
            component,
            resource: Cow::Owned(resource),
        })
    }
}

impl Serialize for ResourcePath {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ResourcePath {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

impl Value for ResourcePath {
    type SelfType<'a> = Self;
    type AsBytes<'a> = String;

    fn fixed_width() -> Option<usize> {
        None
    }

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a,
    {
        String::from_bytes(data).parse().unwrap()
    }

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
    where
        Self: 'b,
    {
        value.to_string()
    }

    fn type_name() -> TypeName {
        TypeName::new("resource_path")
    }
}

impl Key for ResourcePath {
    fn compare(data1: &[u8], data2: &[u8]) -> std::cmp::Ordering {
        String::from_utf8_lossy(data1).cmp(&String::from_utf8_lossy(data2))
    }
}

fn validate_segments<'a>(segments: impl IntoIterator<Item = &'a str> + 'a) -> Result<(), Error> {
    for segment in segments {
        if segment.is_empty() {
            return Err(Error::TooShort);
        }

        for c in segment.chars() {
            if c.is_whitespace() {
                return Err(Error::Whitespace);
            }
            if c == '/' {
                return Err(Error::InvalidCharacter(c));
            }
            if c == ':' {
                return Err(Error::InvalidCharacter(c));
            }
        }
    }

    Ok(())
}
