//! Shared building blocks for the `qrawl_*` templates.

use std::collections::{HashSet, VecDeque};
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::sync::Mutex;

use futures_util::future::FutureExt;
use futures_util::stream::{FuturesUnordered, Stream, StreamExt};
use tokio::sync::mpsc;

use crate::errors::QrawlError;
use crate::tools::extract::{extract_preview, extract_social_parts};
use crate::tools::fetch::{fetch_embed, fetch_strategy};
use crate::tools::map::{map_children_from_document, map_page_from_document};
use crate::tools::normalize::{absolute_url, normalize_urls, CanonicalUrl};
use crate::tools::scrape::{scrape_embed, scrape_from_document};
use crate::types::{Context, Html, SocialKind};
use crate::utils::with_document;

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

/// Whether a visit maps the page's children, for a crawl to go on to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Children {
    Map,
    Skip,
}

/// Fetch one [`Target`] and compose its [`PageContent`] from the tools, the
/// same steps for every URL, in one parse of the page. Also returns the
/// page's children when asked to map them; empty when not, or when only the
/// embed could be fetched.
///
/// Fails when the domain filter rejects the page, or when neither the page
/// nor an embed could be fetched.
pub(super) async fn page_content(
    target: &Target,
    children: Children,
) -> Result<(PageContent, Vec<String>), QrawlError> {
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
        Err(error) => (None, Some(error)),
    };

    // normalize: the page is the URL that served it. A shortlink redirects
    // to its item, whose embed is the one to read; a consent or login wall is
    // no item.
    let url = fetched.as_ref().map_or_else(
        || target.url.clone(),
        |fetched| CanonicalUrl::new(&fetched.address),
    );
    let item = |url: &CanonicalUrl| {
        extract_social_parts(url.as_str()).map(|social| (social.platform, social.kind, social.id))
    };
    let embed = match item(&url) {
        served if served == item(&target.url) => embed,
        // A different item: its embed, never the given one's.
        Some((
            _,
            Some(SocialKind::Post | SocialKind::Video | SocialKind::Photo | SocialKind::Comment),
            _,
        )) => fetch_embed(url.as_str()).await,
        // A wall, a profile, or a plain page: the given item's embed stands.
        _ => embed,
    };

    // scrape: the embed and the page into the same layers.
    let embed_metadata = embed.map_or_else(Vec::new, |embed| scrape_embed(&embed));
    if let Some(error) = page_error {
        if embed_metadata.is_empty() {
            return Err(error);
        }
        log::debug!("{} served only its embed: {}", target.address, error);
    }
    // scrape + map, from one parse: the layers, every link, and the children
    // when asked.
    let ((body, page_metadata, structured), links, children) = match &fetched {
        Some(fetched) => {
            let served_by = fetched.address.clone();
            with_document(&fetched.html, move |document| {
                let children = match children {
                    Children::Map => map_children_from_document(document, &served_by),
                    Children::Skip => Vec::new(),
                };
                (
                    scrape_from_document(document),
                    map_page_from_document(document, &served_by),
                    children,
                )
            })
            .await
        }
        None => (
            (Html::new(String::new()), Vec::new(), Vec::new()),
            Vec::new(),
            Vec::new(),
        ),
    };
    let metadata = [embed_metadata, page_metadata].concat();

    // normalize + extract: the social URLs among the page's links.
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
            .map_or_else(|| target.address.clone(), |fetched| fetched.address.clone()),
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
    Ok((page, children))
}

/// Run `work` over `source`, at most `concurrency` at once, and send what
/// `finish` keeps of each result to `sender`, in completion order. Returns `false`
/// when the consumer is gone.
///
/// Work in flight keeps being polled while the consumer is slow — a fetch that
/// isn't polled times out — and no new work starts while `concurrency` results
/// wait, so at most `2 * concurrency` results are held here, beside the
/// `concurrency` the channel holds.
async fn drive<S, Work, Fut, Finish, T>(
    source: S,
    concurrency: usize,
    sender: &mpsc::Sender<T>,
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
            permit = sender.reserve(), if !ready.is_empty() => match permit {
                Ok(permit) => permit.send(ready.pop_front().expect("ready is not empty")),
                Err(_) => return false,
            },
            item = source.next(), if can_start => in_flight.extend(item.map(&work)),
            output = in_flight.next(), if !in_flight.is_empty() => {
                ready.extend(output.and_then(&mut finish));
            }
            _ = sender.closed() => return false,
        }
    }
}

fn receiver_stream<T: Send + 'static>(
    receiver: mpsc::Receiver<T>,
) -> impl Stream<Item = T> + Send + 'static {
    futures_util::stream::unfold(receiver, |mut receiver| async move {
        receiver.recv().await.map(|item| (item, receiver))
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
/// reaches it: a level starts only when the one before it has finished. A
/// target that redirects counts as the page it landed on, so within a level
/// the first visit to arrive at a page yields it, and the item is that
/// visit's: the address it served and the children it listed. A target whose
/// page was yielded by the time its turn comes isn't visited; one already in
/// flight when a redirect lands on its page is fetched and dropped. `admit`
/// filters every URL before its visit, and the URL a redirect served after it.
///
/// Yields `(level, item)` in completion order within a level. Every visit of
/// a level shares one pool of `context.concurrency` ([`drive`]), and dropping the
/// stream stops the crawl at once.
pub(super) fn crawl<T, Admit, Visit, Fut>(
    context: Context,
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
    let concurrency = context.concurrency.get();
    let (sender, receiver) = mpsc::channel(concurrency);
    tokio::spawn(context.scope(async move {
        // Every URL ever queued, and every page yielded. Pulling a target
        // reads `yielded` and finishing a visit writes it, from two closures,
        // so a lock shares it; it's never held across an await.
        let mut seen: HashSet<CanonicalUrl> = HashSet::new();
        let yielded: Mutex<HashSet<CanonicalUrl>> = Mutex::new(HashSet::new());
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
            let consumer_is_listening = drive(
                // A target a redirect already landed on isn't fetched again.
                futures_util::stream::iter(std::mem::take(&mut frontier)).filter(|target| {
                    let is_yielded = yielded.lock().expect("yielded").contains(&target.url);
                    futures_util::future::ready(!is_yielded)
                }),
                concurrency,
                &sender,
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
                    // The first visit to arrive at a page yields it, whether
                    // it was asked for that page or redirected to it.
                    let is_redirect = visited.url != given;
                    if (is_redirect && !admit(&visited.url))
                        || !yielded.lock().expect("yielded").insert(visited.url.clone())
                    {
                        return None;
                    }
                    seen.insert(visited.url);
                    enqueue(&visited.children, &mut seen, &mut next);
                    Some((level, visited.item))
                },
            )
            .await;
            if !consumer_is_listening {
                return;
            }
            frontier = next;
        }
    }));
    receiver_stream(receiver)
}
