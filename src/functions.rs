//! `snout-stack functions`: a folder of functions, laid out as the functions runtime reads them.
//!
//! The source is one folder per function, `functions/<name>/index.ts`, beside any shared code
//! (`functions/_shared/…`, imported as `../_shared/x.ts`). The runtime reads a mount it never
//! writes:
//!
//!   projects/<ref>.json        which functions the project has, its limits and its variables
//!   bundles/<digest>/src/…     the folder's files, as they were
//!   bundles/<digest>/index.ts  one line importing the function's entrypoint
//!   bundles/<digest>/.built/   main.js: every `npm:`, `jsr:` and URL import resolved into one
//!                              file by `deno bundle`, here and never on a caller's request;
//!                              error.txt when that could not be done, which the caller is told
//!
//! A digest is the hash of what went into the bundle, so a function that did not change is not
//! bundled again, and a changed one is a new directory the runtime picks up on its next request
//! (it reads the manifest again whenever the file changes). Bundles no function names any more
//! are removed. Run it again after any change: `docker compose run --rm functions-deploy`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

use crate::env;
use crate::keys;

/// The variables every function is given, set from the stack's own keys.
pub const PLATFORM_ENV: [&str; 3] = [
	"SNOUTDATA_URL",
	"SNOUTDATA_ANON_KEY",
	"SNOUTDATA_SERVICE_ROLE_KEY",
];

/// Entrypoints, in the order they are looked for.
const ENTRYPOINTS: [&str; 5] = [
	"index.ts",
	"index.js",
	"index.mts",
	"index.mjs",
	"index.tsx",
];

/// Folders under the source that are never copied into a bundle: a package manager's, and
/// anything hidden (`.env` holds the functions' secrets, which reach them as variables, never as
/// a file in their bundle).
const SKIPPED: [&str; 1] = ["node_modules"];

pub fn run(args: &[String]) -> Result<(), String> {
	let mut source = PathBuf::from("/functions");
	let mut out = PathBuf::from("/snoutfn");
	let mut rest = args.iter();
	while let Some(arg) = rest.next() {
		match arg.as_str() {
			"--source" => source = rest.next().ok_or("--source needs a folder")?.into(),
			"--out" => out = rest.next().ok_or("--out needs a folder")?.into(),
			other => return Err(format!("functions does not take {other:?}")),
		}
	}
	let reference = env::required("SNOUT_REF")?;
	if !keys::is_ref(&reference) {
		return Err(format!(
			"SNOUT_REF {reference:?} is not a project ref; `snout-stack init` makes one"
		));
	}
	let deno = env::optional("SNOUT_STACK_DENO").unwrap_or_else(|| "deno".into());

	let found = discover(&source)?;
	let files = if found.is_empty() {
		Vec::new()
	} else {
		read_tree(&source)?
	};
	let mut variables = platform_env()?;
	let secrets_file = source.join(".env");
	if secrets_file.is_file() {
		let text = fs::read_to_string(&secrets_file)
			.map_err(|e| format!("cannot read {}: {e}", secrets_file.display()))?;
		for (name, value) in parse_dotenv(&text)? {
			if PLATFORM_ENV.contains(&name.as_str()) {
				return Err(format!(
					"{name} in {} is set for you from this stack's keys and cannot be overridden",
					secrets_file.display()
				));
			}
			variables.insert(name, value);
		}
	}

	let bundles = out.join("bundles");
	let projects = out.join("projects");
	for dir in [&bundles, &projects] {
		fs::create_dir_all(dir).map_err(|e| format!("cannot make {}: {e}", dir.display()))?;
	}
	let builder = builder_identity(&deno);
	let open = env::list("FUNCTIONS_NO_VERIFY_JWT");

	let mut deployed = Vec::new();
	let mut failed = Vec::new();
	for (name, entrypoint) in &found {
		let digest = digest(name, entrypoint, &files);
		let directory = bundles.join(&digest);
		if !directory.join("index.ts").is_file() {
			write_bundle(&directory, entrypoint, &files)?;
		}
		match build(&deno, &builder, &directory) {
			Ok(()) => println!("{name}: ready ({})", &digest[..12]),
			Err(message) => {
				println!("{name}: could not be bundled; its callers are told why");
				eprintln!("{message}");
				failed.push(name.clone());
			}
		}
		deployed.push(deployed_function(name, &digest, &open));
	}

	let mut manifest = serde_json::json!({
		"functions": deployed,
		"limits": limits()?,
		"env": variables,
	});
	// The runtime checks a key-required function's token against it (snout-functions 0.2.2).
	// Beside `env`, never in it, so no function can read it.
	if let Some(secret) = env::optional("JWT_SECRET") {
		manifest["jwtSecret"] = serde_json::Value::String(secret);
	}
	write_atomically(
		&projects.join(format!("{reference}.json")),
		&serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?,
	)?;
	let kept: Vec<String> = deployed
		.iter()
		.filter_map(|d| d["digest"].as_str().map(str::to_owned))
		.collect();
	prune(&bundles, &kept)?;
	println!(
		"{} function{} deployed",
		found.len(),
		if found.len() == 1 { "" } else { "s" }
	);
	// One function that will not bundle must not keep the others from being served: its callers
	// are told why (error.txt), and this says so here.
	if !failed.is_empty() {
		eprintln!("could not bundle: {}", failed.join(", "));
	}
	Ok(())
}

