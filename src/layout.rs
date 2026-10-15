//! The two vocabularies, old and new, while both are spoken (the own layout).
//!
//! Our servers took the upstream platform's names for roles, schemas, claims and wire events when
//! they were built as drop-in replacements. The new layout renames them (section 2 of that plan),
//! and until every client has moved (Phase 8) a server accepts BOTH and keeps answering in the old
//! one by default. This file is the one map between them, so "is this the server role?" has one
//! answer in every server.
//!
//! **A twin, byte for byte.** Each server carries this file as its own `src/layout.rs`, because in
//! public each one is its own repository and none may reach outside its folder.
//! `common/src/lib.rs`'s test compares every copy with this one inside the workspace, so an edit
//! here that is not copied fails the build. Edit HERE, then copy.
//!
//! Dependency-free on purpose: plain strings in, plain strings out.

use std::borrow::Cow;

/// Which credential a request carries, whatever the token spells it as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
	/// The public key a page carries: `anon`, now `snout_app`.
	App,
	/// A signed-in person: `authenticated`, now `snout_member`.
	Member,
	/// The owner's server key: `service_role`, now `snout_server`.
	Server,
}

impl Access {
	/// Every kind, in a fixed order.
	pub const ALL: [Access; 3] = [Access::App, Access::Member, Access::Server];

	/// The role's old name, which every database before the rename has.
	pub fn old_name(self) -> &'static str {
		match self {
			Access::App => "anon",
			Access::Member => "authenticated",
			Access::Server => "service_role",
		}
	}

	/// The role's new name.
	pub fn new_name(self) -> &'static str {
		match self {
			Access::App => "snout_app",
			Access::Member => "snout_member",
			Access::Server => "snout_server",
		}
	}

	/// The name to `SET ROLE` to in a database: the new one when that database has it (`has_new`,
	/// resolved at connect with `role_exists_sql`), else the old. A renamed database keeps the old
	/// names as alias roles, so the old name works there too; the new one never works in an
	/// unrenamed database, which is why this asks.
	pub fn in_database(self, has_new: bool) -> &'static str {
		if has_new {
			self.new_name()
		} else {
			self.old_name()
		}
	}
}

/// What a JWT's `role` claim means, in either vocabulary. `None` for any other role (a customer's
/// own role in a custom claim is passed through untouched by the caller).
pub fn access_of(role: &str) -> Option<Access> {
	Access::ALL
		.into_iter()
		.find(|a| a.old_name() == role || a.new_name() == role)
}

/// Is this role claim the owner's server key, in either vocabulary?
pub fn is_server(role: &str) -> bool {
	access_of(role) == Some(Access::Server)
}

/// Do two role names name the same credential? A custom role is the same only as itself.
pub fn same_role(a: &str, b: &str) -> bool {
	match (access_of(a), access_of(b)) {
		(Some(x), Some(y)) => x == y,
		_ => a == b,
	}
}

/// The role to run as in a database where `has_new` says whether the new names exist. A custom
/// role (not one of the three) is returned as it came.
pub fn role_in_database(role: &str, has_new: bool) -> &str {
	match access_of(role) {
		Some(a) => a.in_database(has_new),
		None => role,
	}
}

/// The role a request runs as, as the data API runs it: in a database with the new names, the
/// role the claim names, an old name included (there the old names are alias roles that inherit
/// the new ones, so a grant written to either behaves the same in every service); in one without
/// them, a new name said as the old one, the only one that exists there.
pub fn role_to_run(role: &str, has_new: bool) -> &str {
	if has_new {
		role
	} else {
		role_in_database(role, false)
	}
}

/// One SQL statement, true when the new role names exist in this database. Asked once per pool,
/// at connect, never per request.
pub const ROLE_EXISTS_SQL: &str = "select to_regrole('snout_member') is not null";

/// Schemas, old name then new.
pub const SCHEMAS: [(&str, &str); 5] = [
	("auth", "snout_identity"),
	("storage", "snout_files"),
	("realtime", "snout_live"),
	("graphql_public", "snout_graph"),
	("extensions", "snout_ext"),
];

/// The new name of an old schema, or the old name of a new one.
pub fn other_schema(name: &str) -> Option<&'static str> {
	SCHEMAS.iter().find_map(|(old, new)| {
		if *old == name {
			Some(*new)
		} else if *new == name {
			Some(*old)
		} else {
			None
		}
	})
}

/// One SQL statement taking `$1` = the new schema name and `$2` = the old: the one this database
/// has, the new preferred. Resolved at connect, because one shared server serves renamed and
/// unrenamed projects at once.
pub const RESOLVE_SCHEMA_SQL: &str =
	"select case when to_regnamespace($1) is not null then $1 else $2 end";

/// JWT claims that carry a person's data, old name then new. Issued under BOTH until Phase 8.
pub const CLAIMS: [(&str, &str); 2] = [
	("app_metadata", "server_data"),
	("user_metadata", "profile_data"),
];

/// The claim name in the other vocabulary.
pub fn other_claim(name: &str) -> Option<&'static str> {
	CLAIMS.iter().find_map(|(old, new)| {
		if *old == name {
			Some(*new)
		} else if *new == name {
			Some(*old)
		} else {
			None
		}
	})
}

