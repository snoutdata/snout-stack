//! What the front door decides, with no socket in sight: which service a path belongs to, whether
//! a request may pass, and what the service behind it is told. Kept pure so every rule here is a
//! test.
//!
//! The rules are the hosted front door's, for one project that is always awake: the same five
//! path prefixes a client library has as string literals, the same paths open without a key, the
//! same header rewrites. What the hosted door does and this one does not is the part that only
//! exists because there are many projects and they sleep: resolving a project from the host name,
//! waking a paused one, and the plan gate.

use std::collections::BTreeMap;

use crate::keys;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Service {
	Rest,
	Graphql,
	Auth,
	Storage,
	Realtime,
	Functions,
}

impl Service {
	/// The shared servers are multi-tenant, so they are told which project a request is for.
	pub fn is_shared(self) -> bool {
		matches!(
			self,
			Service::Storage | Service::Realtime | Service::Functions
		)
	}

	pub fn name(self) -> &'static str {
		match self {
			Service::Rest => "rest",
			Service::Graphql => "graphql",
			Service::Auth => "auth",
			Service::Storage => "storage",
			Service::Realtime => "realtime",
			Service::Functions => "functions",
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
	pub service: Service,
	/// The path the service sees: the prefix stripped and rebased, the query kept.
	pub upstream_path: String,
	/// Headers the prefix means whatever the client sent; applied last, so they win.
	pub fixed_headers: &'static [(&'static str, &'static str)],
}

struct Prefix {
	prefix: &'static str,
	service: Service,
	base: &'static str,
	headers: &'static [(&'static str, &'static str)],
}

/// Longest first, so a prefix is never shadowed by a shorter neighbour.
const PREFIXES: &[Prefix] = &[
	Prefix {
		prefix: "/rest/v1",
		service: Service::Rest,
		base: "",
		headers: &[],
	},
	// Not a process: the GraphQL resolver is a Postgres function the data API calls, in the
	// schema this header names.
	Prefix {
		prefix: "/graphql/v1",
		service: Service::Graphql,
		base: "/rpc/graphql",
		headers: &[("content-profile", "graphql_public")],
	},
	Prefix {
		prefix: "/auth/v1",
		service: Service::Auth,
		base: "",
		headers: &[],
	},
	// The prefix is told back to storage so a resumable upload's `Location` is reachable.
	Prefix {
		prefix: "/storage/v1",
		service: Service::Storage,
		base: "",
		headers: &[("x-forwarded-prefix", "/storage/v1")],
	},
	// Broadcast over HTTP. Exactly this path under `/api`: the tenant API beside it is the
	// server's admin surface and is never reachable from outside.
	Prefix {
		prefix: "/realtime/v1/api/broadcast",
		service: Service::Realtime,
		base: "/api/broadcast",
		headers: &[],
	},
	Prefix {
		prefix: "/realtime/v1",
		service: Service::Realtime,
		base: "/socket",
		headers: &[],
	},
	Prefix {
		prefix: "/functions/v1",
		service: Service::Functions,
		base: "",
		headers: &[],
	},
];

pub fn service_for_path(path_and_query: &str) -> Option<Match> {
	let (path, query) = match path_and_query.split_once('?') {
		Some((path, query)) => (path, Some(query)),
		None => (path_and_query, None),
	};
	for entry in PREFIXES {
		let Some(rest) = path.strip_prefix(entry.prefix) else {
			continue;
		};
		if !(rest.is_empty() || rest.starts_with('/')) {
			continue;
		}
		let mut upstream = format!("{}{rest}", entry.base);
		if upstream.is_empty() {
			upstream.push('/');
		}
		if let Some(query) = query {
			upstream.push('?');
			upstream.push_str(query);
		}
		return Some(Match {
			service: entry.service,
			upstream_path: upstream,
			fixed_headers: entry.headers,
		});
	}
	None
}

/// Paths a browser reaches with no header of its own: the sign-in flow's redirects (links from a
/// mail client, an identity provider sending the browser back) and a presigned download, whose URL
/// is the credential. Signing in, signing up and recovering are NOT here: every client sends the
/// key with them, and with them open a rotated key still signed people in.
const OPEN_AUTH_PATHS: &[&str] = &[
	"/auth/v1/verify",
	"/auth/v1/authorize",
	"/auth/v1/callback",
	"/auth/v1/sso/saml/acs",
	"/auth/v1/sso/saml/metadata",
];

const OPEN_STORAGE_READS: &[&str] = &[
	"/storage/v1/object/public/",
	"/storage/v1/object/sign/",
	"/storage/v1/render/image/public/",
	"/storage/v1/render/image/sign/",
];

fn path_only(path_and_query: &str) -> &str {
	path_and_query.split('?').next().unwrap_or("")
}

fn under(path: &str, prefix: &str) -> bool {
	path == prefix
		|| path
			.strip_prefix(prefix)
			.is_some_and(|rest| rest.starts_with('/'))
}

pub fn is_open_path(method: &str, path_and_query: &str) -> bool {
	let path = path_only(path_and_query);
	if OPEN_AUTH_PATHS.iter().any(|open| under(path, open)) {
		return true;
	}
	// Minting a signed URL (POST) is authorised; redeeming one (GET) is not.
	let reading = method.eq_ignore_ascii_case("GET") || method.eq_ignore_ascii_case("HEAD");
	reading && OPEN_STORAGE_READS.iter().any(|open| path.starts_with(open))
}

/// The function a functions path names, by the runtime's own rule: letters, digits, hyphens and
/// underscores, so the name the door decides on is the name the runtime runs.
pub fn function_name(upstream_path: &str) -> Option<&str> {
	let first = path_only(upstream_path)
		.trim_start_matches('/')
		.split('/')
		.next()
		.unwrap_or("");
	let lead = first.bytes().next()?;
	let ok = lead.is_ascii_alphanumeric()
		&& first.len() <= 48
		&& first
			.bytes()
			.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
	ok.then_some(first)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
	Anon,
	ServiceRole,
}

/// The project's two keys, as hashes.
pub struct Keys {
	pub anon_hash: String,
	pub service_hash: String,
}

impl Keys {
	pub fn new(anon: &str, service: &str) -> Self {
		Keys {
			anon_hash: keys::key_hash(anon),
			service_hash: keys::key_hash(service),
		}
	}

