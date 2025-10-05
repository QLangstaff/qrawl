//! Common types shared across tools for type safety without dependencies

use serde_json::Value;

/// JSON-LD array of schema.org objects.
pub type Jsonld = Vec<Value>;

/// Metadata key-value pairs.
pub type Metadata = Vec<(String, String)>;
