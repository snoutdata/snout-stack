//! `snout-stack setup`: what the hosted service's host agent does for a project, done once for
//! this one, before the services that need it start.
//!
//! 1. The database roles the services connect as get their passwords. Two are the stack's own
//!    (`AUTH_DB_PASSWORD`, `REST_DB_PASSWORD`); the two the shared servers use are derived from the
//!    JWT secret, as the hosted service derives them.
//! 2. The grants the hosted agent makes before registering a project with Realtime (`sql/`,
//!    the same bytes). The publication is the database image's own, made when it is created.
//!    With `PUSH_DB_PASSWORD` set, push's role and schema too (`sql/60-push.sql`, the bytes the
//!    hosted service runs when push is switched on), and the role's password.
//! 3. Storage's bucket is made in the bundled object store (`S3_CREATE_BUCKET=false` with your own
//!    S3, where you make it).
//! 4. The project is registered with storage and Realtime through their admin APIs, and each is
//!    asked once so it prepares its schema in the project's database.
//!
//! It runs in the database container's network namespace (`network_mode: service:db`), so it
//! reaches Postgres on loopback as the pod's superuser, the one role that needs no password there.
//! Every step is idempotent: `docker compose up` runs it on every start.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::Request;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use serde_json::json;
use tokio_postgres::{Client as Pg, NoTls};

use crate::env;
use crate::gateway::DOMAIN;
use crate::keys;
use crate::s3;

/// The roles the database image makes for the servers, which they log in as.
const AUTH_ROLE: &str = "snout_auth_admin";
const REST_ROLE: &str = "authenticator";
const STORAGE_ROLE: &str = "snout_storage_admin";
const REALTIME_ROLE: &str = "snout_realtime_admin";
const PUSH_ROLE: &str = "snout_push_admin";
const PUBLICATION: &str = "snoutdata_realtime";

/// The hosted service's SQL for a project, byte for byte (its own tests hold the two together).
const PROJECT_SQL: &[(&str, &str)] = &[
	(
		"realtime schema grants",
		include_str!("../sql/30-realtime-schema-grants.sql"),
	),
	(
		"realtime filter columns",
		include_str!("../sql/40-realtime-filter-columns.sql"),
	),
	(
		"realtime owner read",
		include_str!("../sql/50-realtime-owner-read.sql"),
	),
];

/// Push's role and schema; the server migrates the schema itself as that role.
const PUSH_SQL: (&str, &str) = ("push role and schema", include_str!("../sql/60-push.sql"));

const WAIT: Duration = Duration::from_secs(180);

struct Settings {
	reference: String,
	jwt_secret: String,
	anon_key: String,
	service_key: String,
	auth_password: String,
	rest_password: String,
	/// Unset in a stack made before push, which then runs without it.
	push_password: Option<String>,
	storage_admin_key: String,
	realtime_api_secret: String,
	/// Where the shared servers reach the project's database: the database service's name.
	tenant_db_host: String,
	storage_url: String,
	storage_admin_url: String,
	realtime_url: String,
	file_size_limit: u64,
	realtime_users: u32,
	realtime_channels: u32,
	realtime_events: u32,
	bucket: Option<Bucket>,
}

/// The bucket to make in the bundled object store, when there is one to make.
struct Bucket {
	endpoint: String,
	name: String,
	region: String,
	access_key: String,
	secret_key: String,
}

