use serde::{Deserialize, Serialize};

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, IdError> {
                let value = value.into();
                (!value.trim().is_empty())
                    .then_some(Self(value))
                    .ok_or(IdError::Empty)
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

id_type!(AppName);
id_type!(UserId);
id_type!(AgentName);
id_type!(SessionId);
id_type!(InvocationId);
id_type!(EventId);
id_type!(ArtifactName);
id_type!(StateKey);

impl AppName {
    pub(crate) fn trusted(value: &'static str) -> Self {
        Self(value.to_owned())
    }
}

impl UserId {
    pub(crate) fn trusted(value: &'static str) -> Self {
        Self(value.to_owned())
    }
}

impl EventId {
    pub fn for_index(index: usize) -> Self {
        Self(format!("event-{index}"))
    }
}

impl ArtifactName {
    pub fn version_key(&self, version: ArtifactVersionNumber) -> String {
        format!("{}@{}", self.as_str(), version.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ArtifactVersionNumber(pub u32);

impl ArtifactVersionNumber {
    pub const FIRST: Self = Self(1);

    pub const fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IdError {
    #[error("identifier cannot be empty")]
    Empty,
}
