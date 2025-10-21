/// Fetch Profiles
///
/// Each profile represents a platform with its natural default browser so User-Agent and headers are always consistent:
/// - `Windows` → Chrome (most popular on Windows)
/// - `MacOS` → Safari (native browser)
/// - `IOS` → Safari (only real browser on iPhone)
/// - `Android` → Chrome (most popular on Android)
/// - `Minimal` → Basic Mozilla (no platform-specific headers)
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FetchProfile {
    /// Minimal (just User-Agent)
    Minimal,

    /// Chrome on Windows (most popular desktop platform)
    Windows,

    /// Safari on macOS
    MacOS,

    /// Safari on iPhone (most popular mobile browser)
    IOS,

    /// Chrome on Android
    Android,
}

impl Default for FetchProfile {
    fn default() -> Self {
        Self::Windows // Most common platform
    }
}

impl FetchProfile {
    /// Fetch Profile Name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Minimal => "Minimal",
            Self::Windows => "Windows (Chrome)",
            Self::MacOS => "macOS (Safari)",
            Self::IOS => "iOS (Safari)",
            Self::Android => "Android (Chrome)",
        }
    }
}