fn settings() -> Result<Settings, String> {
	Ok(Settings {
		reference: env::required("SNOUT_REF")?,
		jwt_secret: env::required("JWT_SECRET")?,
		anon_key: env::required("ANON_KEY")?,
		service_key: env::required("SERVICE_ROLE_KEY")?,
		auth_password: env::required("AUTH_DB_PASSWORD")?,
		rest_password: env::required("REST_DB_PASSWORD")?,
		push_password: env::optional("PUSH_DB_PASSWORD"),
		storage_admin_key: env::required("STORAGE_ADMIN_API_KEY")?,
		realtime_api_secret: env::required("REALTIME_API_JWT_SECRET")?,
		tenant_db_host: env::optional("TENANT_DB_HOST").unwrap_or_else(|| "db".into()),
		storage_url: env::optional("STORAGE_URL").unwrap_or_else(|| "http://storage:5000".into()),
		storage_admin_url: env::optional("STORAGE_ADMIN_URL")
			.unwrap_or_else(|| "http://storage:5001".into()),
		realtime_url: env::optional("REALTIME_URL")
			.unwrap_or_else(|| "http://realtime:4000".into()),
		file_size_limit: env::number("STORAGE_FILE_SIZE_LIMIT", 52_428_800)?,
		realtime_users: env::number("REALTIME_MAX_CONCURRENT_USERS", 200)?,
		realtime_channels: env::number("REALTIME_MAX_CHANNELS_PER_CLIENT", 100)?,
		realtime_events: env::number("REALTIME_MAX_EVENTS_PER_SECOND", 100)?,
		bucket: if env::optional("S3_CREATE_BUCKET").is_none_or(|v| v != "false") {
			Some(Bucket {
				endpoint: env::optional("S3_ENDPOINT")
					.unwrap_or_else(|| "http://objects:7070".into()),
				name: env::optional("S3_BUCKET").unwrap_or_else(|| "stack".into()),
				region: env::optional("S3_REGION").unwrap_or_else(|| "us-east-1".into()),
				access_key: env::required("S3_ACCESS_KEY")?,
				secret_key: env::required("S3_SECRET_KEY")?,
			})
		} else {
			None
		},
	})
}

pub async fn run() -> Result<(), String> {
	let s = settings()?;
	if !keys::is_ref(&s.reference) {
		return Err(format!(
			"SNOUT_REF {:?} is not a project ref; `snout-stack init` makes one",
			s.reference
		));
	}
	let storage_password = keys::derived_password(&s.jwt_secret, "storage", &s.reference);
	let realtime_password = keys::derived_password(&s.jwt_secret, "realtime", &s.reference);

	let project = connect(&s.reference).await?;
	let cluster = connect("postgres").await?;
	for (role, password) in [
		(AUTH_ROLE, s.auth_password.as_str()),
		(REST_ROLE, s.rest_password.as_str()),
		(STORAGE_ROLE, storage_password.as_str()),
		(REALTIME_ROLE, realtime_password.as_str()),
	] {
		set_password(&cluster, role, password).await?;
	}
	tracing::info!("role passwords set");
	for step in PROJECT_SQL {
		run_sql(&project, *step).await?;
	}
	tracing::info!("project sql applied");
	if let Some(password) = &s.push_password {
		run_sql(&project, PUSH_SQL).await?;
		set_password(&cluster, PUSH_ROLE, password).await?;
		tracing::info!("push role ready");
	}

	let http = Http::new();
	if let Some(bucket) = &s.bucket {
		create_bucket(&http, bucket).await?;
	}
	register_storage(&http, &s, &storage_password).await?;
	register_realtime(&http, &s, &realtime_password).await?;

	// Realtime prepares its schema when it first connects to the project, which its tenant health
	// call makes it do; storage prepares its own on the first request. Both are waited for, so a
	// migration of the customer's that puts a policy on either finds the tables there.
	wait("realtime to prepare the project", || async {
		let _ = http.call("GET", &format!("{}/api/tenants/{}/health", s.realtime_url, s.reference), &realtime_headers(&s), None).await;
		let row = project
			.query_one("select to_regclass('realtime.messages') is not null and to_regclass('realtime.subscription') is not null", &[])
			.await
			.map_err(|e| e.to_string())?;
		Ok(row.get::<_, bool>(0))
	})
	.await?;
	wait("storage to prepare the project", || async {
		let headers = vec![
			(
				"authorization".to_owned(),
				format!("Bearer {}", s.service_key),
			),
			(
				"x-forwarded-host".to_owned(),
				format!("{}.{DOMAIN}", s.reference),
			),
		];
		let (status, _) = http
			.call("GET", &format!("{}/bucket", s.storage_url), &headers, None)
			.await?;
		Ok(status == 200)
	})
	.await?;

	project
		.batch_execute("notify pgrst, 'reload schema'")
		.await
		.map_err(|e| format!("reloading the data API's schema: {e}"))?;
	tracing::info!(reference = %s.reference, "setup done");
	Ok(())
}