/// The request headers a key may arrive in, old first. The door copies `Snout-Key` into `apikey`,
/// so a server behind it reads `apikey`; one reached directly reads either.
pub const KEY_HEADERS: [&str; 2] = ["apikey", "snout-key"];

/// The functions' platform variables, old name then new. Both sets are injected until Phase 8.
pub const FUNCTION_ENV: [(&str, &str); 2] = [
	("SNOUTDATA_ANON_KEY", "SNOUTDATA_APP_KEY"),
	("SNOUTDATA_SERVICE_ROLE_KEY", "SNOUTDATA_SERVER_KEY"),
];

/// Realtime's topic prefix, old then new.
pub const TOPIC_PREFIXES: [&str; 2] = ["realtime:", "live:"];

/// Realtime's event names, old then new. Phoenix's own `phx_*` are neither and never change.
pub const EVENTS: [(&str, &str); 5] = [
	("postgres_changes", "changes"),
	("broadcast", "signal"),
	("presence", "roster"),
	("presence_state", "roster_state"),
	("presence_diff", "roster_diff"),
];

/// Which vocabulary a client spoke, so a server answers it in the same one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vocabulary {
	Old,
	New,
}

/// An event name in the old vocabulary, and which vocabulary it came in. A name in neither
/// (`phx_join`, a customer's own broadcast event) is returned as it came, as `Old`.
pub fn event_to_old(name: &str) -> (&str, Vocabulary) {
	match EVENTS.iter().find(|(_, new)| *new == name) {
		Some((old, _)) => (old, Vocabulary::New),
		None => (name, Vocabulary::Old),
	}
}

/// An old event name spoken in `vocabulary`.
pub fn event_in(old: &str, vocabulary: Vocabulary) -> &str {
	match vocabulary {
		Vocabulary::Old => old,
		Vocabulary::New => EVENTS
			.iter()
			.find(|(o, _)| *o == old)
			.map_or(old, |(_, new)| new),
	}
}

/// A topic with its prefix, split: the name after the prefix, and which vocabulary the prefix was.
/// `None` when it carries neither prefix.
pub fn split_topic(topic: &str) -> Option<(&str, Vocabulary)> {
	if let Some(rest) = topic.strip_prefix(TOPIC_PREFIXES[0]) {
		Some((rest, Vocabulary::Old))
	} else {
		topic
			.strip_prefix(TOPIC_PREFIXES[1])
			.map(|rest| (rest, Vocabulary::New))
	}
}

/// A topic name with the prefix of `vocabulary`.
pub fn topic_in(name: &str, vocabulary: Vocabulary) -> String {
	match vocabulary {
		Vocabulary::Old => format!("{}{name}", TOPIC_PREFIXES[0]),
		Vocabulary::New => format!("{}{name}", TOPIC_PREFIXES[1]),
	}
}

/// What a renamed object is, which decides where an unqualified mention of it is renamed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
	Table,
	Function,
	/// The sequence behind a table's identity column, named after the table when it is made.
	Sequence,
}

/// The objects renamed with their schema (section 2b): the old schema, the old name, the new name,
/// and what it is. Everything else in a renamed schema keeps its name, including indexes,
/// triggers, the migration ledgers and realtime's daily partitions (`messages_YYYY_MM_DD`).
/// GUC names (`storage.operation`, `realtime.topic`, ...) are settings, not schema references,
/// and never change.
pub const OBJECTS: [(&str, &str, &str, Kind); 12] = [
	("auth", "users", "people", Kind::Table),
	("auth", "identities", "provider_links", Kind::Table),
	("auth", "uid", "user_id", Kind::Function),
	("auth", "jwt", "claims", Kind::Function),
	("auth", "role", "access", Kind::Function),
	("auth", "email", "user_email", Kind::Function),
	("storage", "buckets", "spaces", Kind::Table),
	("storage", "objects", "files", Kind::Table),
	("realtime", "messages", "signals", Kind::Table),
	("realtime", "subscription", "listeners", Kind::Table),
	// A database made in the new layout names it after `listeners`; one renamed in place renames
	// it to match.
	(
		"realtime",
		"subscription_id_seq",
		"listeners_id_seq",
		Kind::Sequence,
	),
	("realtime", "send", "signal", Kind::Function),
];

/// The roles, old name then new, the login role included. A renamed database keeps the old names
/// as alias roles that inherit the new ones; grants go to the new names.
pub const ROLES: [(&str, &str); 4] = [
	("anon", "snout_app"),
	("authenticated", "snout_member"),
	("service_role", "snout_server"),
	("authenticator", "snout_gate"),
];

/// The new name of an object in an old schema (`renamed("storage", "objects")` is `files`), or
/// `None` when it keeps its name.
pub fn renamed(old_schema: &str, name: &str) -> Option<&'static str> {
	OBJECTS
		.iter()
		.find(|(s, old, _, _)| *s == old_schema && *old == name)
		.map(|(_, _, new, _)| *new)
}

/// An object's name in a database: the new one in a new-layout database, else the old.
pub fn object_in_layout<'a>(old_schema: &str, name: &'a str, new: bool) -> &'a str {
	match renamed(old_schema, name) {
		Some(other) if new => other,
		_ => name,
	}
}

