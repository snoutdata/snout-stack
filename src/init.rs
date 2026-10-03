//! `snout-stack init`: every secret the stack needs, made here, written as a `.env`.
//!
//! There is no example key anywhere in this repository. The compose file names each value as
//! `${NAME:?…}`, so a stack without them does not start, and this is the one way they are made.

use std::fs::OpenOptions;
use std::io::Write;

use crate::keys;

pub fn run(args: &[String]) -> Result<(), String> {
	let mut out: Option<String> = None;
	let mut force = false;
	let mut rest = args.iter();
	while let Some(arg) = rest.next() {
		match arg.as_str() {
			"--out" => out = Some(rest.next().ok_or("--out needs a file name")?.clone()),
			"--force" => force = true,
			other => return Err(format!("init does not take {other:?}")),
		}
	}
	let text = render(&fresh());
	match out {
		None => {
			print!("{text}");
			Ok(())
		}
		Some(path) => {
			let mut options = OpenOptions::new();
			options.write(true);
			if force {
				options.create(true).truncate(true);
			} else {
				// Never over an existing one: it holds the keys every client was given and the
				// passwords the database already has.
				options.create_new(true);
			}
			let mut file = options.open(&path).map_err(|e| {
				if e.kind() == std::io::ErrorKind::AlreadyExists {
					format!(
						"{path} already exists; it holds this stack's keys and passwords. --force replaces it"
					)
				} else {
					format!("cannot write {path}: {e}")
				}
			})?;
			file.write_all(text.as_bytes())
				.map_err(|e| format!("cannot write {path}: {e}"))?;
			eprintln!("wrote {path}. It holds passwords and keys: keep it out of version control.");
			Ok(())
		}
	}
}

/// One stack's values, in the order the file lists them.
pub struct Values {
	pub pairs: Vec<(&'static str, String, &'static str)>,
}

pub fn fresh() -> Values {
	let reference = keys::new_ref();
	let jwt_secret = keys::random(48);
	let anon = keys::api_key(&jwt_secret, "anon");
	let service = keys::api_key(&jwt_secret, "service_role");
	Values {
		pairs: vec![
			(
				"COMPOSE_PROFILES",
				"objects,images,push".into(),
				"Optional parts of the stack. `objects` is the bundled object store: remove it to keep files in your own S3 (see S3_ENDPOINT below). `images` renders resized images: remove it if you do not need that. `push` sends push notifications to iPhone, Android and the web.",
			),
			(
				"SNOUT_REF",
				reference,
				"The project's ref. It names the database and its owner role.",
			),
			(
				"API_EXTERNAL_URL",
				"http://localhost:8000".into(),
				"Where clients reach the gateway. Put your public https address here.",
			),
			(
				"SITE_URL",
				"http://localhost:3000".into(),
				"Your application: where a user lands after a sign-in or confirmation link.",
			),
			(
				"JWT_SECRET",
				jwt_secret,
				"Signs every token. Changing it retires both keys below and every session.",
			),
			(
				"ANON_KEY",
				anon,
				"The public key: ships in your application.",
			),
			(
				"SERVICE_ROLE_KEY",
				service,
				"The server key: bypasses row-level security. Never ship it.",
			),
			(
				"POSTGRES_PASSWORD",
				keys::random(24),
				"The database superuser's password (it connects only from inside the database container).",
			),
			(
				"POSTGRES_OWNER_PASSWORD",
				keys::random(24),
				"The project owner's password: the login you connect with, as <SNOUT_REF>_owner.",
			),
			(
				"AUTH_DB_PASSWORD",
				keys::random(24),
				"The auth server's database role.",
			),
			(
				"REST_DB_PASSWORD",
				keys::random(24),
				"The data API's database role (authenticator).",
			),
			(
				"PUSH_DB_PASSWORD",
				keys::random(24),
				"The push server's database role.",
			),
			(
				"PUSH_VAPID_SUBJECT",
				"mailto:admin@example.com".into(),
				"Your contact, a mailto: or https: address, named in every web push request. Put your own.",
			),
			(
				"METADATA_DB_PASSWORD",
				keys::random(24),
				"The shared servers' own metadata database.",
			),
			(
				"REALTIME_API_JWT_SECRET",
				keys::random(32),
				"Signs calls to Realtime's tenant API.",
			),
			(
				"REALTIME_METRICS_JWT_SECRET",
				keys::random(32),
				"Signs calls to Realtime's /metrics.",
			),
			(
				"REALTIME_DB_ENC_KEY",
				keys::random(12),
				"Encrypts Realtime's stored tenant secrets (exactly 16 characters).",
			),
			(
				"STORAGE_ADMIN_API_KEY",
				keys::random(32),
				"Authorises calls to storage's admin port.",
			),
			(
				"STORAGE_ENCRYPTION_KEY",
				keys::random(24),
				"Encrypts storage's stored tenant secrets.",
			),
			(
				"S3_ENDPOINT",
				"http://objects:7070".into(),
				"Where storage keeps files: the bundled object store. For your own S3, its endpoint, with S3_BUCKET, S3_REGION and the keys below, and S3_CREATE_BUCKET=false.",
			),
			("S3_BUCKET", "stack".into(), "The bucket files are kept in."),
			(
				"S3_ACCESS_KEY",
				format!("stack{}", keys::random(9)),
				"The object store's access key.",
			),
			(
				"S3_SECRET_KEY",
				keys::random(30),
				"The object store's secret key.",
			),
			(
				"FUNCTIONS_DOOR_SECRET",
				keys::random(32),
				"Proves to the functions runtime that a request came through the gateway.",
			),
		],
	}
}

pub fn render(values: &Values) -> String {
	let mut text = String::from(
		"# Written by `snout-stack init`. Every value was made for this stack alone.\n\
		 # It holds passwords and keys: keep it out of version control, and back it up with your data.\n\n",
	);
	for (name, value, note) in &values.pairs {
		text.push_str(&format!("# {note}\n{name}={value}\n\n"));
	}
	text
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Settings rather than secrets: the same in every stack until someone changes them.
	const FIXED: [&str; 4] = [
		"COMPOSE_PROFILES",
		"S3_ENDPOINT",
		"S3_BUCKET",
		"PUSH_VAPID_SUBJECT",
	];

	#[test]
	fn every_value_is_fresh_and_the_keys_are_signed_with_the_secret() {
		let one = fresh();
		let two = fresh();
		for ((name, a, _), (_, b, _)) in one.pairs.iter().zip(two.pairs.iter()) {
			if !name.ends_with("_URL") && !FIXED.contains(name) {
				assert_ne!(a, b, "{name} repeated across two stacks");
			}
		}
		let get = |name: &str| {
			one.pairs
				.iter()
				.find(|(n, ..)| *n == name)
				.map(|(_, v, _)| v.clone())
				.unwrap()
		};
		assert!(keys::is_ref(&get("SNOUT_REF")));
		assert_eq!(get("ANON_KEY"), keys::api_key(&get("JWT_SECRET"), "anon"));
		assert_eq!(get("REALTIME_DB_ENC_KEY").len(), 16);
		assert!(get("JWT_SECRET").len() >= 32);
	}

	#[test]
	fn values_are_safe_on_a_dotenv_line() {
		let text = render(&fresh());
		for line in text
			.lines()
			.filter(|l| !l.starts_with('#') && !l.is_empty())
		{
			let (_, value) = line.split_once('=').unwrap();
			assert!(
				value
					.bytes()
					.all(|b| b.is_ascii_alphanumeric() || b"-_.:/,@".contains(&b)),
				"{line}"
			);
		}
	}
}
