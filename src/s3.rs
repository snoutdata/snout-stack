//! The one S3 call the stack makes itself: creating storage's bucket in the bundled object store.
//!
//! Signed with AWS Signature Version 4, path style, with an empty body. Everything else storage
//! does with S3 is storage's own; this exists so `docker compose up` needs no client tool to make
//! the bucket first. With your own S3 (AWS, R2), you make the bucket and this is not called.

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

pub struct Signed {
	pub url: String,
	pub headers: Vec<(String, String)>,
}

fn hmac(key: &[u8], data: &str) -> Vec<u8> {
	let mut mac = HmacSha256::new_from_slice(key).expect("HMAC takes a key of any length");
	mac.update(data.as_bytes());
	mac.finalize().into_bytes().to_vec()
}

/// `(yyyymmdd, yyyymmddThhmmssZ)` for a time in seconds since 1970, in UTC.
pub fn amz_dates(unix: u64) -> (String, String) {
	let days = (unix / 86_400) as i64;
	let seconds = unix % 86_400;
	// Howard Hinnant's days-to-civil.
	let z = days + 719_468;
	let era = z.div_euclid(146_097);
	let doe = z - era * 146_097;
	let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
	let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
	let mp = (5 * doy + 2) / 153;
	let day = doy - (153 * mp + 2) / 5 + 1;
	let month = if mp < 10 { mp + 3 } else { mp - 9 };
	let year = yoe + era * 400 + i64::from(month <= 2);
	let date = format!("{year:04}{month:02}{day:02}");
	let stamp = format!(
		"{date}T{:02}{:02}{:02}Z",
		seconds / 3_600,
		(seconds % 3_600) / 60,
		seconds % 60
	);
	(date, stamp)
}

/// A signed `PUT /<bucket>` against `endpoint` (`http://host:port`, no path).
pub fn create_bucket(
	endpoint: &str,
	bucket: &str,
	region: &str,
	access_key: &str,
	secret_key: &str,
	unix: u64,
) -> Result<Signed, String> {
	let host = endpoint
		.strip_prefix("http://")
		.or_else(|| endpoint.strip_prefix("https://"))
		.ok_or_else(|| format!("S3_ENDPOINT {endpoint:?} is not an http(s) address"))?
		.trim_end_matches('/');
	if host.contains('/') {
		return Err(format!(
			"S3_ENDPOINT {endpoint:?} has a path; give the host only"
		));
	}
	let valid_bucket = (3..=63).contains(&bucket.len())
		&& bucket
			.bytes()
			.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'.');
	if !valid_bucket {
		return Err(format!("S3_BUCKET {bucket:?} is not a bucket name"));
	}
	let (date, stamp) = amz_dates(unix);
	let payload = hex::encode(Sha256::digest(b""));
	let canonical = format!(
		"PUT\n/{bucket}\n\nhost:{host}\nx-amz-content-sha256:{payload}\nx-amz-date:{stamp}\n\nhost;x-amz-content-sha256;x-amz-date\n{payload}"
	);
	let scope = format!("{date}/{region}/s3/aws4_request");
	let to_sign = format!(
		"AWS4-HMAC-SHA256\n{stamp}\n{scope}\n{}",
		hex::encode(Sha256::digest(canonical.as_bytes()))
	);
	let mut key = hmac(format!("AWS4{secret_key}").as_bytes(), &date);
	for part in [region, "s3", "aws4_request"] {
		key = hmac(&key, part);
	}
	let signature = hex::encode(hmac(&key, &to_sign));
	Ok(Signed {
		url: format!("{}/{bucket}", endpoint.trim_end_matches('/')),
		headers: vec![
			("host".into(), host.to_owned()),
			("x-amz-content-sha256".into(), payload),
			("x-amz-date".into(), stamp),
			(
				"authorization".into(),
				format!(
					"AWS4-HMAC-SHA256 Credential={access_key}/{scope}, SignedHeaders=host;x-amz-content-sha256;x-amz-date, Signature={signature}"
				),
			),
		],
	})
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn dates_are_utc_calendar_dates() {
		assert_eq!(amz_dates(0), ("19700101".into(), "19700101T000000Z".into()));
		// 2024-02-29 23:59:59, a leap day.
		assert_eq!(amz_dates(1_709_251_199).1, "20240229T235959Z");
		// 2026-10-01 12:34:56.
		assert_eq!(amz_dates(1_790_858_096).1, "20261001T123456Z");
	}

	#[test]
	fn the_request_is_path_style_and_signed_over_the_headers_it_sends() {
		let signed =
			create_bucket("http://objects:7070", "stack", "us-east-1", "ak", "sk", 0).unwrap();
		assert_eq!(signed.url, "http://objects:7070/stack");
		let auth = &signed
			.headers
			.iter()
			.find(|(n, _)| n == "authorization")
			.unwrap()
			.1;
		assert!(auth.starts_with("AWS4-HMAC-SHA256 Credential=ak/19700101/us-east-1/s3/aws4_request, SignedHeaders=host;x-amz-content-sha256;x-amz-date, Signature="));
		assert_eq!(auth.rsplit('=').next().unwrap().len(), 64);
		// The same inputs sign the same; a different secret does not.
		let again =
			create_bucket("http://objects:7070", "stack", "us-east-1", "ak", "sk", 0).unwrap();
		assert_eq!(signed.headers, again.headers);
		let other =
			create_bucket("http://objects:7070", "stack", "us-east-1", "ak", "sx", 0).unwrap();
		assert_ne!(signed.headers, other.headers);
	}

	#[test]
	fn bad_endpoints_and_buckets_are_refused() {
		assert!(create_bucket("objects:7070", "stack", "r", "a", "s", 0).is_err());
		assert!(create_bucket("http://h/x", "stack", "r", "a", "s", 0).is_err());
		assert!(create_bucket("http://h", "Stack", "r", "a", "s", 0).is_err());
	}
}
