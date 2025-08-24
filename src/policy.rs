use crate::{error::*, types::*};

/// Quick syntactic checks (no defaults).
/// We enforce: at least one UA; at least one area; at least one field selector across title/headings/paragraphs.
pub fn validate_policy(p: &Policy) -> Result<()> {
    if p.fetch.user_agents.is_empty() {
        return Err(QrawlError::Other(
            "crawl.user_agents must not be empty".into(),
        ));
    }
    if p.scrape.areas.is_empty() {
        return Err(QrawlError::Other("scrape.areas must not be empty".into()));
    }
    let mut any_field = false;
    for a in &p.scrape.areas {
        if !(a.fields.title.is_empty()
            && a.fields.headings.is_empty()
            && a.fields.paragraphs.is_empty()
            && a.fields.images.is_empty()
            && a.fields.links.is_empty()
            && a.fields.lists.is_empty()
            && a.fields.tables.is_empty())
        {
            any_field = true;
            break;
        }
    }
    if !any_field {
        return Err(QrawlError::Other("at least one selector (title/headings/paragraphs/images/links/lists/tables) is required".into()));
    }
    Ok(())
}