async fn connect(database: &str) -> Result<Pg, String> {
	let config =
		format!("host=127.0.0.1 port=5432 user=snoutpod_admin dbname={database} connect_timeout=5");
	let started = Instant::now();
	loop {
		match tokio_postgres::connect(&config, NoTls).await {
			Ok((client, connection)) => {
				tokio::spawn(async move {
					if let Err(error) = connection.await {
						tracing::warn!(%error, "database connection ended");
					}
				});
				return Ok(client);
			}
			Err(error) if started.elapsed() < WAIT => {
				tracing::debug!(%error, database, "waiting for the database");
				tokio::time::sleep(Duration::from_secs(2)).await;
			}
			Err(error) => {
				return Err(format!(
					"cannot reach the database {database} as snoutpod_admin on 127.0.0.1: {error}"
				));
			}
		}
	}
}

async fn run_sql(client: &Pg, (what, sql): (&str, &str)) -> Result<(), String> {
	client
		.batch_execute(sql)
		.await
		.map_err(|e| format!("{what}: {e}"))
}

/// `alter role … password …` takes no parameters, so the statement is built by Postgres itself
/// with `%I` and `%L`: the password is never spliced into SQL here.
async fn set_password(client: &Pg, role: &str, password: &str) -> Result<(), String> {
	let row = client
		.query_one(
			"select format('alter role %I with password %L', $1::text, $2::text)",
			&[&role, &password],
		)
		.await
		.map_err(|e| format!("preparing {role}'s password: {e}"))?;
	let statement: String = row.get(0);
	client
		.batch_execute(&statement)
		.await
		.map_err(|e| format!("setting {role}'s password: {e}"))
}

fn realtime_headers(s: &Settings) -> Vec<(String, String)> {
	let now = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.map_or(0, |d| d.as_secs());
	vec![(
		"authorization".to_owned(),
		format!("Bearer {}", keys::admin_token(&s.realtime_api_secret, now)),
	)]
}

async fn create_bucket(http: &Http, bucket: &Bucket) -> Result<(), String> {
	wait("the object store", || async {
		let now = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.map_or(0, |d| d.as_secs());
		let signed = s3::create_bucket(
			&bucket.endpoint,
			&bucket.name,
			&bucket.region,
			&bucket.access_key,
			&bucket.secret_key,
			now,
		)?;
		let (status, text) = http.call("PUT", &signed.url, &signed.headers, None).await?;
		// 409 is a bucket that is already there: ours, from an earlier start.
		if (200..300).contains(&status)
			|| (status == 409 && text.contains("BucketAlreadyOwnedByYou"))
		{
			Ok(true)
		} else if status >= 500 || status == 0 {
			Ok(false)
		} else {
			Err(format!(
				"the object store refused to make bucket {} ({status}): {}",
				bucket.name,
				truncate(&text)
			))
		}
	})
	.await?;
	tracing::info!(bucket = %bucket.name, "bucket ready");
	Ok(())
}

async fn register_storage(http: &Http, s: &Settings, password: &str) -> Result<(), String> {
	let body = json!({
		"anonKey": s.anon_key,
		"serviceKey": s.service_key,
		"jwtSecret": s.jwt_secret,
		"databaseUrl": format!("postgres://{STORAGE_ROLE}:{password}@{}:5432/{}", s.tenant_db_host, s.reference),
		"fileSizeLimit": s.file_size_limit,
		"features": { "imageTransformation": { "enabled": true } }
	});
	let headers = vec![("apikey".to_owned(), s.storage_admin_key.clone())];
	let url = format!("{}/tenants/{}", s.storage_admin_url, s.reference);
	wait("storage's admin API", || async {
		let (status, text) = http
			.call("PUT", &url, &headers, Some(body.to_string()))
			.await?;
		if (200..300).contains(&status) {
			Ok(true)
		} else if status >= 500 || status == 0 {
			Ok(false)
		} else {
			Err(format!(
				"storage refused the project ({status}): {}",
				truncate(&text)
			))
		}
	})
	.await?;
	tracing::info!("registered with storage");
	Ok(())
}