	pub fn classify(&self, presented: Option<&str>) -> Option<Role> {
		let hash = keys::key_hash(presented?);
		if keys::constant_time_eq(&hash, &self.service_hash) {
			Some(Role::ServiceRole)
		} else if keys::constant_time_eq(&hash, &self.anon_hash) {
			Some(Role::Anon)
		} else {
			None
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
	pub status: u16,
	pub message: &'static str,
	pub hint: Option<&'static str>,
	pub code: &'static str,
}

impl Refusal {
	/// The body the door writes: PostgREST's field names, always a JSON object.
	pub fn body(&self) -> String {
		let mut body = serde_json::Map::new();
		body.insert("message".into(), self.message.into());
		if let Some(hint) = self.hint {
			body.insert("hint".into(), hint.into());
		}
		body.insert("code".into(), self.code.into());
		serde_json::Value::Object(body).to_string()
	}
}

pub const NOT_FOUND: Refusal = Refusal {
	status: 404,
	message: "There is nothing at this path.",
	hint: None,
	code: "not_found",
};

/// May this request pass? `open_functions` are the functions deployed to be called with no key
/// (a webhook's receiver).
pub fn authorise(
	method: &str,
	path_and_query: &str,
	matched: &Match,
	role: Option<Role>,
	open_functions: &[String],
) -> Result<Option<Role>, Refusal> {
	let open_function = matched.service == Service::Functions
		&& function_name(&matched.upstream_path)
			.is_some_and(|name| open_functions.iter().any(|open| open == name));
	if role.is_none() && !(is_open_path(method, path_and_query) || open_function) {
		// One sentence for "no key" and "a key we do not know", so a stranger cannot tell which.
		return Err(Refusal {
			status: 401,
			message: "No API key found in request, or the key is not one of this project's.",
			hint: Some("Send the project's anon or service_role key as the `apikey` header."),
			code: "no_api_key",
		});
	}
	Ok(role)
}

/// Hop-by-hop headers belong to one connection. `upgrade` is kept for a websocket.
const HOP_BY_HOP: &[&str] = &[
	"connection",
	"keep-alive",
	"proxy-authenticate",
	"proxy-authorization",
	"te",
	"trailer",
	"transfer-encoding",
];

/// Response headers that name the software rather than say what happened.
const HIDDEN_RESPONSE_HEADERS: &[&str] = &[
	"server",
	"x-powered-by",
	"via",
	"x-runtime",
	"x-version",
	"x-application-version",
];

pub const FUNCTIONS_DOOR_HEADER: &str = "x-snoutdata-door";
pub const REF_HEADER: &str = "x-snoutdata-ref";
/// The caller's address, said by the door and never by the client: the auth server keys its
/// per-caller rate limits by it.
pub const CLIENT_HEADER: &str = "x-snoutdata-client";

pub struct Prepare<'a> {
	pub reference: &'a str,
	/// The name the shared servers are told: `<ref>.<domain>`.
	pub domain: &'a str,
	/// The caller's address.
	pub source: &'a str,
	/// `http` or `https`, as the public address says.
	pub scheme: &'a str,
	pub upgrade: bool,
	pub functions_door: Option<&'a str>,
}

/// The request's headers as the service behind the door should see them. Names are lowercase.
/// Returns the headers and the key as presented.
pub fn prepare_headers(
	incoming: &[(String, String)],
	path_and_query: &str,
	matched: &Match,
	options: &Prepare<'_>,
) -> (BTreeMap<String, String>, Option<String>) {
	let mut headers: BTreeMap<String, String> = BTreeMap::new();
	for (name, value) in incoming {
		let key = name.to_ascii_lowercase();
		if HOP_BY_HOP.contains(&key.as_str()) || (key == "upgrade" && !options.upgrade) {
			continue;
		}
		headers
			.entry(key)
			.and_modify(|held| {
				held.push_str(", ");
				held.push_str(value);
			})
			.or_insert_with(|| value.clone());
	}

	// The key from the query, for the realtime socket and an `<img>`, which cannot send headers.
	let api_key = headers
		.get("apikey")
		.cloned()
		.or_else(|| query_api_key(path_and_query));
	if let Some(key) = &api_key {
		headers.insert("apikey".into(), key.clone());
		headers.insert("x-api-key".into(), key.clone());
		// The load-bearing rewrite: the services choose a Postgres role from a bearer token, and a
		// signed-out client sends only `apikey`. A signed-in user's own token outranks the key.
		let has_bearer = headers.get("authorization").is_some_and(|value| {
			let value = value.trim_start();
			value.len() > 7
				&& value[..7].eq_ignore_ascii_case("bearer ")
				&& !value[7..].trim().is_empty()
		});
		if !has_bearer {
			headers.insert("authorization".into(), format!("Bearer {key}"));
		}
	}

	if options.upgrade {
		headers.insert("connection".into(), "Upgrade".into());
	}

	headers.insert(REF_HEADER.into(), options.reference.into());
	headers.insert("x-forwarded-proto".into(), options.scheme.into());
	let forwarded_for = match headers.get("x-forwarded-for") {
		Some(held) => format!("{held}, {}", options.source),
		None => options.source.to_owned(),
	};
	headers.insert("x-forwarded-for".into(), forwarded_for);
	headers.insert(CLIENT_HEADER.into(), options.source.into());
	if let Some(host) = headers.get("host").cloned() {
		headers.insert("x-forwarded-host".into(), host);
	}
	// The auth server would build mail links against a forwarded host; its own public address is
	// the right one, so the header is not passed to it.
	if matched.service == Service::Auth {
		headers.remove("x-forwarded-host");
	}
	// A shared server is told whose request this is by the door, never by the client's `Host`.
	if matched.service.is_shared() {
		let canonical = format!("{}.{}", options.reference, options.domain);
		headers.insert("host".into(), canonical.clone());
		headers.insert("x-forwarded-host".into(), canonical);
	}
	for (name, value) in matched.fixed_headers {
		headers.insert((*name).into(), (*value).into());
	}
	// The proof a request came through the door, sent to the functions runtime only.
	headers.remove(FUNCTIONS_DOOR_HEADER);
	if matched.service == Service::Functions
		&& let Some(secret) = options.functions_door
	{
		headers.insert(FUNCTIONS_DOOR_HEADER.into(), secret.into());
	}
	(headers, api_key)
}

pub fn query_api_key(path_and_query: &str) -> Option<String> {
	let (_, query) = path_and_query.split_once('?')?;
	query.split('&').find_map(|pair| {
		let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
		(name == "apikey").then(|| {
			let spaced = value.replace('+', " ");
			percent_encoding::percent_decode_str(&spaced)
				.decode_utf8_lossy()
				.into_owned()
		})
	})
}

/// The path with `apikey` taken out of its query. The key may arrive in the query (a websocket
/// URL and an `<img src>` have nowhere else to put it) and becomes the header; it must not travel
/// on, because to the data API every unknown query parameter is a column filter, and the request
/// would fail as a malformed one. A websocket upgrade keeps it: the Realtime handshake reads it.
pub fn without_api_key(path_and_query: &str) -> String {
	let Some((path, query)) = path_and_query.split_once('?') else {
		return path_and_query.to_owned();
	};
	let kept: Vec<&str> = query
		.split('&')
		.filter(|pair| {
			!pair.is_empty()
				&& !pair
					.split('=')
					.next()
					.is_some_and(|name| name.eq_ignore_ascii_case("apikey"))
		})
		.collect();
	if kept.is_empty() {
		path.to_owned()
	} else {
		format!("{path}?{}", kept.join("&"))
	}
}

/// Whether a response header reaches the client: the ones that name the software do not.
pub fn visible_response_header(name: &str) -> Option<&str> {
	let lower = name.to_ascii_lowercase();
	(!HIDDEN_RESPONSE_HEADERS.contains(&lower.as_str())).then_some(name)
}

/// CORS on everything: a browser client's every call is cross-origin. The origin is echoed.
pub fn cors_headers(
	origin: Option<&str>,
	request_headers: Option<&str>,
) -> Vec<(&'static str, String)> {
	vec![
		("access-control-allow-origin", origin.unwrap_or("*").to_owned()),
		("access-control-allow-methods", "GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS".to_owned()),
		(
			"access-control-allow-headers",
			request_headers
				.unwrap_or("authorization, apikey, x-api-key, x-client-info, content-type, accept, accept-profile, content-profile, prefer, range, x-upsert")
				.to_owned(),
		),
		("access-control-expose-headers", "content-range, content-length, content-encoding, etag, x-total-count".to_owned()),
		("access-control-max-age", "86400".to_owned()),
		("vary", "origin, access-control-request-headers".to_owned()),
	]
}

#[cfg(test)]
mod tests {
	use super::*;