/// The runtime's rule for a function's name: letters, digits, hyphens and underscores, a letter or
/// digit first, at most 48 characters. A folder starting with `_` is shared code, not a function.
pub fn is_function_name(name: &str) -> bool {
	let Some(lead) = name.bytes().next() else {
		return false;
	};
	lead.is_ascii_alphanumeric()
		&& name.len() <= 48
		&& name
			.bytes()
			.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// The functions in `source`: each folder with a valid name and an entrypoint, by name.
fn discover(source: &Path) -> Result<Vec<(String, String)>, String> {
	if !source.is_dir() {
		return Err(format!(
			"{} is not a folder. Put each function in functions/<name>/index.ts",
			source.display()
		));
	}
	let mut found = Vec::new();
	let entries =
		fs::read_dir(source).map_err(|e| format!("cannot read {}: {e}", source.display()))?;
	for entry in entries.flatten() {
		let name = entry.file_name().to_string_lossy().into_owned();
		if !entry.path().is_dir() || name.starts_with('_') || name.starts_with('.') {
			continue;
		}
		if !is_function_name(&name) {
			eprintln!("skipping {name}: a function's name is letters, digits, - and _, at most 48");
			continue;
		}
		match ENTRYPOINTS
			.iter()
			.find(|file| entry.path().join(file).is_file())
		{
			Some(file) => found.push((name.clone(), format!("{name}/{file}"))),
			None => eprintln!("skipping {name}: it has no index.ts"),
		}
	}
	found.sort();
	Ok(found)
}

/// Every file under `source` that goes into a bundle, by its path relative to `source` with `/`
/// separators, sorted.
fn read_tree(source: &Path) -> Result<Vec<(String, Vec<u8>)>, String> {
	let mut files = Vec::new();
	let mut pending = vec![(source.to_path_buf(), String::new())];
	while let Some((dir, prefix)) = pending.pop() {
		let entries =
			fs::read_dir(&dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
		for entry in entries.flatten() {
			let name = entry.file_name().to_string_lossy().into_owned();
			if name.starts_with('.') || SKIPPED.contains(&name.as_str()) {
				continue;
			}
			let relative = if prefix.is_empty() {
				name
			} else {
				format!("{prefix}/{name}")
			};
			let kind = entry.file_type().map_err(|e| e.to_string())?;
			if kind.is_dir() {
				pending.push((entry.path(), relative));
			} else if kind.is_file() {
				let bytes = fs::read(entry.path())
					.map_err(|e| format!("cannot read {}: {e}", entry.path().display()))?;
				files.push((relative, bytes));
			}
		}
	}
	files.sort_by(|a, b| a.0.cmp(&b.0));
	Ok(files)
}

/// What a bundle is made of, hashed: the format, the entrypoint and every file with its path.
/// Lengths are written before contents, so no two different trees hash alike.
pub fn digest(name: &str, entrypoint: &str, files: &[(String, Vec<u8>)]) -> String {
	let mut hash = Sha256::new();
	hash.update(b"snout-stack-bundle-1\0");
	for part in [name.as_bytes(), entrypoint.as_bytes()] {
		hash.update((part.len() as u64).to_be_bytes());
		hash.update(part);
	}
	for (path, bytes) in files {
		hash.update((path.len() as u64).to_be_bytes());
		hash.update(path.as_bytes());
		hash.update((bytes.len() as u64).to_be_bytes());
		hash.update(bytes);
	}
	hex::encode(hash.finalize())
}

/// The shim the runtime starts: importing a module runs it, and a function's module serves from
/// its top level.
pub fn shim(entrypoint: &str) -> String {
	format!(
		"// Written by snout-stack. The function's own source is under src/.\nimport './src/{entrypoint}';\n"
	)
}

fn write_bundle(
	directory: &Path,
	entrypoint: &str,
	files: &[(String, Vec<u8>)],
) -> Result<(), String> {
	let scratch = directory.with_extension("unpacking");
	let _ = fs::remove_dir_all(&scratch);
	for (path, bytes) in files {
		let target = scratch.join("src").join(path);
		if let Some(parent) = target.parent() {
			fs::create_dir_all(parent)
				.map_err(|e| format!("cannot make {}: {e}", parent.display()))?;
		}
		fs::write(&target, bytes).map_err(|e| format!("cannot write {}: {e}", target.display()))?;
	}
	fs::write(scratch.join("index.ts"), shim(entrypoint)).map_err(|e| e.to_string())?;
	let _ = fs::remove_dir_all(directory);
	fs::rename(&scratch, directory)
		.map_err(|e| format!("cannot place {}: {e}", directory.display()))
}

/// What made a build, written beside it: a different Deno rebuilds.
fn builder_identity(deno: &str) -> String {
	let version = Command::new(deno)
		.arg("--version")
		.output()
		.ok()
		.and_then(|out| String::from_utf8(out.stdout).ok())
		.and_then(|text| text.lines().next().map(str::to_owned))
		.unwrap_or_default();
	format!("{deno} {version}").trim().to_owned()
}

/// `deno bundle` over one bundle, unless it was already made by this builder. A failure is
/// written where the runtime reads it.
fn build(deno: &str, builder: &str, directory: &Path) -> Result<(), String> {
	let built = directory.join(".built");
	if fs::read_to_string(built.join("builder")).is_ok_and(|held| held.trim() == builder)
		&& built.join("main.js").is_file()
	{
		return Ok(());
	}
	let fresh = directory.join(".built.new");
	let _ = fs::remove_dir_all(&fresh);
	fs::create_dir_all(&fresh).map_err(|e| e.to_string())?;
	let output = Command::new(deno)
		.current_dir(directory)
		.env("DENO_NO_UPDATE_CHECK", "1")
		.args([
			"bundle",
			"--quiet",
			"--platform",
			"deno",
			"--format",
			"esm",
			"--output",
		])
		.arg(fresh.join("main.js"))
		.arg(directory.join("index.ts"))
		.output()
		.map_err(|e| format!("cannot run {deno}: {e}"))?;
	if output.status.success() && fresh.join("main.js").is_file() {
		fs::write(fresh.join("builder"), format!("{builder}\n")).map_err(|e| e.to_string())?;
		let _ = fs::remove_dir_all(&built);
		fs::rename(&fresh, &built).map_err(|e| e.to_string())?;
		return Ok(());
	}
	let mut message = String::from_utf8_lossy(&output.stderr).into_owned();
	message.push_str(&String::from_utf8_lossy(&output.stdout));
	message.truncate(4000);
	let _ = fs::remove_dir_all(&fresh);
	let _ = fs::remove_dir_all(&built);
	fs::create_dir_all(&built).map_err(|e| e.to_string())?;
	fs::write(built.join("error.txt"), format!("{}\n", message.trim()))
		.map_err(|e| e.to_string())?;
	Err(message)
}

fn platform_env() -> Result<BTreeMap<String, String>, String> {
	let mut out = BTreeMap::new();
	// Where a function reaches this stack's API: the gateway on the stack's own network, since the
	// public address may be one the functions container cannot reach (localhost is its own).
	out.insert(
		PLATFORM_ENV[0].to_owned(),
		env::optional("FUNCTIONS_API_URL").unwrap_or_else(|| "http://gateway:8000".into()),
	);
	out.insert(PLATFORM_ENV[1].to_owned(), env::required("ANON_KEY")?);
	out.insert(
		PLATFORM_ENV[2].to_owned(),
		env::required("SERVICE_ROLE_KEY")?,
	);
	Ok(out)
}

/// One function as the manifest names it. `verifyJwt` is false only for a function the gateway lets
/// through with no key (`FUNCTIONS_NO_VERIFY_JWT`, a webhook's receiver): every other one is run
/// only for a token signed with this stack's secret.
fn deployed_function(name: &str, digest: &str, open: &[String]) -> serde_json::Value {
	serde_json::json!({ "name": name, "digest": digest, "verifyJwt": !open.iter().any(|o| o == name) })
}

fn limits() -> Result<serde_json::Value, String> {
	Ok(serde_json::json!({
		"memoryMb": env::number::<u32>("FUNCTIONS_MEMORY_MB", 256)?,
		"wallMs": env::number::<u64>("FUNCTIONS_WALL_MS", 60_000)?,
		"cpuMs": env::number::<u64>("FUNCTIONS_CPU_MS", 5_000)?,
	}))
}

/// `NAME=value` lines; `#` comments and blank lines skipped; a value may be quoted with `"` or
/// `'`, and `export ` before a name is accepted. Names are what a shell accepts.
pub fn parse_dotenv(text: &str) -> Result<Vec<(String, String)>, String> {
	let mut out = Vec::new();
	for (number, line) in text.lines().enumerate() {
		let line = line.trim();
		if line.is_empty() || line.starts_with('#') {
			continue;
		}
		let line = line.strip_prefix("export ").unwrap_or(line);
		let Some((name, value)) = line.split_once('=') else {
			return Err(format!(
				"functions/.env line {}: not NAME=value",
				number + 1
			));
		};
		let name = name.trim();
		let valid = name
			.bytes()
			.next()
			.is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
			&& name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
		if !valid {
			return Err(format!(
				"functions/.env line {}: {name:?} is not a variable name",
				number + 1
			));
		}
		let value = value.trim();
		let value = ['"', '\'']
			.iter()
			.find_map(|q| value.strip_prefix(*q).and_then(|v| v.strip_suffix(*q)))
			.unwrap_or(value);
		out.push((name.to_owned(), value.to_owned()));
	}
	Ok(out)
}

/// Written beside, then renamed over, so the runtime reads the old manifest or the new one. Only
/// its owner may read it: it holds the service key and the functions' secrets.
fn write_atomically(path: &Path, bytes: &[u8]) -> Result<(), String> {
	let fresh = path.with_extension("json.new");
	fs::write(&fresh, bytes).map_err(|e| format!("cannot write {}: {e}", fresh.display()))?;
	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		fs::set_permissions(&fresh, fs::Permissions::from_mode(0o600))
			.map_err(|e| e.to_string())?;
	}
	fs::rename(&fresh, path).map_err(|e| format!("cannot place {}: {e}", path.display()))
}

fn prune(bundles: &Path, kept: &[String]) -> Result<(), String> {
	let entries = fs::read_dir(bundles).map_err(|e| e.to_string())?;
	for entry in entries.flatten() {
		let name = entry.file_name().to_string_lossy().into_owned();
		if !kept.contains(&name) {
			let _ = fs::remove_dir_all(entry.path());
		}
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	fn tree(files: &[(&str, &str)]) -> Vec<(String, Vec<u8>)> {
		files
			.iter()
			.map(|(p, t)| ((*p).to_owned(), t.as_bytes().to_vec()))
			.collect()
	}

	#[test]
	fn names_follow_the_runtimes_rule() {
		assert!(is_function_name("hello"));
		assert!(is_function_name("stripe-hook_2"));
		assert!(!is_function_name(""));
		assert!(!is_function_name("-x"));
		assert!(!is_function_name("a.b"));
		assert!(!is_function_name(&"a".repeat(49)));
	}

	#[test]
	fn a_digest_is_hex_and_moves_with_any_part_of_the_bundle() {
		let files = tree(&[("hello/index.ts", "Deno.serve(() => new Response('hi'))")]);
		let one = digest("hello", "hello/index.ts", &files);
		assert_eq!(one.len(), 64);
		assert!(
			one.bytes()
				.all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
		);
		assert_eq!(one, digest("hello", "hello/index.ts", &files));
		assert_ne!(one, digest("other", "hello/index.ts", &files));
		let changed = tree(&[("hello/index.ts", "Deno.serve(() => new Response('ho'))")]);
		assert_ne!(one, digest("hello", "hello/index.ts", &changed));
		// A path and its contents cannot trade bytes.
		let a = tree(&[("ab", "c")]);
		let b = tree(&[("a", "bc")]);
		assert_ne!(digest("n", "e", &a), digest("n", "e", &b));
	}

	#[test]
	fn the_shim_imports_the_entrypoint_under_src() {
		assert!(shim("hello/index.ts").contains("import './src/hello/index.ts';"));
	}

	#[test]
	fn dotenv_lines_are_read_as_a_shell_would() {
		let read = parse_dotenv(
			"# a comment\n\nSTRIPE_KEY=sk_1\nexport QUOTED=\"a b\"\nSINGLE='x=y'\nEMPTY=\n",
		)
		.unwrap();
		assert_eq!(
			read,
			vec![
				("STRIPE_KEY".into(), "sk_1".into()),
				("QUOTED".into(), "a b".into()),
				("SINGLE".into(), "x=y".into()),
				("EMPTY".into(), String::new()),
			]
		);
		assert!(parse_dotenv("1BAD=x").is_err());
		assert!(parse_dotenv("no equals").is_err());
	}

	#[test]
	fn a_function_needs_a_signed_token_unless_it_is_listed_open() {
		let open = vec!["stripe-webhook".to_owned()];
		assert_eq!(deployed_function("hello", "d", &open)["verifyJwt"], true);
		assert_eq!(deployed_function("stripe-webhook", "d", &open)["verifyJwt"], false);
		assert_eq!(deployed_function("stripe", "d", &open)["verifyJwt"], true);
	}

	#[test]
	fn a_folder_becomes_bundles_and_a_manifest_and_secrets_stay_out_of_bundles() {
		let root = std::env::temp_dir().join(format!("snout-stack-fn-{}", keys::random(6)));
		let source = root.join("functions");
		fs::create_dir_all(source.join("hello")).unwrap();
		fs::create_dir_all(source.join("_shared")).unwrap();
		fs::create_dir_all(source.join("no-entry")).unwrap();
		fs::write(source.join("hello/index.ts"), "import '../_shared/x.ts';").unwrap();
		fs::write(source.join("_shared/x.ts"), "export {};").unwrap();
		fs::write(source.join(".env"), "SECRET=1").unwrap();

		let found = discover(&source).unwrap();
		assert_eq!(found, vec![("hello".into(), "hello/index.ts".into())]);
		let files = read_tree(&source).unwrap();
		let paths: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
		assert_eq!(paths, vec!["_shared/x.ts", "hello/index.ts"]);

		let directory = root.join("bundles").join("d");
		write_bundle(&directory, "hello/index.ts", &files).unwrap();
		assert!(directory.join("src/_shared/x.ts").is_file());
		assert!(!directory.join("src/.env").exists());
		assert_eq!(
			fs::read_to_string(directory.join("index.ts")).unwrap(),
			shim("hello/index.ts")
		);

		write_atomically(&root.join("p.json"), b"{}").unwrap();
		assert_eq!(fs::read_to_string(root.join("p.json")).unwrap(), "{}");
		fs::create_dir_all(root.join("bundles/old")).unwrap();
		prune(&root.join("bundles"), &["d".into()]).unwrap();
		assert!(!root.join("bundles/old").exists());
		assert!(directory.exists());
		let _ = fs::remove_dir_all(&root);
	}
}
