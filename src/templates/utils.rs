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
use crate::tools::normalize::{absolute_url, normalize_urls, CanonicalUrl};
use crate::tools::scrape::{scrape_all, scrape_embed};
use crate::types::{Context, Html};

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
    let address =
        absolute_url(input).ok_or_else(|| QrawlError::InvalidUrl(input.trim().to_string()))?;
    Ok(Target {
        url: CanonicalUrl::new(&address),
        address,
    })
}

/// Fetch one [`Target`] and compose its [`PageContent`] from the tools, the
/// same steps for every URL. Also returns the fetched page, when it fetched,
/// for a crawl to map children from.
///
/// Fails when the domain filter rejects the page, or when neither the page
/// nor an embed could be fetched.
pub(super) async fn page_content(
    target: &Target,
) -> Result<(PageContent, Option<Fetched>), QrawlError> {
    // fetch: the page, and the platform's embed when the URL has one.
    let (fetched, embed) = futures_util::future::join(
        fetch_strategy(&target.address),
        fetch_embed(target.url.as_str()),
    )
    .await;
    let (fetched, page_error) = match fetched {
        Ok(fetched) => (Some(fetched), None),
        // A filtered page is not stood in for by its embed.
        Err(blocked @ QrawlError::BlockedDomain(_)) => return Err(blocked),
        Err(e) => (None, Some(e)),
    };

    // normalize: the page is the URL that served it. A shortlink redirects
    // to its item, so the served URL may have an embed the given one lacks;
    // a consent or login wall has none, so the given URL's embed stands.
    let url = fetched.as_ref().map_or_else(
        || target.url.clone(),
        |fetched| CanonicalUrl::new(&fetched.url),
    );
    let is_same_item =
        extract_social_parts(url.as_str()) == extract_social_parts(target.url.as_str());
    let embed = if is_same_item {
        embed
    } else {
        fetch_embed(url.as_str()).await.or(embed)
    };

    // scrape: the embed and the page into the same layers.
    let embed_metadata = embed.map_or_else(Vec::new, |embed| scrape_embed(&embed));
    if let Some(e) = page_error {
        if embed_metadata.is_empty() {
            return Err(e);
        }
        log::debug!("{} served only its embed: {}", target.address, e);
    }
    let (body, page_metadata, structured) = match &fetched {
        Some(fetched) => scrape_all(&fetched.html).await,
        None => (Html::new(String::new()), Vec::new(), Vec::new()),
    };
    let metadata = [embed_metadata, page_metadata].concat();

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
    let social = extract_social_parts(url.as_str());
    let page = PageContent {
        address: fetched
            .as_ref()
            .map_or_else(|| target.address.clone(), |fetched| fetched.url.clone()),
        url,
        platform: social.as_ref().map(|social| social.platform),
        kind: social.as_ref().and_then(|social| social.kind),
        handle: social.as_ref().and_then(|social| social.handle.clone()),
        id: social.and_then(|social| social.id),
        title: preview.title,
        text: preview.text,
        author: preview.author,
        image_url: preview.image_url,
        video_url: preview.video_url,
        structured,
        metadata,
        socials,
        body: (!body.as_str().is_empty()).then_some(body),
    };
    Ok((page, fetched))
}

/// Run `work` over `source`, at most `concurrency` at once, and send what
/// `finish` keeps of each result to `tx`, in completion order. Returns `false`
/// when the consumer is gone.
///
/// Work in flight keeps being polled while the consumer is slow — a fetch that
/// isn't polled times out — and no new work starts while `concurrency` results
/// wait, so at most `2 * concurrency` results are held here, beside the
/// `concurrency` the channel holds.
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

/// What a visit made of one [`Target`].
pub(super) struct Visited<T> {
    /// The page the target turned out to be: the canonical form of the URL
    /// that served it, which differs from the target's when it redirected.
    pub(super) url: CanonicalUrl,
    pub(super) item: T,
    /// URLs to crawl at the next level.
    pub(super) children: Vec<String>,
}

/// Breadth-first crawl from `seeds`, `depth` levels of children deep, as one
/// crawl ([`Context::scope`]) on its own task.
///
/// `visit` fetches one [`Target`] at its level (seeds are level 0) and
/// returns what it [`Visited`], or `None` to drop it. Children of the last
/// level are ignored. Each page is yielded once, at the shallowest level that
/// reaches it: a level starts only when the one before it has finished, so
/// the result doesn't depend on which fetch completes first. A target that
/// redirects yields the page it landed on, at the level of the target, unless
/// that page was already yielded. `admit` filters every
/// URL before its visit, and the URL a redirect served after it.
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
    Fut: Future<Output = Option<Visited<T>>> + Send + 'static,
{
    let concurrency = ctx.concurrency.get();
    let (tx, rx) = mpsc::channel(concurrency);
    tokio::spawn(ctx.scope(async move {
        // Every URL ever queued, and every page yielded.
        let mut seen: HashSet<CanonicalUrl> = HashSet::new();
        let mut yielded: HashSet<CanonicalUrl> = HashSet::new();
        let mut frontier = Vec::new();
        let enqueue =
            |urls: &[String], seen: &mut HashSet<CanonicalUrl>, queue: &mut Vec<Target>| {
                queue.extend(
                    urls.iter()
                        .filter_map(|url| parse_url(url).ok())
                        .filter(|target| admit(&target.url) && seen.insert(target.url.clone())),
                );
            };
        enqueue(&seeds, &mut seen, &mut frontier);
        for level in 0..=depth {
            let mut next = Vec::new();
            let mut redirected = Vec::new();
            let consumer_is_listening = drive(
                futures_util::stream::iter(std::mem::take(&mut frontier)),
                concurrency,
                &tx,
                // A page whose visit panics is dropped; the rest of the crawl goes on.
                |target: Target| {
                    let given = target.url.clone();
                    let address = target.address.clone();
                    let visiting = AssertUnwindSafe(visit(target, level)).catch_unwind();
                    async move {
                        match visiting.await {
                            Ok(visited) => (given, visited),
                            Err(_) => {
                                log::error!("crawling {} panicked", address);
                                (given, None)
                            }
                        }
                    }
                },
                |(given, visited): (CanonicalUrl, Option<Visited<T>>)| {
                    let visited = visited?;
                    if visited.url != given {
                        redirected.push(visited);
                        return None;
                    }
                    yielded.insert(given);
                    enqueue(&visited.children, &mut seen, &mut next);
                    Some((level, visited.item))
                },
            )
            .await;
            if !consumer_is_listening {
                return;
            }
            // A redirect is settled once its whole level is in, so which page
            // it yields doesn't depend on which fetch finished first: the page
            // it landed on, unless that page was already yielded.
            for visited in redirected {
                if !admit(&visited.url) || !yielded.insert(visited.url.clone()) {
                    continue;
                }
                seen.insert(visited.url.clone());
                next.retain(|target| target.url != visited.url);
                enqueue(&visited.children, &mut seen, &mut next);
                if tx.send((level, visited.item)).await.is_err() {
                    return;
                }
            }
            frontier = next;
        }
    }));
    receiver_stream(rx)
}
