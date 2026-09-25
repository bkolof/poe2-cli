//! The pathofexile.com trade API for PoE2: searches and listing fetches,
//! within the site's rate limits, with the user's session when there is one.

pub mod limits;
pub mod session;

use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;
use ureq::http::Response;

use limits::{Limits, Policy};

const SITE: &str = "https://www.pathofexile.com";
/// The trade site returns at most this many listings per fetch.
const FETCH_BATCH: usize = 10;

#[derive(Debug, Deserialize)]
pub struct SearchResult {
    /// The search's id, which also forms its trade site URL.
    pub id: String,
    /// Listing ids in the search's sort order.
    pub result: Vec<String>,
    pub total: Option<u64>,
}

pub struct Client {
    agent: ureq::Agent,
    session: Option<String>,
}

impl Client {
    /// A client with the stored session, if the user logged in.
    pub fn new() -> Result<Self> {
        Ok(Self::with_session(session::load()?))
    }

    pub fn with_session(session: Option<String>) -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            .max_redirects(0)
            .user_agent(concat!("poe2-cli/", env!("CARGO_PKG_VERSION")))
            .build()
            .into();
        Self { agent, session }
    }

    pub fn has_session(&self) -> bool {
        self.session.is_some()
    }

    /// Whether pathofexile.com accepts the session. Uses the account page,
    /// which does not count against the trade limits.
    pub fn session_valid(&self) -> Result<bool> {
        let Some(session) = &self.session else {
            return Ok(false);
        };
        let response = self
            .agent
            .get(format!("{SITE}/my-account"))
            .header("Cookie", format!("POESESSID={session}"))
            .call()?;
        Ok(response.status() == 200)
    }

    pub fn search(&self, league: &str, query: &serde_json::Value) -> Result<SearchResult> {
        let url = format!("{SITE}/api/trade2/search/poe2/{}", encode(league));
        let mut request = self.agent.post(&url);

        if let Some(session) = &self.session {
            request = request.header("Cookie", format!("POESESSID={session}"));
        }

        let response = self.limited(Policy::Search, || Ok(request.send_json(query)?))?;
        read_json(response)
    }

    /// The raw listing JSON for the given listing ids, in batches of ten.
    pub fn fetch(&self, search_id: &str, ids: &[String]) -> Result<Vec<String>> {
        let mut bodies = Vec::new();

        for batch in ids.chunks(FETCH_BATCH) {
            let url = format!(
                "{SITE}/api/trade2/fetch/{}?query={search_id}",
                batch.join(",")
            );
            let mut request = self.agent.get(&url);

            if let Some(session) = &self.session {
                request = request.header("Cookie", format!("POESESSID={session}"));
            }

            let response = self.limited(Policy::Fetch, || Ok(request.call()?))?;
            bodies.push(read_body(response)?);
        }

        Ok(bodies)
    }

    /// Run a request inside the rate limit and learn the limits it reports.
    fn limited(
        &self,
        policy: Policy,
        send: impl FnOnce() -> Result<Response<ureq::Body>>,
    ) -> Result<Response<ureq::Body>> {
        let mut limits = Limits::load()?;
        limits.acquire(policy)?;
        let response = send()?;

        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        };
        // A response names its rule sets (e.g. `Ip`, `Account`); each has its own headers.
        let names = header("X-Rate-Limit-Rules").unwrap_or_default();
        let joined = |suffix: &str| {
            let values: Vec<String> = names
                .split(',')
                .filter_map(|name| header(&format!("X-Rate-Limit-{}{suffix}", name.trim())))
                .collect();
            (!values.is_empty()).then(|| values.join(","))
        };
        limits.update(
            policy,
            joined("").as_deref(),
            joined("-State").as_deref(),
            header("Retry-After").as_deref(),
        );
        limits.save()?;

        if response.status() == 429 {
            bail!("the trade site rate-limited this IP; poe2 will wait it out on the next run");
        }

        Ok(response)
    }
}

/// The trade site page for a search, where the listings can be bought.
pub fn search_url(league: &str, search_id: &str) -> String {
    format!("{SITE}/trade2/search/poe2/{}/{search_id}", encode(league))
}

fn read_json<T: serde::de::DeserializeOwned>(response: Response<ureq::Body>) -> Result<T> {
    let body = read_body(response)?;
    serde_json::from_str(&body).context("unexpected response from the trade site")
}

/// The body of a successful response; the site's own message otherwise.
fn read_body(mut response: Response<ureq::Body>) -> Result<String> {
    let status = response.status();
    let body = response.body_mut().read_to_string()?;

    if status.is_success() {
        return Ok(body);
    }

    #[derive(Deserialize)]
    struct Error {
        error: Message,
    }

    #[derive(Deserialize)]
    struct Message {
        message: String,
    }

    let message = serde_json::from_str::<Error>(&body)
        .map(|e| e.error.message)
        .unwrap_or_else(|_| format!("HTTP {status}"));

    if status == 401 || status == 403 {
        return Err(anyhow!(
            "the trade site refused the request ({message}); if you logged in, the session may have expired: run `poe2 trade login`"
        ));
    }

    Err(anyhow!("the trade site refused the search: {message}"))
}

fn encode(text: &str) -> String {
    text.replace(' ', "%20")
}
