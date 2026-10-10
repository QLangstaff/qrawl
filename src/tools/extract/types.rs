use serde::Serialize;

/// The content a page declares about itself in its metadata.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub title: Option<String>,
    /// Body / caption / description.
    pub text: Option<String>,
    pub author: Option<String>,
    /// Hero image or thumbnail.
    pub image_url: Option<String>,
    /// A video: a file, or an embeddable player.
    pub video_url: Option<String>,
}