	fn matched(path: &str) -> Match {
		service_for_path(path).expect("routed")
	}

	#[test]
	fn the_five_prefixes_route_and_rebase() {
		assert_eq!(
			matched("/rest/v1/todos?select=*").upstream_path,
			"/todos?select=*"
		);
		assert_eq!(matched("/rest/v1").upstream_path, "/");
		let graphql = matched("/graphql/v1");
		assert_eq!(graphql.service, Service::Graphql);
		assert_eq!(graphql.upstream_path, "/rpc/graphql");
		assert_eq!(
			matched("/auth/v1/token?grant_type=password").upstream_path,
			"/token?grant_type=password"
		);
		assert_eq!(
			matched("/storage/v1/object/public/a/b.png").service,
			Service::Storage
		);
		assert_eq!(
			matched("/realtime/v1/websocket?vsn=1.0.0").upstream_path,
			"/socket/websocket?vsn=1.0.0"
		);
		assert_eq!(
			matched("/realtime/v1/api/broadcast").upstream_path,
			"/api/broadcast"
		);
		assert_eq!(matched("/functions/v1/hello/x").upstream_path, "/hello/x");
	}

	#[test]
	fn nothing_else_routes() {
		assert!(service_for_path("/").is_none());
		assert!(service_for_path("/restv1").is_none());
		assert!(service_for_path("/rest/v10").is_none());
		assert!(
			service_for_path("/pg/tables").is_none(),
			"the schema API is not served"
		);
		// The realtime tenant API is under /api, and only broadcast is routed there.
		assert_eq!(
			matched("/realtime/v1/api/tenants").upstream_path,
			"/socket/api/tenants"
		);
	}

