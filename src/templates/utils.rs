//! Shared building blocks for the `qrawl_*` templates.

use std::collections::{HashSet, VecDeque};
use std::future::Future;
use std::panic::AssertUnwindSafe;

use futures_util::future::FutureExt;
use futures_util::stream::{FuturesUnordered, Stream, StreamExt};
use tokio::sync::mpsc;

use crate::errors::QrawlError;
use crate::tools::extract::{extract_preview, extract_social_parts};
use crate::tools::fetch::{fetch_embed, fetch_strategy, Fetched};
use crate::tools::map::map_page;
use crate::tools::normalize::{normalize_urls, CanonicalUrl};
use crate::tools::scrape::{scrape_all, scrape_embed};
use crate::types::{Context, Html, Metadata};
use crate::utils::embed_keys::PREFIXES;

use super::types::PageContent;

/// A URL to crawl. `address` is what gets fetched: the URL as given, because
/// a server can answer `http://`, `www.`, and a trailing slash differently.
/// `url` only identifies the page, so two addresses of one page are crawled
/// once.
#[derive(Debug, Clone)]
pub(super) struct Target {
    pub(super) address: String,
    pub(super) url: CanonicalUrl,
}

/// The [`Target`] for `input`, or an error — before any network — when
/// `input` isn't an HTTP URL. An input without a scheme (`example.com/a`) is
/// fetched over `https`.
pub(super) fn parse_url(input: &str) -> Result<Target, QrawlError> {
    let input = input.trim();
    let url = CanonicalUrl::new(input);
    // `CanonicalUrl` is `https` for every HTTP URL and leaves anything else
    // (`mailto:`, `ftp:`, a relative path) as written.
    // An email address parses as a host with a user (`https://a@b.com`).
    let is_http = url::Url::parse(url.as_str())
        .is_ok_and(|parsed| parsed.scheme() == "https" && parsed.username().is_empty());
    if !is_http {
        return Err(QrawlError::InvalidUrl(input.to_string()));
    }
    let lower = input.to_ascii_lowercase();
    let address = if lower.starts_with("http://") || lower.starts_with("https://") {
        input.to_string()
    } else {
        format!("https://{input}")
    };
    Ok(Target { address, url })
}

/// Fetch one [`Target`] and compose its [`PageContent`] from the tools, the
/// same steps for every URL. Also returns the fetched page, when it fetched,
/// for a crawl to map children from.
///
/// Fails only when neither the page nor an embed could be fetched.
pub(super) async fn page_content(
    target: &Target,
) -> Result<(PageContent, Option<Fetched>), QrawlError> {
    let url = target.url.as_str();

    // fetch: the page, and the platform's embed when the URL has one.
    let (fetched, embed) =
        futures_util::future::join(fetch_strategy(&target.address), fetch_embed(url)).await;

    // scrape: the embed and the page into the same layers.
    let embed_metadata = embed.map_or_else(Vec::new, |embed| scrape_embed(&embed));
    let fetched = match fetched {
        Ok(fetched) => Some(fetched),
        Err(e) if embed_metadata.is_empty() => return Err(e),
        Err(_) => None,
    };
    let (body, page_metadata, structured) = match &fetched {
        Some(fetched) => scrape_all(&fetched.html).await,
        None => (Html::new(String::new()), Vec::new(), Vec::new()),
    };
    let metadata = merge_metadata(embed_metadata, page_metadata);

    // map + normalize + extract: the social URLs the page links to.
    let links = match &fetched {
        Some(fetched) => map_page(&fetched.html, &fetched.url).await,
        None => Vec::new(),
    };
    let socials = normalize_urls(&links)
        .into_iter()
        .filter(|link| extract_social_parts(link).is_some_and(|social| social.kind.is_some()))
        .collect();

    // extract: the content fields and the URL's social facts.
    let preview = extract_preview(&metadata);
    let social = extract_social_parts(url);
    let page = PageContent {
        url: target.url.clone(),
        address: fetched
            .as_ref()
            .map_or_else(|| target.address.clone(), |fetched| fetched.url.clone()),
        platform: social.as_ref().map(|social| social.platform),
        kind: social.as_ref().and_then(|social| social.kind),
        title: preview.title,
        text: preview.text,
        author: preview.author.or(social.and_then(|social| social.handle)),
        image_url: preview.image_url,
        video_url: preview.video_url,
        structured,
        metadata,
        socials,
        body: (!body.as_str().is_empty()).then_some(body),
    };
    Ok((page, fetched))
}

