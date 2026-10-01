//! Everything secret the stack makes or checks: random values, the project ref, the two API keys
//! (HS256 tokens signed with the project's JWT secret), what the front door compares a presented
//! key against, and the two database passwords derived from the JWT secret.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

/// The characters a ref is drawn from: base32 without i, l, o and u, so a ref read aloud or
/// retyped is not misread. The first character is a letter, so `<ref>` and `<ref>_owner` are
/// legal unquoted SQL identifiers (the pod image names the database and its owner after it).
const ALPHABET: &[u8] = b"0123456789abcdefghjkmnpqrstvwxyz";
const LETTERS: &[u8] = b"abcdefghjkmnpqrstvwxyz";
pub const REF_LENGTH: usize = 13;

fn random_bytes(count: usize) -> Vec<u8> {
	let mut bytes = vec![0u8; count];
	getrandom::fill(&mut bytes).expect("the operating system's random source is unavailable");
	bytes
}

/// `bytes` random bytes as base64url: safe in a URL, a connection string and a `.env` line.
pub fn random(bytes: usize) -> String {
	URL_SAFE_NO_PAD.encode(random_bytes(bytes))
}

/// One character from `set`, without the bias `byte % len` has when `len` does not divide 256.
fn pick(set: &[u8]) -> char {
	let ceiling = 256 - (256 % set.len());
	loop {
		let byte = random_bytes(1)[0] as usize;
		if byte < ceiling {
			return set[byte % set.len()] as char;
		}
	}
}

pub fn new_ref() -> String {
	let mut out = String::with_capacity(REF_LENGTH);
	out.push(pick(LETTERS));
	for _ in 1..REF_LENGTH {
		out.push(pick(ALPHABET));
	}
	out
}

pub fn is_ref(value: &str) -> bool {
	value.len() == REF_LENGTH
		&& LETTERS.contains(&value.as_bytes()[0])
		&& value.bytes().all(|b| ALPHABET.contains(&b))
}

/// An HS256 token. The API keys never expire (their `exp` is 2100-01-01): they are the project's
/// identity, and rotating the JWT secret is what retires them.
pub fn sign(secret: &str, claims: &serde_json::Value) -> String {
	let head = URL_SAFE_NO_PAD.encode(br#"{"alg":"HS256","typ":"JWT"}"#);
	let body = URL_SAFE_NO_PAD.encode(claims.to_string());
	let mut mac =
		HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC takes a key of any length");
	mac.update(format!("{head}.{body}").as_bytes());
	format!(
		"{head}.{body}.{}",
		URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
	)
}

pub fn api_key(secret: &str, role: &str) -> String {
	sign(
		secret,
		&serde_json::json!({ "role": role, "iss": "snoutdata", "iat": 0, "exp": 4_102_444_800_u64 }),
	)
}

/// A short-lived admin token (Realtime's tenant API checks the signature and `exp`). The window
/// starts thirty seconds ago because the container's clock and ours need not agree to the second.
pub fn admin_token(secret: &str, now_secs: u64) -> String {
	sign(
		secret,
		&serde_json::json!({ "role": "service_role", "iat": now_secs.saturating_sub(30), "exp": now_secs + 300 }),
	)
}

/// SHA-256 of a presented key, hex. The front door compares hashes, never the keys themselves.
pub fn key_hash(key: &str) -> String {
	hex::encode(Sha256::digest(key.as_bytes()))
}

pub fn constant_time_eq(a: &str, b: &str) -> bool {
	if a.len() != b.len() {
		return false;
	}
	a.bytes()
		.zip(b.bytes())
		.fold(0u8, |diff, (x, y)| diff | (x ^ y))
		== 0
}

/// A password derived from the JWT secret, domain-separated by `purpose`. The same derivation the
/// hosted service uses for the roles Realtime and storage connect as, so there is no third
/// credential to store, and rotating the JWT secret rotates these with it.
pub fn derived_password(jwt_secret: &str, purpose: &str, reference: &str) -> String {
	let mut mac =
		HmacSha256::new_from_slice(jwt_secret.as_bytes()).expect("HMAC takes a key of any length");
	mac.update(format!("snoutpod:{purpose}:db:{reference}").as_bytes());
	let mut out = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
	out.truncate(32);
	out
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn a_ref_is_thirteen_characters_starting_with_a_letter() {
		for _ in 0..200 {
			let made = new_ref();
			assert!(is_ref(&made), "{made}");
		}
		assert!(!is_ref("0abcdefghjkmn"));
		assert!(!is_ref("abcdefghijklm"), "i and l are not in the alphabet");
		assert!(!is_ref("abc"));
	}

	#[test]
	fn a_token_verifies_with_its_secret() {
		let token = api_key("s3cret-s3cret-s3cret-s3cret-s3cret", "anon");
		let parts: Vec<&str> = token.split('.').collect();
		assert_eq!(parts.len(), 3);
		let claims: serde_json::Value =
			serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap();
		assert_eq!(claims["role"], "anon");
		let mut mac = HmacSha256::new_from_slice(b"s3cret-s3cret-s3cret-s3cret-s3cret").unwrap();
		mac.update(format!("{}.{}", parts[0], parts[1]).as_bytes());
		assert_eq!(
			URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes()),
			parts[2]
		);
	}

	#[test]
	fn derived_passwords_match_the_hosted_derivation() {
		// createHmac('sha256', 'secret').update('snoutpod:realtime:db:abcdefghjkmnp')
		//   .digest('base64url').slice(0, 32), computed with Node.
		let made = derived_password("secret", "realtime", "abcdefghjkmnp");
		assert_eq!(made, "tloHwqmJCilku-9gauvM4UYmLWBU2IdP");
		assert_ne!(made, derived_password("secret", "storage", "abcdefghjkmnp"));
		assert_eq!(
			made,
			derived_password("secret", "realtime", "abcdefghjkmnp")
		);
	}

	#[test]
	fn hashes_compare_in_constant_time_and_correctly() {
		assert!(constant_time_eq(&key_hash("a"), &key_hash("a")));
		assert!(!constant_time_eq(&key_hash("a"), &key_hash("b")));
		assert!(!constant_time_eq("ab", "abc"));
	}
}