/// The old name of a schema given by either name, or `None` for a schema that is not renamed.
fn old_schema(name: &str) -> Option<&'static str> {
	SCHEMAS
		.iter()
		.find(|(old, new)| *old == name || *new == name)
		.map(|(old, _)| *old)
}

fn new_schema(old: &str) -> Option<&'static str> {
	SCHEMAS.iter().find(|(o, _)| *o == old).map(|(_, new)| *new)
}

/// Which way a statement is spelled over: into the new names (a statement written in the old
/// ones), or back into the old (a migration file, written in the new ones since 2026-10-14).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Toward {
	New,
	Old,
}

impl Toward {
	/// A schema's name on this side, given its old name.
	fn schema(self, old: &'static str) -> &'static str {
		match self {
			Toward::New => new_schema(old).unwrap_or(old),
			Toward::Old => old,
		}
	}

	/// A schema standing alone, renamed when it is one from the other side.
	fn schema_alone(self, name: &str) -> Option<&'static str> {
		match self {
			Toward::New => new_schema(name),
			Toward::Old => SCHEMAS
				.iter()
				.find(|(_, new)| *new == name)
				.map(|(old, _)| *old),
		}
	}

	/// An object of a renamed schema (by its old name), renamed when it is one from the other side.
	fn object(self, old_schema: &str, name: &str) -> Option<&'static str> {
		OBJECTS
			.iter()
			.find(|(s, old, new, _)| {
				*s == old_schema
					&& match self {
						Toward::New => *old == name,
						Toward::Old => *new == name,
					}
			})
			.map(|(_, old, new, _)| match self {
				Toward::New => *new,
				Toward::Old => *old,
			})
	}

	/// A table of a `bare` schema standing alone, renamed when it is one from the other side.
	fn bare_table(self, name: &str, bare: &[&str]) -> Option<&'static str> {
		OBJECTS
			.iter()
			.filter(|(s, _, _, kind)| *kind == Kind::Table && bare.contains(s))
			.find_map(|(_, old, new, _)| match self {
				Toward::New if *old == name => Some(*new),
				Toward::Old if *new == name => Some(*old),
				_ => None,
			})
	}

	/// A role, renamed when it is one from the other side.
	fn role(self, name: &str) -> Option<&'static str> {
		ROLES.iter().find_map(|(old, new)| match self {
			Toward::New if *old == name => Some(*new),
			Toward::Old if *new == name => Some(*old),
			_ => None,
		})
	}
}

/// What `sql_in_layout` renames beyond schema-qualified names.
#[derive(Debug, Clone, Copy, Default)]
pub struct Rewrite<'a> {
	/// The old schemas whose TABLES are also renamed unqualified (a server whose search path is
	/// that schema: `objects` for storage, `users` for auth), and as a literal that is exactly the
	/// name (`relname = 'messages'`).
	pub bare: &'a [&'a str],
	/// Rename the role names (`anon` ...) too, wherever they stand alone: in grants, and in a
	/// literal (`'anon, authenticated, service_role'`, `array['anon']`). For a migration that
	/// grants; never for a statement where `authenticated` may be a value.
	pub roles: bool,
}

/// A statement written in the old names, in the names of a database: unchanged (borrowed) when
/// `new` is false, else with every old schema, renamed object and (as `how` asks) role spelled
/// the new way. Identifiers are matched exactly as written, lowercase or double-quoted, never
/// inside a comment.
///
/// **Literals.** A string literal is translated as SQL (dynamic SQL is the reason, as in
/// `format('grant usage on schema realtime to %I', r)`), with two rules. A literal that is only a
/// dotted name changes only when it names a renamed object (`'storage.objects'`), so a setting
/// (`'storage.operation'`, `'realtime.topic'`) and an operation name (`'storage.object.upload'`)
/// are left as they are; one that is a single word changes when it is a schema, a `bare` table
/// or (`roles`) a role. And inside a literal, prose keeps its words: an unqualified table is never
/// renamed (`'a filter of subscription %'`), and a schema standing alone only right after the word
/// `schema` (`'grant usage on schema realtime to '`, but not `'snout-auth migrations'`). `E'...'`, `B'...'`, `X'...'`, `N'...'` and
/// `U&'...'` literals are left as they are.
pub fn sql_in_layout<'s>(sql: &'s str, new: bool, how: Rewrite<'_>) -> Cow<'s, str> {
	if !new {
		return Cow::Borrowed(sql);
	}
	spelled(sql, how, Toward::New)
}

/// A statement written in the NEW names (every migration file, since 2026-10-14), in the names of
/// a database: unchanged (borrowed) when `new` is true, else with every new schema, renamed object
/// and (as `how` asks) role spelled the old way. The exact inverse of `sql_in_layout`, by the same
/// rules: a database in the old layout (one made before the rename, until `rename.sql` runs on it)
/// runs the statements it always ran, whichever names the file is written in.
pub fn sql_from_new<'s>(sql: &'s str, new: bool, how: Rewrite<'_>) -> Cow<'s, str> {
	if new {
		return Cow::Borrowed(sql);
	}
	spelled(sql, how, Toward::Old)
}

fn spelled<'s>(sql: &'s str, how: Rewrite<'_>, toward: Toward) -> Cow<'s, str> {
	let out = rewrite(sql, how, false, toward);
	if out == sql {
		Cow::Borrowed(sql)
	} else {
		Cow::Owned(out)
	}
}

