//! The front door: one port, every service behind it.
//!
//! Every decision is `routes.rs`'s; this file is the sockets. A request is routed by its path,
//! refused here if it carries no key it may pass without, rewritten, and streamed to the service
//! and back without being buffered. A websocket upgrade is forwarded and the two upgraded
//! connections are joined.

use std::net::SocketAddr;
use std::sync::Arc;

use bytes::Bytes;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Empty, Full};
use hyper::body::Incoming;
use hyper::header::{HeaderName, HeaderValue};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::{TokioExecutor, TokioIo};
use tokio::net::TcpListener;

use crate::env;
use crate::routes::{self, Keys, Prepare, Refusal, Service};

type Body = BoxBody<Bytes, hyper::Error>;

/// The name the shared servers are told a request is for: `<ref>.stack.internal`. Nothing
/// resolves it; the servers read their tenant from its first label.
pub const DOMAIN: &str = "stack.internal";

struct Gateway {
	reference: String,
	keys: Keys,
	scheme: String,
	functions_door: Option<String>,
	open_functions: Vec<String>,
	client_address_header: Option<String>,
	upstreams: Upstreams,
	client: Client<HttpConnector, Incoming>,
}

struct Upstreams {
	rest: String,
	auth: String,
	storage: String,
	realtime: String,
	functions: String,
	push: String,
}

impl Upstreams {
	fn base(&self, service: Service) -> &str {
		match service {
			Service::Rest | Service::Graphql => &self.rest,
			Service::Auth => &self.auth,
			Service::Storage => &self.storage,
			Service::Realtime => &self.realtime,
			Service::Functions => &self.functions,
			Service::Push => &self.push,
		}
	}
}

pub async fn run() -> Result<(), String> {
	let reference = env::required("SNOUT_REF")?;
	if !crate::keys::is_ref(&reference) {
		return Err(format!(
			"SNOUT_REF {reference:?} is not a project ref (13 characters, a letter first); `snout-stack init` makes one"
		));
	}
	let external = env::required("API_EXTERNAL_URL")?;
	let scheme = if external.starts_with("https://") {
		"https"
	} else {
		"http"
	};
	let port: u16 = env::optional("GATEWAY_PORT").map_or(Ok(8000), |p| {
		p.parse()
			.map_err(|_| format!("GATEWAY_PORT {p:?} is not a port"))
	})?;
	let mut connector = HttpConnector::new();
	connector.set_nodelay(true);
	let gateway = Arc::new(Gateway {
		keys: Keys::new(
			&env::required("ANON_KEY")?,
			&env::required("SERVICE_ROLE_KEY")?,
		),
		reference,
		scheme: scheme.to_owned(),
		functions_door: env::optional("FUNCTIONS_DOOR_SECRET"),
		open_functions: env::list("FUNCTIONS_NO_VERIFY_JWT"),
		client_address_header: env::optional("GATEWAY_CLIENT_ADDRESS_HEADER")
			.map(|h| h.to_ascii_lowercase()),
		upstreams: Upstreams {
			rest: env::optional("REST_URL").unwrap_or_else(|| "http://db:3000".into()),
			auth: env::optional("AUTH_URL").unwrap_or_else(|| "http://db:9999".into()),
			storage: env::optional("STORAGE_URL").unwrap_or_else(|| "http://storage:5000".into()),
			realtime: env::optional("REALTIME_URL")
				.unwrap_or_else(|| "http://realtime:4000".into()),
			functions: env::optional("FUNCTIONS_URL")
				.unwrap_or_else(|| "http://functions:9000".into()),
			push: env::optional("PUSH_URL").unwrap_or_else(|| "http://db:5200".into()),
		},
		client: Client::builder(TokioExecutor::new()).build(connector),
	});
	if gateway.functions_door.is_none() {
		tracing::warn!(
			"FUNCTIONS_DOOR_SECRET is not set: the functions runtime cannot tell the gateway's requests from a function's own"
		);
	}

	let listener = TcpListener::bind(("0.0.0.0", port))
		.await
		.map_err(|e| format!("cannot listen on {port}: {e}"))?;
	tracing::info!(port, reference = %gateway.reference, "gateway listening");
	let shutdown = crate::shutdown_signal();
	tokio::pin!(shutdown);
	loop {
		let (stream, peer) = tokio::select! {
			accepted = listener.accept() => match accepted {
				Ok(pair) => pair,
				Err(error) => {
					tracing::warn!(%error, "accept failed");
					continue;
				}
			},
			() = &mut shutdown => {
				tracing::info!("gateway stopping");
				return Ok(());
			}
		};
		let _ = stream.set_nodelay(true);
		let gateway = gateway.clone();
		tokio::spawn(async move {
			let service = service_fn(move |request| {
				let gateway = gateway.clone();
				async move { Ok::<_, hyper::Error>(gateway.handle(request, peer).await) }
			});
			if let Err(error) = http1::Builder::new()
				.serve_connection(TokioIo::new(stream), service)
				.with_upgrades()
				.await && !error.is_incomplete_message()
			{
				tracing::debug!(%error, "connection ended");
			}
		});
	}
}