	#[test]
	fn open_paths_are_the_sign_in_flow_and_presigned_reads() {
		assert!(!is_open_path("POST", "/auth/v1/token?grant_type=password"));
		assert!(!is_open_path("POST", "/auth/v1/signup"));
		assert!(!is_open_path("POST", "/auth/v1/recover"));
		assert!(is_open_path("GET", "/auth/v1/verify?token=x"));
		assert!(!is_open_path("GET", "/auth/v1/user"));
		assert!(!is_open_path("POST", "/auth/v1/tokenx"));
		assert!(is_open_path("GET", "/storage/v1/object/sign/b/k?token=t"));
		assert!(
			!is_open_path("POST", "/storage/v1/object/sign/b/k"),
			"minting a signed URL needs a key"
		);
		assert!(!is_open_path("GET", "/storage/v1/object/b/k"));
	}

	#[test]
	fn a_request_without_a_known_key_is_refused_unless_open() {
		let keys = Keys::new("anon-key", "service-key");
		assert_eq!(keys.classify(Some("anon-key")), Some(Role::Anon));
		assert_eq!(keys.classify(Some("service-key")), Some(Role::ServiceRole));
		assert_eq!(keys.classify(Some("other")), None);
		assert_eq!(keys.classify(None), None);

		let rest = matched("/rest/v1/todos");
		assert_eq!(
			authorise("GET", "/rest/v1/todos", &rest, None, &[])
				.unwrap_err()
				.status,
			401
		);
		assert!(authorise("GET", "/rest/v1/todos", &rest, Some(Role::Anon), &[]).is_ok());

		let hook = matched("/functions/v1/stripe-hook");
		assert!(
			authorise(
				"POST",
				"/functions/v1/stripe-hook",
				&hook,
				None,
				&["stripe-hook".into()]
			)
			.is_ok()
		);
		assert!(
			authorise(
				"POST",
				"/functions/v1/stripe-hook",
				&hook,
				None,
				&["other".into()]
			)
			.is_err()
		);
		let sneaky = matched("/functions/v1/stripe-hook.x/y");
		assert!(
			authorise(
				"POST",
				"/functions/v1/stripe-hook.x/y",
				&sneaky,
				None,
				&["stripe-hook".into()]
			)
			.is_err()
		);
	}