fn ident_start(c: u8) -> bool {
	c.is_ascii_alphabetic() || c == b'_' || c >= 0x80
}

fn ident_char(c: u8) -> bool {
	ident_start(c) || c.is_ascii_digit() || c == b'$'
}

/// One name: its text as written, its value, and whether it was double-quoted.
struct Part<'s> {
	value: Cow<'s, str>,
	quoted: bool,
}

impl Part<'_> {
	fn write(&self, out: &mut String, name: &str) {
		if self.quoted {
			out.push('"');
			out.push_str(&name.replace('"', "\"\""));
			out.push('"');
		} else {
			out.push_str(name);
		}
	}
}

/// A name at `i` (unquoted or double-quoted): the part, and the index after it.
fn name_at(sql: &str, i: usize) -> Option<(Part<'_>, usize)> {
	let b = sql.as_bytes();
	match b.get(i) {
		Some(b'"') => {
			let mut j = i + 1;
			loop {
				match b.get(j) {
					None => return None,
					Some(b'"') if b.get(j + 1) == Some(&b'"') => j += 2,
					Some(b'"') => break,
					Some(_) => j += 1,
				}
			}
			let raw = &sql[i + 1..j];
			let value = if raw.contains("\"\"") {
				Cow::Owned(raw.replace("\"\"", "\""))
			} else {
				Cow::Borrowed(raw)
			};
			Some((
				Part {
					value,
					quoted: true,
				},
				j + 1,
			))
		}
		Some(&c) if ident_start(c) => {
			let mut j = i + 1;
			while j < b.len() && ident_char(b[j]) {
				j += 1;
			}
			Some((
				Part {
					value: Cow::Borrowed(&sql[i..j]),
					quoted: false,
				},
				j,
			))
		}
		_ => None,
	}
}

/// The index just after a single-quoted literal that opens at `i` (the quote), or `None` when it
/// never closes. `backslash`: an `E'...'` literal, where `\'` does not close it.
fn literal_end(b: &[u8], i: usize, backslash: bool) -> Option<usize> {
	let mut j = i + 1;
	while j < b.len() {
		match b[j] {
			b'\\' if backslash => j += 2,
			b'\'' if b.get(j + 1) == Some(&b'\'') => j += 2,
			b'\'' => return Some(j + 1),
			_ => j += 1,
		}
	}
	None
}

/// A dollar quote opening at `i`: the tag's length and the index of the closing tag.
fn dollar_quote(sql: &str, i: usize) -> Option<(usize, usize)> {
	let b = sql.as_bytes();
	let mut j = i + 1;
	if j < b.len() && ident_start(b[j]) {
		j += 1;
		while j < b.len() && (ident_char(b[j]) && b[j] != b'$') {
			j += 1;
		}
	}
	if b.get(j) != Some(&b'$') {
		return None;
	}
	let tag = &sql[i..=j];
	sql[j + 1..]
		.find(tag)
		.map(|close| (tag.len(), j + 1 + close))
}

/// A single name standing alone, renamed: a schema (when `schemas`), a `bare` table (when
/// `tables`), or a role (when `how.roles`), each when it is a name from the other side.
fn alone(
	name: &str,
	how: Rewrite<'_>,
	schemas: bool,
	tables: bool,
	toward: Toward,
) -> Option<&'static str> {
	if schemas && let Some(other) = toward.schema_alone(name) {
		return Some(other);
	}
	if tables && let Some(other) = toward.bare_table(name, how.bare) {
		return Some(other);
	}
	if how.roles {
		return toward.role(name);
	}
	None
}

fn translate_literal(raw: &str, how: Rewrite<'_>, toward: Toward) -> String {
	let text = raw.replace("''", "'");
	let dotted = !text.is_empty()
		&& text.split('.').all(|part| {
			part.bytes()
				.next()
				.is_some_and(|c| ident_char(c) && c != b'$')
				&& part.bytes().all(ident_char)
		});
	let out = if dotted {
		let parts: Vec<&str> = text.split('.').collect();
		match parts.as_slice() {
			[one] => alone(one, how, true, true, toward).map_or(text.clone(), str::to_string),
			[schema, name] => match old_schema(schema)
				.and_then(|old| Some((toward.schema(old), toward.object(old, name)?)))
			{
				Some((schema, name)) => format!("{schema}.{name}"),
				None => text.clone(),
			},
			_ => text.clone(),
		}
	} else {
		rewrite(&text, how, true, toward)
	};
	out.replace('\'', "''")
}

