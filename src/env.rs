//! Reading the environment, with a sentence when something required is missing.

pub fn optional(name: &str) -> Option<String> {
	std::env::var(name)
		.ok()
		.map(|v| v.trim().to_owned())
		.filter(|v| !v.is_empty())
}

pub fn required(name: &str) -> Result<String, String> {
	optional(name).ok_or_else(|| {
		format!(
			"{name} is not set. `snout-stack init` writes every value the stack needs into .env"
		)
	})
}

/// A comma-separated list; empty items are dropped.
pub fn list(name: &str) -> Vec<String> {
	optional(name)
		.map(|v| {
			v.split(',')
				.map(|s| s.trim().to_owned())
				.filter(|s| !s.is_empty())
				.collect()
		})
		.unwrap_or_default()
}

pub fn number<T: std::str::FromStr>(name: &str, default: T) -> Result<T, String> {
	match optional(name) {
		None => Ok(default),
		Some(value) => value
			.parse()
			.map_err(|_| format!("{name} {value:?} is not a number")),
	}
}
