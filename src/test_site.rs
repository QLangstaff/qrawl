//! A local HTTP site for tests: serves the pages a closure returns and counts
//! the requests it gets.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// What the site answers for a path.
pub(crate) enum Reply {
    Html(String),
    Redirect(String),
    NotFound,
}

pub(crate) struct TestSite {
    addr: SocketAddr,
    hits: Arc<AtomicUsize>,
}

impl TestSite {
    /// Serve `reply(path_and_query)` on a free local port.
    pub(crate) async fn serve(reply: impl Fn(&str) -> Reply + Send + Sync + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local addr");
        let hits = Arc::new(AtomicUsize::new(0));
        let reply = Arc::new(reply);
        tokio::spawn({
            let hits = hits.clone();
            async move {
                while let Ok((mut socket, _)) = listener.accept().await {
                    let (hits, reply) = (hits.clone(), reply.clone());
                    tokio::spawn(async move {
                        let mut request = Vec::new();
                        let mut chunk = [0u8; 1024];
                        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                            match socket.read(&mut chunk).await {
                                Ok(read) if read > 0 => request.extend_from_slice(&chunk[..read]),
                                _ => return,
                            }
                        }
                        hits.fetch_add(1, Ordering::SeqCst);
                        let request = String::from_utf8_lossy(&request);
                        let path = request.split_whitespace().nth(1).unwrap_or("/");
                        let response = match reply(path) {
                            Reply::Html(body) => format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                body.len(),
                                body
                            ),
                            Reply::Redirect(location) => format!(
                                "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            ),
                            Reply::NotFound => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(),
                        };
                        let _ = socket.write_all(response.as_bytes()).await;
                    });
                }
            }
        });
        Self { addr, hits }
    }

    pub(crate) fn url(&self, path: &str) -> String {
        format!("http://{}{}", self.addr, path)
    }

    pub(crate) fn port(&self) -> u16 {
        self.addr.port()
    }

    pub(crate) fn hits(&self) -> usize {
        self.hits.load(Ordering::SeqCst)
    }
}

/// A page long enough to pass fetch validation, listing `links` the way
/// `map_children` recognizes children: repeated sibling articles.
pub(crate) fn listing_page(title: &str, head: &str, links: &[String]) -> String {
    let articles: String = links
        .iter()
        .map(|link| {
            format!(
                "<article><h2><a href=\"{link}\">Item {link}</a></h2><p>{}</p></article>",
                "Lorem ipsum dolor sit amet. ".repeat(4)
            )
        })
        .collect();
    format!(
        "<!DOCTYPE html><html><head><title>{title}</title>{head}</head><body><main>{articles}<p>{}</p></main></body></html>",
        "Lorem ipsum dolor sit amet, consectetur adipiscing elit. ".repeat(12)
    )
}
