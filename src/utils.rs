//! Shared Utils
//!
//! What more than one tool needs. A tool never calls another tool; it calls
//! this.

use ego_tree::NodeId;
use html5ever::driver::Parser;
use html5ever::tendril::{StrTendril, TendrilSink};
use html5ever::tokenizer::{BufferQueue, TokenSink, Tokenizer, TokenizerOpts};
use html5ever::tree_builder::{ElementFlags, NodeOrText, QuirksMode, TreeSink};
use html5ever::{namespace_url, ns, Attribute, ExpandedName, LocalName, ParseOpts, QualName};
use scraper::Selector;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::LazyLock;

use crate::types::{Html, SocialPlatform};

mod social;

pub(crate) use social::social_parts;

/// Selector for anchor elements with hrefs.
pub(crate) static LINK_SELECTOR: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("a[href]").expect("valid link selector"));

/// Selector for JSON-LD script tags.
pub(crate) static JSONLD_SELECTOR: LazyLock<Selector> = LazyLock::new(|| {
    Selector::parse("script[type='application/ld+json']").expect("valid jsonld selector")
});

/// The lowercased host of `url`, without the root dot of a fully qualified
/// name (`example.com.`), or `None` when `url` has no host (`mailto:`,
/// `file:`).
pub(crate) fn host_of_url(url: &url::Url) -> Option<String> {
    url.host_str()
        .map(|host| host.trim_end_matches('.').to_ascii_lowercase())
}

/// [`host_of_url`] for a URL string, or `None` when it doesn't parse.
pub(crate) fn host_of(url: &str) -> Option<String> {
    host_of_url(&url::Url::parse(url).ok()?)
}

/// Whether `host` is `domain` or a subdomain of it (`old.reddit.com` matches
/// `reddit.com`; `notreddit.com` doesn't).
pub(crate) fn host_matches(host: &str, domain: &str) -> bool {
    host == domain
        || host
            .strip_suffix(domain)
            .is_some_and(|prefix| prefix.ends_with('.'))
}

/// A host in the form two hosts compare in: lowercased, IDNA-encoded, without
/// the root dot of a fully qualified name or a `www.` prefix
/// (`WWW.Example.COM.` → `example.com`). `www.com` keeps its `www.`: without it
/// the host would be a bare TLD.
pub(crate) fn canonical_domain(host: &str) -> String {
    let lower = host.trim_end_matches('.').to_ascii_lowercase();
    let idna = idna::domain_to_ascii(&lower).unwrap_or(lower);
    let mut apex = idna.as_str();
    while let Some(stripped) = apex.strip_prefix("www.").filter(|rest| rest.contains('.')) {
        apex = stripped;
    }
    apex.to_string()
}

/// The domain a list entry names, in [`canonical_domain`] form. An entry may
/// be written as a host, a wildcard (`*.example.com`), a host with a port or
/// a path, or a whole URL.
pub(crate) fn listed_domain(entry: &str) -> String {
    let entry = entry.trim();
    let host = host_of(entry).unwrap_or_else(|| {
        let without_wildcard = entry.trim_start_matches("*.").trim_start_matches('.');
        let without_path = without_wildcard
            .split(['/', '?', '#'])
            .next()
            .unwrap_or(without_wildcard);
        without_path
            .rsplit_once(':')
            .filter(|(_, port)| {
                !port.is_empty() && port.chars().all(|character| character.is_ascii_digit())
            })
            .map_or(without_path, |(host, _port)| host)
            .to_string()
    });
    canonical_domain(&host)
}