	fn prepare(incoming: &[(&str, &str)], path: &str, upgrade: bool) -> BTreeMap<String, String> {
		let incoming: Vec<(String, String)> = incoming
			.iter()
			.map(|(n, v)| ((*n).into(), (*v).into()))
			.collect();
		let options = Prepare {
			reference: "abcdefghjkmnp",
			domain: "stack.internal",
			source: "10.0.0.9",
			scheme: "https",
			upgrade,
			functions_door: Some("door"),
		};
		prepare_headers(&incoming, path, &matched(path), &options).0
	}

	#[test]
	fn the_key_becomes_a_bearer_only_when_there_is_none() {
		let headers = prepare(&[("apikey", "anon-key")], "/rest/v1/todos", false);
		assert_eq!(headers["authorization"], "Bearer anon-key");
		assert_eq!(headers["x-api-key"], "anon-key");
		let signed_in = prepare(
			&[("apikey", "anon-key"), ("Authorization", "Bearer user-jwt")],
			"/rest/v1/todos",
			false,
		);
		assert_eq!(signed_in["authorization"], "Bearer user-jwt");
		let query = prepare(
			&[],
			"/realtime/v1/websocket?apikey=anon%2Dkey&vsn=1.0.0",
			true,
		);
		assert_eq!(query["apikey"], "anon-key");
		assert_eq!(query["connection"], "Upgrade");
	}