/// The embed's pairs, then the page's — without any page pair that uses an
/// embed key, so a page's own `<meta>` tags can't pose as its embed.
fn merge_metadata(embed_metadata: Metadata, page_metadata: Metadata) -> Metadata {
    let is_embed_key = |key: &str| PREFIXES.iter().any(|prefix| key.starts_with(prefix));
    embed_metadata
        .into_iter()
        .chain(
            page_metadata
                .into_iter()
                .filter(|(key, _)| !is_embed_key(key)),
        )
        .collect()
}

/// Run `work` over `source`, at most `concurrency` at once, and send what
/// `finish` keeps of each result to `tx`, in completion order. Returns `false`
/// when the consumer is gone.
///
/// Work in flight keeps being polled while the consumer is slow — a fetch that
/// isn't polled times out — and no new work starts while `concurrency` results
/// wait, so at most `2 * concurrency` results are ever held.
async fn drive<S, Work, Fut, Finish, T>(
    source: S,
    concurrency: usize,
    tx: &mpsc::Sender<T>,
    work: Work,
    mut finish: Finish,
) -> bool
where
    S: Stream,
    Work: Fn(S::Item) -> Fut,
    Fut: Future,
    Finish: FnMut(Fut::Output) -> Option<T>,
{
    let mut source = Box::pin(source.fuse());
    let mut in_flight = FuturesUnordered::new();
    let mut ready = VecDeque::new();
    loop {
        if source.is_done() && in_flight.is_empty() && ready.is_empty() {
            return true;
        }
        let can_start =
            !source.is_done() && in_flight.len() < concurrency && ready.len() < concurrency;
        tokio::select! {
            permit = tx.reserve(), if !ready.is_empty() => match permit {
                Ok(permit) => permit.send(ready.pop_front().expect("ready is not empty")),
                Err(_) => return false,
            },
            item = source.next(), if can_start => in_flight.extend(item.map(&work)),
            output = in_flight.next(), if !in_flight.is_empty() => {
                ready.extend(output.and_then(&mut finish));
            }
            _ = tx.closed() => return false,
        }
    }
}

fn receiver_stream<T: Send + 'static>(
    rx: mpsc::Receiver<T>,
) -> impl Stream<Item = T> + Send + 'static {
    futures_util::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|item| (item, rx))
    })
}

/// Breadth-first crawl from `seeds`, `depth` levels of children deep, as one
/// crawl ([`Context::scope`]) on its own task.
///
/// `visit` fetches one [`Target`] at its level (seeds are level 0) and
/// returns its item plus the URLs to crawl at the next level, or `None` to
/// drop it. Children of the last level are ignored. Each page is visited once,
/// at the shallowest level that reaches it: a level starts only when the one
/// before it has finished, so the result doesn't depend on which fetch
/// completes first. `admit` filters every URL before its visit.
///
/// Yields `(level, item)` in completion order within a level. Every visit of
/// a level shares one pool of `ctx.concurrency` ([`drive`]), and dropping the
/// stream stops the crawl at once.
pub(super) fn crawl<T, Admit, Visit, Fut>(
    ctx: Context,
    seeds: Vec<String>,
    depth: usize,
    admit: Admit,
    visit: Visit,
) -> impl Stream<Item = (usize, T)> + Send + 'static
where
    T: Send + 'static,
    Admit: Fn(&CanonicalUrl) -> bool + Send + Sync + 'static,
    Visit: Fn(Target, usize) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Option<(T, Vec<String>)>> + Send + 'static,
{
    let concurrency = ctx.concurrency.get();
    let (tx, rx) = mpsc::channel(concurrency);
    tokio::spawn(ctx.scope(async move {
        let mut seen = HashSet::new();
        let mut admit_new = |urls: &[String]| -> Vec<Target> {
            urls.iter()
                .filter_map(|url| parse_url(url).ok())
                .filter(|target| admit(&target.url) && seen.insert(target.url.clone()))
                .collect()
        };
        let mut frontier = admit_new(&seeds);
        for level in 0..=depth {
            let mut next = Vec::new();
            let consumer_is_listening = drive(
                futures_util::stream::iter(std::mem::take(&mut frontier)),
                concurrency,
                &tx,
                // A page whose visit panics is dropped; the rest of the crawl goes on.
                |target: Target| {
                    let address = target.address.clone();
                    let visiting = AssertUnwindSafe(visit(target, level)).catch_unwind();
                    async move {
                        match visiting.await {
                            Ok(visited) => visited,
                            Err(_) => {
                                log::error!("crawling {} panicked", address);
                                None
                            }
                        }
                    }
                },
                |visited| {
                    let (item, children) = visited?;
                    next.extend(admit_new(&children));
                    Some((level, item))
                },
            )
            .await;
            if !consumer_is_listening {
                return;
            }
            frontier = next;
        }
    }));
    receiver_stream(rx)
}
