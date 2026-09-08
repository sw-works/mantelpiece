//! Newtype identifiers. Each one serializes as a plain string.

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

macro_rules! string_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_owned())
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }
    };
}

macro_rules! uuid_id {
    ($(#[$meta:meta])* $name:ident) => {
        string_id!($(#[$meta])* $name);

        impl $name {
            /// Mint a fresh random id.
            pub fn random() -> Self {
                Self(Uuid::new_v4().to_string())
            }
        }
    };
}

string_id!(
    /// Owner's Cognito `sub`. Also the tenant partition key everywhere.
    HouseholdId
);
uuid_id!(
    /// A person who uploaded into a household: the owner, an Amazon-signed-in friend, or a guest.
    ContributorId
);
uuid_id!(MediaId);
uuid_id!(TripId);
uuid_id!(AlbumId);
uuid_id!(PlaylistId);
uuid_id!(
    /// One upload batch, which becomes one Step Functions execution.
    BatchId
);
string_id!(
    /// Opaque TV install identifier, chosen by the TV app on first launch.
    DeviceId
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_serialize_as_plain_strings() {
        let id = HouseholdId::new("abc");
        assert_eq!(serde_json::to_string(&id).unwrap(), "\"abc\"");
        let back: HouseholdId = serde_json::from_str("\"abc\"").unwrap();
        assert_eq!(back, id);
    }

    #[test]
    fn random_ids_differ() {
        assert_ne!(MediaId::random(), MediaId::random());
    }
}