	#[test]
	fn the_shared_servers_are_told_the_project_by_the_door() {
		let storage = prepare(
			&[("host", "evil.example.com"), ("apikey", "k")],
			"/storage/v1/bucket",
			false,
		);
		assert_eq!(storage["host"], "abcdefghjkmnp.stack.internal");
		assert_eq!(storage["x-forwarded-host"], "abcdefghjkmnp.stack.internal");
		assert_eq!(storage["x-forwarded-prefix"], "/storage/v1");
		let auth = prepare(
			&[
				("host", "api.example.com"),
				("apikey", "k"),
				("x-api-version", "2024-01-01"),
			],
			"/auth/v1/user",
			false,
		);
		assert!(!auth.contains_key("x-forwarded-host"));
		assert_eq!(auth["host"], "api.example.com");
		assert_eq!(
			auth["x-api-version"], "2024-01-01",
			"passed through as sent"
		);
	}

	#[test]
	fn the_door_secret_goes_to_functions_only_and_a_client_cannot_forge_it() {
		let functions = prepare(
			&[("x-snoutdata-door", "forged"), ("apikey", "k")],
			"/functions/v1/hello",
			false,
		);
		assert_eq!(functions["x-snoutdata-door"], "door");
		let rest = prepare(
			&[("x-snoutdata-door", "forged"), ("apikey", "k")],
			"/rest/v1/t",
			false,
		);
		assert!(!rest.contains_key("x-snoutdata-door"));
	}

	#[test]
	fn the_caller_address_is_the_doors_not_the_clients() {
		let headers = prepare(
			&[
				("x-forwarded-for", "1.2.3.4"),
				("x-snoutdata-client", "1.2.3.4"),
				("apikey", "k"),
			],
			"/auth/v1/token",
			false,
		);
		assert_eq!(headers["x-snoutdata-client"], "10.0.0.9");
		assert_eq!(headers["x-forwarded-for"], "1.2.3.4, 10.0.0.9");
	}

	#[test]
	fn graphql_names_its_schema_whatever_the_client_says() {
		let headers = prepare(
			&[("content-profile", "public"), ("apikey", "k")],
			"/graphql/v1",
			false,
		);
		assert_eq!(headers["content-profile"], "graphql_public");
	}

	#[test]
	fn the_key_never_travels_on_in_the_query() {
		assert_eq!(
			without_api_key("/compat_notes?select=id&apikey=k&limit=1"),
			"/compat_notes?select=id&limit=1"
		);
		assert_eq!(without_api_key("/t?APIKEY=k"), "/t");
		assert_eq!(without_api_key("/t?apikeys=1"), "/t?apikeys=1");
		assert_eq!(without_api_key("/t"), "/t");
	}

	#[test]
	fn response_headers_that_name_software_are_dropped() {
		assert_eq!(visible_response_header("Server"), None);
		assert_eq!(
			visible_response_header("x-api-version"),
			Some("x-api-version")
		);
		assert_eq!(
			visible_response_header("content-range"),
			Some("content-range")
		);
	}

	#[test]
	fn a_refusal_is_a_json_object() {
		let body: serde_json::Value = serde_json::from_str(&NOT_FOUND.body()).unwrap();
		assert_eq!(body["code"], "not_found");
	}
}