fn rewrite(sql: &str, how: Rewrite<'_>, in_literal: bool, toward: Toward) -> String {
	let b = sql.as_bytes();
	let mut out = String::with_capacity(sql.len() + 16);
	let mut i = 0;
	// The byte before the current token was a `.`: a name there is a field, never renamed.
	let mut after_dot = false;
	// The word before this one was `schema`: inside a literal, the only place a schema standing
	// alone is renamed (`'grant usage on schema realtime to '`), so prose keeps its words
	// (`'snout-auth migrations'`, `'Direct deletion from storage tables'`).
	let mut after_schema = false;
	while i < b.len() {
		let c = b[i];
		if c == b'-' && b.get(i + 1) == Some(&b'-') {
			let end = sql[i..].find('\n').map_or(b.len(), |n| i + n);
			out.push_str(&sql[i..end]);
			i = end;
		} else if c == b'/' && b.get(i + 1) == Some(&b'*') {
			let mut depth = 0usize;
			let mut j = i;
			while j < b.len() {
				if b[j] == b'/' && b.get(j + 1) == Some(&b'*') {
					depth += 1;
					j += 2;
				} else if b[j] == b'*' && b.get(j + 1) == Some(&b'/') {
					depth -= 1;
					j += 2;
					if depth == 0 {
						break;
					}
				} else {
					j += 1;
				}
			}
			out.push_str(&sql[i..j]);
			i = j;
		} else if c == b'\'' {
			match literal_end(b, i, false) {
				Some(end) => {
					out.push('\'');
					out.push_str(&translate_literal(&sql[i + 1..end - 1], how, toward));
					out.push('\'');
					i = end;
				}
				None => {
					out.push_str(&sql[i..]);
					i = b.len();
				}
			}
		} else if c == b'$' {
			if let Some((tag, close)) = dollar_quote(sql, i) {
				out.push_str(&sql[i..i + tag]);
				out.push_str(&rewrite(&sql[i + tag..close], how, in_literal, toward));
				out.push_str(&sql[close..close + tag]);
				i = close + tag;
			} else {
				let mut j = i + 1;
				while j < b.len() && b[j].is_ascii_digit() {
					j += 1;
				}
				out.push_str(&sql[i..j]);
				i = j;
			}
		} else if c.is_ascii_digit() {
			let mut j = i + 1;
			while j < b.len() && ident_char(b[j]) {
				j += 1;
			}
			out.push_str(&sql[i..j]);
			i = j;
		} else if let Some((first, after)) = name_at(sql, i) {
			// A one-letter prefix of a special literal (`E'...'` and the like): left as written.
			let prefix = !first.quoted
				&& matches!(&*first.value, "E" | "e" | "B" | "b" | "X" | "x" | "N" | "n");
			if prefix && b.get(after) == Some(&b'\'') {
				let end =
					literal_end(b, after, matches!(&*first.value, "E" | "e")).unwrap_or(b.len());
				out.push_str(&sql[i..end]);
				i = end;
				after_dot = false;
				continue;
			}
			if !first.quoted
				&& matches!(&*first.value, "U" | "u")
				&& b.get(after) == Some(&b'&')
				&& b.get(after + 1) == Some(&b'\'')
			{
				let end = literal_end(b, after + 1, false).unwrap_or(b.len());
				out.push_str(&sql[i..end]);
				i = end;
				after_dot = false;
				continue;
			}
			let qualifies = b.get(after) == Some(&b'.');
			if after_dot {
				out.push_str(&sql[i..after]);
				i = after;
			} else if qualifies && let Some(old) = old_schema(&first.value) {
				// `schema.name`: the schema, and the name when it is renamed with it.
				first.write(&mut out, toward.schema(old));
				out.push('.');
				i = after + 1;
				if let Some((second, next)) = name_at(sql, i) {
					match toward.object(old, &second.value) {
						Some(other) => second.write(&mut out, other),
						None => out.push_str(&sql[i..next]),
					}
					i = next;
				}
			} else {
				// A schema standing alone (`create schema storage`), a `bare` table outside a literal
				// (`from objects`, `objects.name`), or a role.
				match alone(
					&first.value,
					how,
					!in_literal || after_schema,
					!in_literal,
					toward,
				) {
					Some(other) => first.write(&mut out, other),
					None => out.push_str(&sql[i..after]),
				}
				i = after;
			}
			after_dot = false;
			after_schema = !first.quoted && first.value.eq_ignore_ascii_case("schema");
			continue;
		} else {
			let ch = sql[i..].chars().next().unwrap_or('\u{fffd}');
			out.push(ch);
			i += ch.len_utf8().max(1);
			if !ch.is_whitespace() {
				after_schema = false;
			}
			after_dot = c == b'.';
			continue;
		}
		after_dot = c == b'.';
		// Comments keep what came before; a literal, a quote or a number is a token of its own.
		if !(c == b'-' || c == b'/') {
			after_schema = false;
		}
	}
	out
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn both_role_names_mean_the_same_credential() {
		for a in Access::ALL {
			assert_eq!(access_of(a.old_name()), Some(a));
			assert_eq!(access_of(a.new_name()), Some(a));
			assert!(same_role(a.old_name(), a.new_name()));
		}
		assert!(is_server("service_role"));
		assert!(is_server("snout_server"));
		assert!(!is_server("snout_member"));
		assert!(!is_server("authenticated"));
		assert_eq!(access_of("my_custom_role"), None);
		assert!(!same_role("anon", "authenticated"));
		assert!(same_role("my_custom_role", "my_custom_role"));
	}

	#[test]
	fn a_database_is_given_the_name_it_has() {
		assert_eq!(role_in_database("snout_member", false), "authenticated");
		assert_eq!(role_in_database("authenticated", true), "snout_member");
		assert_eq!(role_in_database("anon", false), "anon");
		assert_eq!(role_in_database("snout_app", true), "snout_app");
		assert_eq!(role_in_database("my_custom_role", true), "my_custom_role");
	}

	#[test]
	fn a_request_runs_as_the_role_its_claim_names() {
		for role in [
			"anon",
			"authenticated",
			"service_role",
			"snout_app",
			"snout_member",
			"snout_server",
			"my_role",
		] {
			assert_eq!(role_to_run(role, true), role);
		}
		assert_eq!(role_to_run("snout_member", false), "authenticated");
		assert_eq!(role_to_run("snout_server", false), "service_role");
		assert_eq!(role_to_run("anon", false), "anon");
		assert_eq!(role_to_run("my_role", false), "my_role");
	}

	#[test]
	fn schemas_and_claims_map_both_ways() {
		assert_eq!(other_schema("storage"), Some("snout_files"));
		assert_eq!(other_schema("snout_live"), Some("realtime"));
		assert_eq!(other_schema("public"), None);
		assert_eq!(other_claim("app_metadata"), Some("server_data"));
		assert_eq!(other_claim("profile_data"), Some("user_metadata"));
		assert_eq!(other_claim("role"), None);
	}

	#[test]
	fn events_and_topics_round_trip() {
		for (old, new) in EVENTS {
			assert_eq!(event_to_old(new), (old, Vocabulary::New));
			assert_eq!(event_to_old(old), (old, Vocabulary::Old));
			assert_eq!(event_in(old, Vocabulary::New), new);
			assert_eq!(event_in(old, Vocabulary::Old), old);
		}
		assert_eq!(event_to_old("phx_join"), ("phx_join", Vocabulary::Old));
		assert_eq!(event_in("phx_reply", Vocabulary::New), "phx_reply");
		assert_eq!(
			split_topic("realtime:room"),
			Some(("room", Vocabulary::Old))
		);
		assert_eq!(split_topic("live:room"), Some(("room", Vocabulary::New)));
		assert_eq!(split_topic("room"), None);
		assert_eq!(topic_in("room", Vocabulary::New), "live:room");
		assert_eq!(topic_in("room", Vocabulary::Old), "realtime:room");
	}

	fn new(sql: &str, bare: &[&str], roles: bool) -> String {
		sql_in_layout(sql, true, Rewrite { bare, roles }).into_owned()
	}

	#[test]
	fn an_old_database_gets_the_text_unchanged() {
		let sql = "select * from storage.objects";
		assert!(matches!(
			sql_in_layout(sql, false, Rewrite::default()),
			Cow::Borrowed(_)
		));
		assert!(matches!(
			sql_in_layout("select 1 from public.t", true, Rewrite::default()),
			Cow::Borrowed(_)
		));
	}

	#[test]
	fn qualified_names_take_the_new_schema_and_the_new_object() {
		let none = Rewrite::default();
		assert_eq!(
			sql_in_layout(
				"insert into realtime.messages (topic) select x from realtime.subscription",
				true,
				none
			),
			"insert into snout_live.signals (topic) select x from snout_live.listeners"
		);
		assert_eq!(
			new(
				"b::realtime.equality_op, realtime.handlers, realtime.topic(), snout_realtime.migrations",
				&[],
				false
			),
			"b::snout_live.equality_op, snout_live.handlers, snout_live.topic(), snout_realtime.migrations"
		);
		assert_eq!(
			new(
				"select auth.uid(), auth.jwt(), auth.role(), auth.email(), auth.users.email from auth.identities",
				&[],
				false
			),
			"select snout_identity.user_id(), snout_identity.claims(), snout_identity.access(), snout_identity.user_email(), snout_identity.people.email from snout_identity.provider_links"
		);
		assert_eq!(
			new(
				"perform realtime.send(p, e, t); grant usage on sequence realtime.subscription_id_seq to x",
				&[],
				false
			),
			"perform snout_live.signal(p, e, t); grant usage on sequence snout_live.listeners_id_seq to x"
		);
		// A name already in the new schema still takes the new object name (snout-auth templates
		// its schema in).
		assert_eq!(
			new("references snout_identity.users(id)", &[], false),
			"references snout_identity.people(id)"
		);
		assert_eq!(
			new(
				r#"select * from "storage"."objects", "storage".foldername(n)"#,
				&[],
				false
			),
			r#"select * from "snout_files"."files", "snout_files".foldername(n)"#
		);
		assert_eq!(
			new(
				"set search_path to storage, public, extensions; create schema if not exists realtime; grant usage on schema graphql_public to x",
				&[],
				false
			),
			"set search_path to snout_files, public, snout_ext; create schema if not exists snout_live; grant usage on schema snout_graph to x"
		);
	}

	#[test]
	fn bare_tables_only_for_the_schemas_asked() {
		let storage = &["storage"][..];
		assert_eq!(
			new(
				"SELECT 1 FROM objects o JOIN buckets b ON b.id = o.bucket_id",
				storage,
				false
			),
			"SELECT 1 FROM files o JOIN spaces b ON b.id = o.bucket_id"
		);
		assert_eq!(
			new(
				"INSERT INTO objects (name) VALUES ($1) ON CONFLICT (bucket_id, name) DO UPDATE SET user_metadata = objects.user_metadata",
				storage,
				false
			),
			"INSERT INTO files (name) VALUES ($1) ON CONFLICT (bucket_id, name) DO UPDATE SET user_metadata = files.user_metadata"
		);
		assert_eq!(
			new(
				"select * from (select * from buckets) buckets",
				storage,
				false
			),
			"select * from (select * from spaces) spaces"
		);
		assert_eq!(
			new("select 1 from users", storage, false),
			"select 1 from users"
		);
		assert_eq!(
			new(
				"update users set x = 1; delete from identities; select * from scim_users",
				&["auth"],
				false
			),
			"update people set x = 1; delete from provider_links; select * from scim_users"
		);
		// A field after a dot is a column, never a table; a function name is never renamed bare.
		assert_eq!(
			new(
				"select (r).messages, x.objects, role, email, uid from t",
				&["realtime", "storage", "auth"],
				false
			),
			"select (r).messages, x.objects, role, email, uid from t"
		);
	}

	#[test]
	fn settings_and_values_in_literals_keep_their_names() {
		let all = &["storage", "realtime", "auth"][..];
		for sql in [
			"select set_config('storage.operation', $1, true), set_config('storage.allow_delete_query', 'true', true)",
			"select current_setting('realtime.topic', true), current_setting('storage.anon_role', true)",
			"select storage.operation() = 'storage.object.upload'",
			"select 'send', claims ->> 'role', 'email', 'broadcast', 'messages_' || 'x', '1 second', '{}'",
			"raise warning 'snout_realtime: a filter of subscription % could not be evaluated: %', a, b",
			"select hashtext('snout_realtime.prepare'), 'request.jwt.claims', ''",
			"select pg_advisory_xact_lock(hashtext('snout-auth migrations'))",
			"raise exception 'Direct deletion from storage tables is not allowed. Use the Storage API instead.'",
			"raise exception 'the realtime schema, -- not a comment, /* nor this */ auth'",
		] {
			let expected = sql.replace("storage.operation()", "snout_files.operation()");
			assert_eq!(new(sql, all, true), expected, "{sql}");
		}
	}

	#[test]
	fn literals_that_are_names_or_sql_are_translated() {
		let realtime = &["realtime"][..];
		assert_eq!(
			new(
				"select to_regclass('realtime.messages') is not null, to_regprocedure('snout_identity.uid()')",
				realtime,
				false
			),
			"select to_regclass('snout_live.signals') is not null, to_regprocedure('snout_identity.user_id()')"
		);
		assert_eq!(
			new(
				r"where p.relname = 'messages' and n.nspname = 'realtime' and c.relname ~ '^messages_\d{4}_\d{2}_\d{2}$'",
				realtime,
				false
			),
			r"where p.relname = 'signals' and n.nspname = 'snout_live' and c.relname ~ '^messages_\d{4}_\d{2}_\d{2}$'"
		);
		assert_eq!(
			new(
				"execute format('create table if not exists realtime.%I partition of realtime.messages for values from (%L) to (%L)', 'messages_' || d)",
				realtime,
				false
			),
			"execute format('create table if not exists snout_live.%I partition of snout_live.signals for values from (%L) to (%L)', 'messages_' || d)"
		);
		// A literal inside a literal: the setting stays, the table moves.
		assert_eq!(
			new(
				"execute 'select set_config(''realtime.topic'', ''x'', true) from realtime.messages'",
				realtime,
				false
			),
			"execute 'select set_config(''realtime.topic'', ''x'', true) from snout_live.signals'"
		);
		// Prose names the schema it is about; its bare words stay.
		assert_eq!(
			new(
				"raise exception 'realtime.on: an event name is required for messages'",
				realtime,
				false
			),
			"raise exception 'snout_live.on: an event name is required for messages'"
		);
		// A schema standing alone in dynamic SQL, after the word `schema` (comments between are
		// fine; anything else is not).
		assert_eq!(
			new(
				"execute format('GRANT USAGE ON SCHEMA storage TO %I', r); execute 'grant usage on schema /* x */ realtime to ' || g; x := 'schema, storage'",
				realtime,
				false
			),
			"execute format('GRANT USAGE ON SCHEMA snout_files TO %I', r); execute 'grant usage on schema /* x */ snout_live to ' || g; x := 'schema, storage'"
		);
	}

	#[test]
	fn roles_only_when_asked() {
		let grants = "grantee text := 'anon, authenticated, service_role' || ', postgres'; \
			execute 'grant usage on schema realtime to ' || grantee; \
			foreach r in array array['anon', 'authenticated', 'service_role', 'postgres'] loop end loop; \
			x := coalesce(nullif(current_setting('storage.anon_role', true), ''), 'anon'); grant r to authenticator";
		assert_eq!(
			new(grants, &[], true),
			"grantee text := 'snout_app, snout_member, snout_server' || ', postgres'; \
			execute 'grant usage on schema snout_live to ' || grantee; \
			foreach r in array array['snout_app', 'snout_member', 'snout_server', 'postgres'] loop end loop; \
			x := coalesce(nullif(current_setting('storage.anon_role', true), ''), 'snout_app'); grant r to snout_gate"
		);
		assert_eq!(
			new(
				"insert into users (aud, role) values ('authenticated', 'authenticated')",
				&["auth"],
				false
			),
			"insert into people (aud, role) values ('authenticated', 'authenticated')"
		);
	}

	#[test]
	fn comments_dollar_quotes_and_special_literals() {
		let sql = "-- realtime.messages stays in a comment\n\
			/* storage.objects /* nested */ too */ \
			create function realtime.f() returns void language plpgsql as $body$ \
			begin insert into realtime.messages values ($1); -- realtime.subscription\n end $body$; \
			select $$ storage.objects $$, $1, $tag$ 'realtime.messages' $tag$, \
			E'storage.objects \\' realtime.messages', X'00', U&'storage.objects'";
		assert_eq!(
			new(sql, &[], false),
			"-- realtime.messages stays in a comment\n\
			/* storage.objects /* nested */ too */ \
			create function snout_live.f() returns void language plpgsql as $body$ \
			begin insert into snout_live.signals values ($1); -- realtime.subscription\n end $body$; \
			select $$ snout_files.files $$, $1, $tag$ 'snout_live.signals' $tag$, \
			E'storage.objects \\' realtime.messages', X'00', U&'storage.objects'"
		);
	}

	#[test]
	fn odd_text_is_copied_not_broken() {
		for sql in [
			"select 'unclosed storage.objects",
			"select \"unclosed realtime.messages",
			"select $tag$ never closed storage.objects",
			"/* never closed storage.objects",
			"select 'naïve café' from realtime.messages ü",
			"select a$b, 1e5, 1.5, x.y.z, %I, $",
			"",
		] {
			let out = new(sql, &["storage", "realtime"], true);
			assert_eq!(new(&out, &["storage", "realtime"], true), out, "{sql}");
		}
		assert_eq!(
			new("select 'naïve' from realtime.messages ü", &[], false),
			"select 'naïve' from snout_live.signals ü"
		);
		assert_eq!(
			new("select 'unclosed storage.objects", &[], false),
			"select 'unclosed storage.objects"
		);
	}

	#[test]
	fn a_file_in_the_new_names_is_spelled_back_for_an_old_database() {
		let how = Rewrite {
			bare: &["storage"],
			roles: true,
		};
		let file = "create table snout_files.spaces (id text); create table files (bucket_id text references spaces (id)); \
			grant usage on schema snout_files to snout_app, snout_member, snout_server; \
			select to_regclass('snout_files.files'), snout_identity.user_id(), set_config('storage.anon_role', 'snout_app', false); \
			-- snout_files.files stays in a comment\n\
			execute format('grant usage on schema snout_live to %I', 'snout_gate')";
		assert!(matches!(sql_from_new(file, true, how), Cow::Borrowed(_)));
		assert_eq!(
			sql_from_new(file, false, how),
			"create table storage.buckets (id text); create table objects (bucket_id text references buckets (id)); \
			grant usage on schema storage to anon, authenticated, service_role; \
			select to_regclass('storage.objects'), auth.uid(), set_config('storage.anon_role', 'anon', false); \
			-- snout_files.files stays in a comment\n\
			execute format('grant usage on schema realtime to %I', 'authenticator')"
		);
		// A function of a bare schema is never renamed standing alone, either way.
		assert_eq!(
			sql_from_new(
				"select user_id, people from people",
				false,
				Rewrite {
					bare: &["auth"],
					roles: false
				}
			),
			"select user_id, users from users"
		);
	}

	#[test]
	fn spelling_back_is_the_inverse_of_spelling_over() {
		let all = &["storage", "realtime", "auth"][..];
		for (sql, bare, roles) in [
			(
				"insert into realtime.messages (topic) select x from realtime.subscription",
				&[][..],
				false,
			),
			(
				"select auth.uid(), auth.jwt(), auth.role(), auth.email(), auth.users.email from auth.identities",
				&[],
				false,
			),
			(
				"perform realtime.send(p, e, t); grant usage on sequence realtime.subscription_id_seq to x",
				&[],
				false,
			),
			(
				r#"select * from "storage"."objects", "storage".foldername(n)"#,
				&[],
				false,
			),
			(
				"set search_path to storage, public, extensions; create schema if not exists realtime; grant usage on schema graphql_public to x",
				&[],
				false,
			),
			(
				"SELECT 1 FROM objects o JOIN buckets b ON b.id = o.bucket_id",
				&["storage"],
				false,
			),
			(
				"update users set x = 1; delete from identities; select * from scim_users",
				&["auth"],
				false,
			),
			(
				"select current_setting('realtime.topic', true), current_setting('storage.anon_role', true)",
				all,
				true,
			),
			(
				"select to_regclass('realtime.messages') is not null, relname = 'messages', nspname = 'realtime'",
				&["realtime"],
				false,
			),
			(
				"execute 'select set_config(''realtime.topic'', ''x'', true) from realtime.messages'",
				&["realtime"],
				false,
			),
			(
				"execute format('GRANT USAGE ON SCHEMA storage TO %I', r); x := 'schema, storage'",
				&[],
				false,
			),
			(
				"grantee text := 'anon, authenticated, service_role'; foreach r in array array['anon', 'postgres'] loop end loop; grant r to authenticator",
				&[],
				true,
			),
			(
				"-- realtime.messages\nselect $$ storage.objects $$, E'storage.objects', U&'storage.objects'",
				&[],
				false,
			),
		] {
			let how = Rewrite { bare, roles };
			let over = sql_in_layout(sql, true, how);
			assert_eq!(sql_from_new(&over, false, how), sql, "{sql}");
		}
	}
}