/// The `http(s)` URL `input` names — with `https://` when it names no scheme
/// (`example.com/a`) — or `None` when it isn't one (`mailto:`, `tel:911`,
/// `ftp:`, a relative path, an email address, a bare word or number).
///
/// With an explicit `http(s)://`, any host is taken as written (`intranet`,
/// an IDN, an IP address). Without one, the input has to look like a host for
/// `https://` to be put before it.
pub(crate) fn absolute_url(input: &str) -> Option<String> {
    let input = input.trim();
    let http_url = |candidate: &str| {
        url::Url::parse(candidate)
            .ok()
            .filter(|url| matches!(url.scheme(), "http" | "https") && url.host().is_some())
    };
    let lower = input.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return http_url(input).map(|_| input.to_string());
    }
    // `https:example.com` is a URL the `url` crate completes.
    if let Some(rest) = lower
        .strip_prefix("https:")
        .or_else(|| lower.strip_prefix("http:"))
    {
        return http_url(input)
            .filter(|_| names_a_host(rest.trim_start_matches('/')))
            .map(String::from);
    }
    if !names_a_host(input) {
        return None;
    }
    let candidate = format!("https://{input}");
    http_url(&candidate).map(|_| candidate)
}

/// Whether a scheme-less input starts with a host: a name under a TLD,
/// `localhost`, or an IP address, with an optional port — not a scheme
/// (`tel:911`), a path, an email address, or a number (`3.14`).
fn names_a_host(input: &str) -> bool {
    let authority = input.split(['/', '?', '#']).next().unwrap_or(input);
    if authority.contains('@') {
        return false;
    }
    let host = match authority.strip_prefix('[') {
        // A bracketed IPv6 address, with an optional port after it.
        Some(bracketed) => return bracketed.contains(']'),
        None => match authority.rsplit_once(':') {
            Some((host, port))
                if !port.is_empty() && port.chars().all(|character| character.is_ascii_digit()) =>
            {
                host
            }
            Some(_) => return false,
            None => authority,
        },
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host == "localhost" {
        return true;
    }
    let labels: Vec<&str> = host.split('.').collect();
    // A label is letters, digits, hyphens and underscores; an IDN's letters
    // aren't ASCII.
    let is_label = |label: &&str| {
        !label.is_empty()
            && label.chars().all(|character| {
                character.is_alphanumeric() || character == '-' || character == '_'
            })
    };
    let is_ipv4 = labels.len() == 4 && labels.iter().all(|label| label.parse::<u8>().is_ok());
    // A TLD is letters, or punycode (`xn--p1ai`).
    let has_tld = labels.len() >= 2
        && labels.iter().all(is_label)
        && labels.last().is_some_and(|tld| {
            tld.len() >= 2 && (tld.starts_with("xn--") || tld.chars().all(char::is_alphabetic))
        });
    is_ipv4 || has_tld
}

/// The social platform a host belongs to, or `None` for a non-social host.
/// Subdomain-aware (`old.reddit.com`, `m.facebook.com`, `vm.tiktok.com`) and
/// handles the alt short-domains (`youtu.be`, `fb.me`, …).
pub(crate) fn social_platform(host: &str) -> Option<SocialPlatform> {
    const DOMAINS: &[(&str, SocialPlatform)] = &[
        ("reddit.com", SocialPlatform::Reddit),
        ("redd.it", SocialPlatform::Reddit),
        ("tiktok.com", SocialPlatform::TikTok),
        ("instagram.com", SocialPlatform::Instagram),
        ("instagr.am", SocialPlatform::Instagram),
        ("facebook.com", SocialPlatform::Facebook),
        ("fb.me", SocialPlatform::Facebook),
        ("fb.com", SocialPlatform::Facebook),
        ("fb.watch", SocialPlatform::Facebook),
        ("youtube.com", SocialPlatform::YouTube),
        ("youtube-nocookie.com", SocialPlatform::YouTube),
        ("youtu.be", SocialPlatform::YouTube),
        ("twitter.com", SocialPlatform::Twitter),
        ("x.com", SocialPlatform::Twitter),
        ("t.co", SocialPlatform::Twitter),
        ("linkedin.com", SocialPlatform::LinkedIn),
        ("lnkd.in", SocialPlatform::LinkedIn),
        ("vimeo.com", SocialPlatform::Vimeo),
        ("pinterest.com", SocialPlatform::Pinterest),
        ("pin.it", SocialPlatform::Pinterest),
    ];
    DOMAINS
        .iter()
        .find(|(domain, _)| host_matches(host, domain))
        .map(|(_, platform)| *platform)
        // Pinterest's country hosts, each verified to redirect to
        // `<country>.pinterest.com`. A `pinterest` label alone proves nothing:
        // `pinterest.io` and `pinterest.evil.com` aren't Pinterest.
        .or_else(|| {
            const COUNTRY_SUFFIXES: [&str; 28] = [
                "at", "be", "ca", "ch", "cl", "co.in", "co.kr", "co.nz", "co.uk", "com.au",
                "com.mx", "de", "dk", "es", "fr", "id", "ie", "in", "it", "jp", "kr", "mx", "nl",
                "nz", "ph", "pt", "ru", "se",
            ];
            let suffix = host
                .strip_prefix("pinterest.")
                .or_else(|| host.split_once(".pinterest.").map(|(_, suffix)| suffix))?;
            COUNTRY_SUFFIXES
                .contains(&suffix)
                .then_some(SocialPlatform::Pinterest)
        })
}

/// The short name of a schema.org type: a full IRI
/// (`https://schema.org/Recipe`), a CURIE (`schema:Recipe`), and a bare term
/// all yield `Recipe`.
pub(crate) fn short_schema_type(declared: &str) -> Option<String> {
    declared
        .rsplit([':', '/', '#'])
        .find(|segment| !segment.is_empty())
        .map(str::to_string)
}

/// The schema.org types an entity declares in `@type`, as
/// [`short_schema_type`] names, whether `@type` is one string or an array.
pub(crate) fn schema_types(entity: &serde_json::Value) -> Vec<String> {
    match entity.get("@type") {
        Some(serde_json::Value::String(declared)) => {
            short_schema_type(declared).into_iter().collect()
        }
        Some(serde_json::Value::Array(declared)) => declared
            .iter()
            .filter_map(serde_json::Value::as_str)
            .filter_map(short_schema_type)
            .collect(),
        _ => Vec::new(),
    }
}

/// The metadata keys an embed is scraped into (`scrape_embed`) and read back
/// from (`extract_preview`).
pub(crate) mod embed_keys {
    pub(crate) const OEMBED_TITLE: &str = "oembed:title";
    pub(crate) const OEMBED_AUTHOR_NAME: &str = "oembed:author_name";
    pub(crate) const OEMBED_THUMBNAIL_URL: &str = "oembed:thumbnail_url";
    pub(crate) const REDDIT_TITLE: &str = "reddit:title";
    pub(crate) const REDDIT_TEXT: &str = "reddit:text";
    pub(crate) const REDDIT_AUTHOR: &str = "reddit:author";
    pub(crate) const REDDIT_IMAGE: &str = "reddit:image";
    pub(crate) const REDDIT_VIDEO: &str = "reddit:video";
    /// A link post's destination: the page the post is about.
    pub(crate) const REDDIT_URL: &str = "reddit:url";
    /// The prefixes only an embed may use, so a page's own `<meta>` tags can't
    /// pose as one.
    pub(crate) const PREFIXES: [&str; 2] = ["oembed:", "reddit:"];
}

/// Nodes nested deeper than this are not a page. The walkers that read a
/// document recurse once per level, so the bound and [`READ_STACK_BYTES`]
/// together keep them from overflowing the stack, which would abort the
/// process.
const MAX_DOM_DEPTH: usize = 512;

/// The stack a document is read on: enough for every walker at
/// [`MAX_DOM_DEPTH`] in a debug build, whatever stack size the caller's
/// runtime gives its blocking threads.
const READ_STACK_BYTES: usize = 32 * 1024 * 1024;

/// Markup fed to the tokenizer at a time, so a page is read only as far as
/// the sink needs.
pub(crate) const TOKENIZE_CHUNK_BYTES: usize = 4 * 1024;

/// Run `sink` over `html`'s tokens, a piece at a time, until `is_done` or the
/// end; the sink after. The parser drops one byte order mark, at the start;
/// the tokenizer's own option would drop one at the start of every piece.
pub(crate) fn tokenize_until<Sink: TokenSink>(
    html: &str,
    sink: Sink,
    is_done: impl Fn(&Sink) -> bool,
) -> Sink {
    let options = TokenizerOpts {
        discard_bom: false,
        ..TokenizerOpts::default()
    };
    let mut tokenizer = Tokenizer::new(sink, options);
    let mut queue = BufferQueue::default();
    let mut rest = html.strip_prefix('\u{feff}').unwrap_or(html);
    while !rest.is_empty() && !is_done(&tokenizer.sink) {
        let mut end = TOKENIZE_CHUNK_BYTES.min(rest.len());
        while !rest.is_char_boundary(end) {
            end += 1;
        }
        let (piece, tail) = rest.split_at(end);
        queue.push_back(StrTendril::from(piece));
        let _ = tokenizer.feed(&mut queue);
        rest = tail;
    }
    tokenizer.end();
    tokenizer.sink
}

/// Tree operations the parser may make per byte of markup, over
/// [`TREE_OPERATIONS_FLOOR`]. A page's tree grows in step with its markup, an
/// element or a text node per tag or run of text. One that leaves dozens of
/// formatting tags open and then starts paragraph after paragraph makes the
/// parser rebuild every one of them in each: twenty operations a byte, a tree
/// of ten nodes a byte, and no depth to show for it. Such a page is built to
/// exhaust the parser's memory, and its parse stops.
const TREE_OPERATIONS_PER_BYTE: usize = 4;

/// The tree operations every document may make, however small its markup.
const TREE_OPERATIONS_FLOOR: usize = 16 * 1024;

/// A tree sink that builds a [`scraper::Html`] and watches the parser build
/// it: how deep each node is placed, read off the tree it stands in, and how
/// many operations the parser makes. A parse past [`MAX_DOM_DEPTH`] or the
/// operations budget stops: the parser's cost grows with the square of the
/// depth, and the tree it builds is the one measure of depth it can't disagree
/// with.
struct DepthBounded {
    sink: scraper::Html,
    /// Each node's depth, once read off the tree. Cleared whenever the parser
    /// moves a node that already stands in the tree, which only misnested
    /// markup makes it do; a page that nests as written pays one walk a node.
    depth: HashMap<NodeId, usize>,
    operations: usize,
    too_deep: bool,
}

impl DepthBounded {
    fn new(sink: scraper::Html) -> Self {
        Self {
            sink,
            depth: HashMap::new(),
            operations: 0,
            too_deep: false,
        }
    }

    /// How deep `node` stands in the tree, the root at 1, as [`is_too_deep`]
    /// counts it.
    fn depth_of(&mut self, node: &NodeId) -> usize {
        if let Some(depth) = self.depth.get(node) {
            return *depth;
        }
        let depth = self
            .sink
            .tree
            .get(*node)
            .map_or(0, |node| node.ancestors().count() + 1);
        self.depth.insert(*node, depth);
        depth
    }

    /// Forget every depth before `child`, a node that may already stand in the
    /// tree, is placed: its subtree's depths change with it.
    fn moving(&mut self, child: &NodeOrText<NodeId>) {
        if let NodeOrText::AppendNode(node) = child {
            let stands_in_tree = self
                .sink
                .tree
                .get(*node)
                .is_some_and(|node| node.parent().is_some());
            if stands_in_tree {
                // A fresh map: clearing one keeps its capacity and sweeps it all.
                self.depth = HashMap::new();
            }
        }
    }

    /// Note `child` about to stand at `depth`, and remember it there.
    fn place_at(&mut self, child: &NodeOrText<NodeId>, depth: usize) {
        if let NodeOrText::AppendNode(node) = child {
            self.depth.insert(*node, depth);
        }
        if depth > MAX_DOM_DEPTH {
            self.too_deep = true;
        }
    }

    /// Whether the parser has made more operations than the markup fed so far
    /// accounts for.
    fn over_budget(&self, bytes_fed: usize) -> bool {
        self.operations > bytes_fed * TREE_OPERATIONS_PER_BYTE + TREE_OPERATIONS_FLOOR
    }
}

impl TreeSink for DepthBounded {
    type Handle = NodeId;
    type Output = scraper::Html;

    fn finish(self) -> scraper::Html {
        self.sink.finish()
    }

    fn parse_error(&mut self, message: Cow<'static, str>) {
        self.sink.parse_error(message)
    }

    fn get_document(&mut self) -> NodeId {
        self.sink.get_document()
    }

    fn elem_name<'a>(&'a self, target: &'a NodeId) -> ExpandedName<'a> {
        self.sink.elem_name(target)
    }

    fn create_element(
        &mut self,
        name: QualName,
        attributes: Vec<Attribute>,
        flags: ElementFlags,
    ) -> NodeId {
        self.operations += 1;
        self.sink.create_element(name, attributes, flags)
    }

    fn create_comment(&mut self, text: StrTendril) -> NodeId {
        self.operations += 1;
        self.sink.create_comment(text)
    }

    fn create_pi(&mut self, target: StrTendril, data: StrTendril) -> NodeId {
        self.operations += 1;
        self.sink.create_pi(target, data)
    }

    fn append(&mut self, parent: &NodeId, child: NodeOrText<NodeId>) {
        self.operations += 1;
        self.moving(&child);
        let depth = self.depth_of(parent) + 1;
        self.place_at(&child, depth);
        self.sink.append(parent, child)
    }

    fn append_based_on_parent_node(
        &mut self,
        element: &NodeId,
        previous_element: &NodeId,
        child: NodeOrText<NodeId>,
    ) {
        self.operations += 1;
        self.moving(&child);
        // Beside `element` when it has a parent, else under `previous_element`.
        let has_parent = self
            .sink
            .tree
            .get(*element)
            .is_some_and(|element| element.parent().is_some());
        let depth = if has_parent {
            self.depth_of(element)
        } else {
            self.depth_of(previous_element) + 1
        };
        self.place_at(&child, depth);
        self.sink
            .append_based_on_parent_node(element, previous_element, child)
    }

    fn append_doctype_to_document(
        &mut self,
        name: StrTendril,
        public_id: StrTendril,
        system_id: StrTendril,
    ) {
        self.operations += 1;
        self.sink
            .append_doctype_to_document(name, public_id, system_id)
    }

    fn mark_script_already_started(&mut self, node: &NodeId) {
        self.sink.mark_script_already_started(node)
    }

    fn pop(&mut self, node: &NodeId) {
        self.sink.pop(node)
    }

    fn get_template_contents(&mut self, target: &NodeId) -> NodeId {
        self.sink.get_template_contents(target)
    }

    fn same_node(&self, x: &NodeId, y: &NodeId) -> bool {
        self.sink.same_node(x, y)
    }

    fn set_quirks_mode(&mut self, mode: QuirksMode) {
        self.sink.set_quirks_mode(mode)
    }

    fn append_before_sibling(&mut self, sibling: &NodeId, new_node: NodeOrText<NodeId>) {
        self.operations += 1;
        self.moving(&new_node);
        let depth = self.depth_of(sibling);
        self.place_at(&new_node, depth);
        self.sink.append_before_sibling(sibling, new_node)
    }

    fn add_attrs_if_missing(&mut self, target: &NodeId, attributes: Vec<Attribute>) {
        self.operations += 1;
        self.sink.add_attrs_if_missing(target, attributes)
    }

    fn associate_with_form(
        &mut self,
        target: &NodeId,
        form: &NodeId,
        nodes: (&NodeId, Option<&NodeId>),
    ) {
        self.sink.associate_with_form(target, form, nodes)
    }

    fn remove_from_parent(&mut self, target: &NodeId) {
        self.operations += 1;
        // A fresh map: clearing one keeps its capacity and sweeps it all.
        self.depth = HashMap::new();
        self.sink.remove_from_parent(target)
    }

    fn reparent_children(&mut self, node: &NodeId, new_parent: &NodeId) {
        self.operations += 1;
        // A fresh map: clearing one keeps its capacity and sweeps it all.
        self.depth = HashMap::new();
        self.sink.reparent_children(node, new_parent)
    }

    fn is_mathml_annotation_xml_integration_point(&self, handle: &NodeId) -> bool {
        self.sink.is_mathml_annotation_xml_integration_point(handle)
    }
}

/// The parser's options for a bounded parse: the one leading byte order mark
/// is stripped here, since the tokenizer's option would drop one at the start
/// of every piece fed to it.
fn bounded_parse_options() -> ParseOpts {
    ParseOpts {
        tokenizer: TokenizerOpts {
            discard_bom: false,
            ..TokenizerOpts::default()
        },
        ..ParseOpts::default()
    }
}

/// Feed `html` to `parser` a piece at a time, and stop with `None` as soon as
/// the tree it builds nests deeper than [`MAX_DOM_DEPTH`] or the parser works
/// past its operations budget. The finished tree is measured once more: the
/// parser can move a subtree deeper after placing it.
fn parse_bounded(mut parser: Parser<DepthBounded>, html: &str) -> Option<scraper::Html> {
    let html = html.strip_prefix('\u{feff}').unwrap_or(html);
    let mut fed = 0;
    while fed < html.len() {
        let mut end = (fed + TOKENIZE_CHUNK_BYTES).min(html.len());
        while !html.is_char_boundary(end) {
            end += 1;
        }
        parser.process(StrTendril::from(&html[fed..end]));
        fed = end;
        let sink = &parser.tokenizer.sink.sink;
        if sink.too_deep || sink.over_budget(fed) {
            return None;
        }
    }
    let document = parser.finish();
    (!is_too_deep(&document)).then_some(document)
}

/// Parse `html` as a document, or `None` when it nests deeper than
/// [`MAX_DOM_DEPTH`] or makes the parser work past its budget, found as soon
/// as it does rather than after a parse the depth makes slow.
pub(crate) fn parse_document_bounded(html: &str) -> Option<scraper::Html> {
    let parser = html5ever::parse_document(
        DepthBounded::new(scraper::Html::new_document()),
        bounded_parse_options(),
    );
    parse_bounded(parser, html)
}

/// Parse `html` as a fragment standing inside a `context` element, or `None`
/// on what refuses a document.
pub(crate) fn parse_fragment_bounded(html: &str, context: &str) -> Option<scraper::Html> {
    let parser = html5ever::parse_fragment(
        DepthBounded::new(scraper::Html::new_fragment()),
        bounded_parse_options(),
        QualName::new(None, ns!(html), LocalName::from(context)),
        Vec::new(),
    );
    parse_bounded(parser, html)
}

fn is_too_deep(document: &scraper::Html) -> bool {
    let mut depth = 0usize;
    document.tree.root().traverse().any(|edge| {
        match edge {
            ego_tree::iter::Edge::Open(_) => depth += 1,
            ego_tree::iter::Edge::Close(_) => depth -= 1,
        }
        depth > MAX_DOM_DEPTH
    })
}

/// Parse `html` and read it with `read`, off the async runtime: building a DOM
/// is CPU-bound, and a parsed document can't cross an `await`. A document
/// [`parse_document_bounded`] refuses is read as an empty one.
pub(crate) async fn with_document<T: Send + 'static>(
    html: &Html,
    read: impl FnOnce(&scraper::Html) -> T + Send + 'static,
) -> T {
    let html = html.to_string();
    tokio::task::spawn_blocking(move || {
        std::thread::scope(|scope| {
            std::thread::Builder::new()
                .stack_size(READ_STACK_BYTES)
                .spawn_scoped(scope, move || {
                    let Some(document) = parse_document_bounded(&html) else {
                        log::warn!("document too deep or too costly to parse read as empty");
                        return read(&scraper::Html::parse_document(""));
                    };
                    read(&document)
                })
                .expect("spawning the document reader")
                .join()
                .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
        })
    })
    .await
    .expect("reading a parsed document panicked")
}

/// How long a top-level domain is: `.io` to `.international` and beyond. An
/// email is found (`extract`) and kept (`normalize`) under the same bound.
pub(crate) const TLD_LEN: std::ops::RangeInclusive<usize> = 2..=24;

/// A phone number's extension, as a regex: ` ext. 12`, `, extension 12`,
/// `extn 12`, `x12`, `x-12`, `ext: 12`, `ext #12`, `(x12)`. A number is read
/// (`extract`) and stripped of it (`normalize`) by the same rule.
pub(crate) const PHONE_EXTENSION: &str = r"[\s,(]*(?:ext(?:ension|n)?\.?|x\.?)[\s:#.\-]*[0-9]+\)?";