fn full(bytes: impl Into<Bytes>) -> Body {
	Full::new(bytes.into())
		.map_err(|never| match never {})
		.boxed()
}

fn header_text(request: &Request<Incoming>, name: &str) -> Option<String> {
	request
		.headers()
		.get(name)
		.and_then(|v| v.to_str().ok())
		.map(str::to_owned)
}

fn with_cors(mut response: Response<Body>, cors: &[(&'static str, String)]) -> Response<Body> {
	for (name, value) in cors {
		if let Ok(value) = HeaderValue::from_str(value) {
			response.headers_mut().insert(*name, value);
		}
	}
	response
}

fn refusal(refused: &Refusal, cors: &[(&'static str, String)]) -> Response<Body> {
	let mut response = Response::new(full(refused.body()));
	*response.status_mut() =
		StatusCode::from_u16(refused.status).unwrap_or(StatusCode::BAD_REQUEST);
	response.headers_mut().insert(
		"content-type",
		HeaderValue::from_static("application/json; charset=utf-8"),
	);
	response
		.headers_mut()
		.insert("cache-control", HeaderValue::from_static("no-store"));
	with_cors(response, cors)
}

fn is_upgrade(request: &Request<Incoming>) -> bool {
	request.headers().contains_key("upgrade")
		&& request
			.headers()
			.get("connection")
			.and_then(|v| v.to_str().ok())
			.is_some_and(|v| {
				v.split(',')
					.any(|t| t.trim().eq_ignore_ascii_case("upgrade"))
			})
}

impl Gateway {
	fn source(&self, request: &Request<Incoming>, peer: SocketAddr) -> String {
		// Behind a TLS terminator the peer is the terminator. Trusted only when configured,
		// because a client can put anything in a header.
		if let Some(name) = &self.client_address_header
			&& let Some(value) = header_text(request, name)
			&& let Some(first) = value
				.split(',')
				.next()
				.map(str::trim)
				.filter(|v| !v.is_empty())
		{
			return first.to_owned();
		}
		peer.ip().to_string()
	}

	async fn handle(&self, mut request: Request<Incoming>, peer: SocketAddr) -> Response<Body> {
		let cors = routes::cors_headers(
			header_text(&request, "origin").as_deref(),
			header_text(&request, "access-control-request-headers").as_deref(),
		);
		// A preflight is answered here: it carries no credential and never reaches a service.
		if request.method() == Method::OPTIONS
			&& request
				.headers()
				.contains_key("access-control-request-method")
		{
			let mut response = Response::new(full(Bytes::new()));
			*response.status_mut() = StatusCode::NO_CONTENT;
			return with_cors(response, &cors);
		}
		let path_and_query = request
			.uri()
			.path_and_query()
			.map_or("/", |p| p.as_str())
			.to_owned();
		let Some(matched) = routes::service_for_path(&path_and_query) else {
			return refusal(&routes::NOT_FOUND, &cors);
		};
		let presented =
			header_text(&request, "apikey").or_else(|| routes::query_api_key(&path_and_query));
		let role = self.keys.classify(presented.as_deref());
		if let Err(refused) = routes::authorise(
			request.method().as_str(),
			&path_and_query,
			&matched,
			role,
			&self.open_functions,
		) {
			return refusal(&refused, &cors);
		}

		let upgrade = is_upgrade(&request);
		let incoming: Vec<(String, String)> = request
			.headers()
			.iter()
			.filter_map(|(name, value)| {
				value
					.to_str()
					.ok()
					.map(|v| (name.as_str().to_owned(), v.to_owned()))
			})
			.collect();
		let source = self.source(&request, peer);
		let (headers, _) = routes::prepare_headers(
			&incoming,
			&path_and_query,
			&matched,
			&Prepare {
				reference: &self.reference,
				domain: DOMAIN,
				source: &source,
				scheme: &self.scheme,
				upgrade,
				functions_door: self.functions_door.as_deref(),
			},
		);

		let client_upgrade = upgrade.then(|| hyper::upgrade::on(&mut request));
		let (parts, body) = request.into_parts();
		let target = format!(
			"{}{}",
			self.upstreams.base(matched.service).trim_end_matches('/'),
			if upgrade {
				matched.upstream_path.clone()
			} else {
				routes::without_api_key(&matched.upstream_path)
			}
		);
		let mut builder = Request::builder().method(parts.method).uri(&target);
		for (name, value) in &headers {
			if let (Ok(name), Ok(value)) = (
				HeaderName::from_bytes(name.as_bytes()),
				HeaderValue::from_str(value),
			) {
				builder = builder.header(name, value);
			}
		}
		let Ok(upstream_request) = builder.body(body) else {
			return refusal(
				&Refusal {
					status: 400,
					message: "The request could not be forwarded.",
					hint: None,
					code: "bad_request",
				},
				&cors,
			);
		};

		let mut answer = match self.client.request(upstream_request).await {
			Ok(answer) => answer,
			Err(error) => {
				tracing::warn!(service = matched.service.name(), %error, "service unreachable");
				return refusal(
					&Refusal {
						status: 502,
						message: "The service behind this path is not answering.",
						hint: Some("It may still be starting; `docker compose ps` shows which."),
						code: "service_unreachable",
					},
					&cors,
				);
			}
		};

		if answer.status() == StatusCode::SWITCHING_PROTOCOLS {
			let upstream_upgrade = hyper::upgrade::on(&mut answer);
			if let Some(client_upgrade) = client_upgrade {
				tokio::spawn(join_upgrades(client_upgrade, upstream_upgrade));
			}
			let mut response = Response::new(Empty::new().map_err(|never| match never {}).boxed());
			*response.status_mut() = StatusCode::SWITCHING_PROTOCOLS;
			copy_visible(answer.headers(), response.headers_mut());
			return response;
		}

		let (parts, body) = answer.into_parts();
		let mut response = Response::new(body.boxed());
		*response.status_mut() = parts.status;
		copy_visible(&parts.headers, response.headers_mut());
		with_cors(response, &cors)
	}
}

fn copy_visible(from: &hyper::HeaderMap, to: &mut hyper::HeaderMap) {
	for (name, value) in from {
		if let Some(visible) = routes::visible_response_header(name.as_str())
			&& let Ok(visible) = HeaderName::from_bytes(visible.as_bytes())
		{
			to.append(visible, value.clone());
		}
	}
}

async fn join_upgrades(client: hyper::upgrade::OnUpgrade, upstream: hyper::upgrade::OnUpgrade) {
	let (client, upstream) = match tokio::try_join!(client, upstream) {
		Ok(pair) => pair,
		Err(error) => {
			tracing::debug!(%error, "upgrade not completed");
			return;
		}
	};
	let mut client = TokioIo::new(client);
	let mut upstream = TokioIo::new(upstream);
	let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
}