async fn register_realtime(http: &Http, s: &Settings, password: &str) -> Result<(), String> {
	let body = json!({
		"tenant": {
			"name": s.reference,
			"external_id": s.reference,
			"jwt_secret": s.jwt_secret,
			"max_concurrent_users": s.realtime_users,
			"max_channels_per_client": s.realtime_channels,
			"max_events_per_second": s.realtime_events,
			"extensions": [{
				"type": "postgres_cdc_rls",
				"settings": {
					"db_host": s.tenant_db_host,
					"db_name": s.reference,
					"db_user": REALTIME_ROLE,
					"db_password": password,
					"db_port": "5432",
					"region": "snoutpod",
					"poll_interval_ms": 100,
					"poll_max_record_bytes": 1_048_576,
					"publication": PUBLICATION,
					"slot_name": "snoutdata_realtime_slot",
					"ssl_enforced": false
				}
			}]
		}
	});
	let url = format!("{}/api/tenants", s.realtime_url);
	wait("Realtime's tenant API", || async {
		let (status, text) = http
			.call("POST", &url, &realtime_headers(s), Some(body.to_string()))
			.await?;
		if (200..300).contains(&status) {
			Ok(true)
		} else if status >= 500 || status == 0 {
			Ok(false)
		} else {
			Err(format!(
				"Realtime refused the project ({status}): {}",
				truncate(&text)
			))
		}
	})
	.await?;
	tracing::info!("registered with Realtime");
	Ok(())
}

fn truncate(text: &str) -> &str {
	// Some answers carry a tenant's stored (sealed) settings; a log line is not the place.
	match text.char_indices().nth(200) {
		Some((at, _)) => &text[..at],
		None => text,
	}
}

/// Retry `check` until it says yes, a real refusal, or `WAIT` runs out. An unreachable service is
/// a `no`: it may still be starting.
async fn wait<F, Fut>(what: &str, mut check: F) -> Result<(), String>
where
	F: FnMut() -> Fut,
	Fut: std::future::Future<Output = Result<bool, String>>,
{
	let started = Instant::now();
	let mut last = String::new();
	loop {
		match check().await {
			Ok(true) => return Ok(()),
			Ok(false) => {}
			Err(error) if error.starts_with("unreachable") => last = error,
			Err(error) => return Err(error),
		}
		if started.elapsed() > WAIT {
			return Err(format!(
				"gave up waiting for {what} after {}s{}",
				WAIT.as_secs(),
				if last.is_empty() {
					String::new()
				} else {
					format!(": {last}")
				}
			));
		}
		tokio::time::sleep(Duration::from_secs(2)).await;
	}
}

struct Http {
	client: Client<HttpConnector, Full<Bytes>>,
}

impl Http {
	fn new() -> Self {
		Http {
			client: Client::builder(TokioExecutor::new()).build(HttpConnector::new()),
		}
	}

	/// One call. `Err("unreachable …")` when nothing answered, which `wait` treats as not yet.
	async fn call(
		&self,
		method: &str,
		url: &str,
		headers: &[(String, String)],
		body: Option<String>,
	) -> Result<(u16, String), String> {
		let mut builder = Request::builder().method(method).uri(url);
		for (name, value) in headers {
			builder = builder.header(name, value);
		}
		if body.is_some() {
			builder = builder.header("content-type", "application/json");
		}
		let request = builder
			.body(Full::new(Bytes::from(body.unwrap_or_default())))
			.map_err(|e| format!("building a request to {url}: {e}"))?;
		let answer = tokio::time::timeout(Duration::from_secs(15), self.client.request(request))
			.await
			.map_err(|_| format!("unreachable: {url} did not answer"))?
			.map_err(|e| format!("unreachable: {url}: {e}"))?;
		let status = answer.status().as_u16();
		let bytes = answer
			.into_body()
			.collect()
			.await
			.map_err(|e| format!("unreachable: {url}: {e}"))?
			.to_bytes();
		Ok((status, String::from_utf8_lossy(&bytes).into_owned()))
	}
}
