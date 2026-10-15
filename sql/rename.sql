-- rename.sql: the own layout, in place.
--
-- GENERATED from the layout map; edit the generator, never this.
-- Old layout to new.
-- Run as the superuser in ONE transaction (psql -1 -v ON_ERROR_STOP=1); it commits nothing itself,
-- and any refusal or a non-empty scan leaves the database exactly as it was.

select set_config('lock_timeout', '5s', true);

-- Another session holding anything in the schemas this renames would either block the rename past
-- the lock timeout or keep using a name that is about to change under it.
create function pg_temp.own_layout_locks() returns void
	language plpgsql
as $own$
declare
	held text;
begin
	select string_agg(distinct format('%s (pid %s)', l.relation::regclass, l.pid), ', ')
	into held
	from pg_locks l
	join pg_class c on c.oid = l.relation
	join pg_namespace n on n.oid = c.relnamespace
	where l.database = (select oid from pg_database where datname = current_database())
		and l.pid <> pg_backend_pid()
		and l.granted
		and n.nspname in ('auth', 'storage', 'realtime', 'graphql_public', 'extensions');
	if held is not null then
		raise exception 'own-layout: other sessions hold %; nothing was changed', held;
	end if;
end
$own$;

-- The text rewrite, one rule per line (rename.ts, "The text rule").
create function pg_temp.own_layout_text(t text) returns text
	language plpgsql immutable
as $own$
declare
	r text := t;
begin
	if r is null then
		return null;
	end if;
	r := regexp_replace(r, $re$\mauth\.uid\($re$, $re$snout_identity.user_id($re$, 'g');
	r := regexp_replace(r, $re$\mauth\.jwt\($re$, $re$snout_identity.claims($re$, 'g');
	r := regexp_replace(r, $re$\mauth\.role\($re$, $re$snout_identity.access($re$, 'g');
	r := regexp_replace(r, $re$\mauth\.email\($re$, $re$snout_identity.user_email($re$, 'g');
	r := regexp_replace(r, $re$\mrealtime\.send\($re$, $re$snout_live.signal($re$, 'g');
	r := regexp_replace(r, $re$\mauth\.users_([A-Za-z0-9_]*_seq)\M$re$, $re$snout_identity.people_\1$re$, 'g');
	r := regexp_replace(r, $re$\mauth\.users\M$re$, $re$snout_identity.people$re$, 'g');
	r := regexp_replace(r, $re$\mauth\.identities_([A-Za-z0-9_]*_seq)\M$re$, $re$snout_identity.provider_links_\1$re$, 'g');
	r := regexp_replace(r, $re$\mauth\.identities\M$re$, $re$snout_identity.provider_links$re$, 'g');
	r := regexp_replace(r, $re$\mstorage\.buckets_([A-Za-z0-9_]*_seq)\M$re$, $re$snout_files.spaces_\1$re$, 'g');
	r := regexp_replace(r, $re$\mstorage\.buckets\M$re$, $re$snout_files.spaces$re$, 'g');
	r := regexp_replace(r, $re$\mstorage\.objects_([A-Za-z0-9_]*_seq)\M$re$, $re$snout_files.files_\1$re$, 'g');
	r := regexp_replace(r, $re$\mstorage\.objects\M$re$, $re$snout_files.files$re$, 'g');
	r := regexp_replace(r, $re$\mrealtime\.messages_([A-Za-z0-9_]*_seq)\M$re$, $re$snout_live.signals_\1$re$, 'g');
	r := regexp_replace(r, $re$\mrealtime\.messages\M$re$, $re$snout_live.signals$re$, 'g');
	r := regexp_replace(r, $re$\mrealtime\.subscription_([A-Za-z0-9_]*_seq)\M$re$, $re$snout_live.listeners_\1$re$, 'g');
	r := regexp_replace(r, $re$\mrealtime\.subscription\M$re$, $re$snout_live.listeners$re$, 'g');
	r := regexp_replace(r, $re$(^|[^'"A-Za-z0-9_.$])auth\.(?=[A-Za-z_])$re$, $re$\1snout_identity.$re$, 'g');
	r := regexp_replace(r, $re$'auth\.(?![A-Za-z0-9_]+\.)(?=[A-Za-z_])$re$, $re$'snout_identity.$re$, 'g');
	r := regexp_replace(r, $re$(\m(nspname|schema_name|table_schema|schemaname)\s*=\s*)'auth'$re$, $re$\1'snout_identity'$re$, 'g');
	r := regexp_replace(r, $re$(\mto_regnamespace\s*\(\s*)'auth'$re$, $re$\1'snout_identity'$re$, 'g');
	r := regexp_replace(r, $re$(\mhas_schema_privilege\s*\(\s*(('[^']*'|[^()',]*)\s*,\s*)?)'auth'$re$, $re$\1'snout_identity'$re$, 'g');
	r := regexp_replace(r, $re$(\m[Ss][Cc][Hh][Ee][Mm][Aa]\s+)auth\M$re$, $re$\1snout_identity$re$, 'g');
	r := regexp_replace(r, $re$(^|[^'"A-Za-z0-9_.$])storage\.(?=[A-Za-z_])$re$, $re$\1snout_files.$re$, 'g');
	r := regexp_replace(r, $re$'storage\.(?!(operation|allow_delete_query|anon_role|authenticated_role|service_role|super_user|install_roles)')(?![A-Za-z0-9_]+\.)(?=[A-Za-z_])$re$, $re$'snout_files.$re$, 'g');
	r := regexp_replace(r, $re$(\m(nspname|schema_name|table_schema|schemaname)\s*=\s*)'storage'$re$, $re$\1'snout_files'$re$, 'g');
	r := regexp_replace(r, $re$(\mto_regnamespace\s*\(\s*)'storage'$re$, $re$\1'snout_files'$re$, 'g');
	r := regexp_replace(r, $re$(\mhas_schema_privilege\s*\(\s*(('[^']*'|[^()',]*)\s*,\s*)?)'storage'$re$, $re$\1'snout_files'$re$, 'g');
	r := regexp_replace(r, $re$(\m[Ss][Cc][Hh][Ee][Mm][Aa]\s+)storage\M$re$, $re$\1snout_files$re$, 'g');
	r := regexp_replace(r, $re$(^|[^'"A-Za-z0-9_.$])realtime\.(?=[A-Za-z_])$re$, $re$\1snout_live.$re$, 'g');
	r := regexp_replace(r, $re$'realtime\.(?!(topic|event|args|socket_id|out)')(?![A-Za-z0-9_]+\.)(?=[A-Za-z_])$re$, $re$'snout_live.$re$, 'g');
	r := regexp_replace(r, $re$(\m(nspname|schema_name|table_schema|schemaname)\s*=\s*)'realtime'$re$, $re$\1'snout_live'$re$, 'g');
	r := regexp_replace(r, $re$(\mto_regnamespace\s*\(\s*)'realtime'$re$, $re$\1'snout_live'$re$, 'g');
	r := regexp_replace(r, $re$(\mhas_schema_privilege\s*\(\s*(('[^']*'|[^()',]*)\s*,\s*)?)'realtime'$re$, $re$\1'snout_live'$re$, 'g');
	r := regexp_replace(r, $re$(\m[Ss][Cc][Hh][Ee][Mm][Aa]\s+)realtime\M$re$, $re$\1snout_live$re$, 'g');
	r := regexp_replace(r, $re$(^|[^'"A-Za-z0-9_.$])graphql_public\.(?=[A-Za-z_])$re$, $re$\1snout_graph.$re$, 'g');
	r := regexp_replace(r, $re$'graphql_public\.(?![A-Za-z0-9_]+\.)(?=[A-Za-z_])$re$, $re$'snout_graph.$re$, 'g');
	r := regexp_replace(r, $re$(\m(nspname|schema_name|table_schema|schemaname)\s*=\s*)'graphql_public'$re$, $re$\1'snout_graph'$re$, 'g');
	r := regexp_replace(r, $re$(\mto_regnamespace\s*\(\s*)'graphql_public'$re$, $re$\1'snout_graph'$re$, 'g');
	r := regexp_replace(r, $re$(\mhas_schema_privilege\s*\(\s*(('[^']*'|[^()',]*)\s*,\s*)?)'graphql_public'$re$, $re$\1'snout_graph'$re$, 'g');
	r := regexp_replace(r, $re$(\m[Ss][Cc][Hh][Ee][Mm][Aa]\s+)graphql_public\M$re$, $re$\1snout_graph$re$, 'g');
	r := regexp_replace(r, $re$(^|[^'"A-Za-z0-9_.$])extensions\.(?=[A-Za-z_])$re$, $re$\1snout_ext.$re$, 'g');
	r := regexp_replace(r, $re$'extensions\.(?![A-Za-z0-9_]+\.)(?=[A-Za-z_])$re$, $re$'snout_ext.$re$, 'g');
	r := regexp_replace(r, $re$(\m(nspname|schema_name|table_schema|schemaname)\s*=\s*)'extensions'$re$, $re$\1'snout_ext'$re$, 'g');
	r := regexp_replace(r, $re$(\mto_regnamespace\s*\(\s*)'extensions'$re$, $re$\1'snout_ext'$re$, 'g');
	r := regexp_replace(r, $re$(\mhas_schema_privilege\s*\(\s*(('[^']*'|[^()',]*)\s*,\s*)?)'extensions'$re$, $re$\1'snout_ext'$re$, 'g');
	r := regexp_replace(r, $re$(\m[Ss][Cc][Hh][Ee][Mm][Aa]\s+)extensions\M$re$, $re$\1snout_ext$re$, 'g');
	r := regexp_replace(r, $re$(['"])anon\1$re$, $re$\1snout_app\1$re$, 'g');
	r := regexp_replace(r, $re$(\m([Tt][Oo]|[Ff][Rr][Oo][Mm])\s+|,\s*)anon(?=\s*([,;)']|$))$re$, $re$\1snout_app$re$, 'g');
	r := regexp_replace(r, $re$(['"])authenticated\1$re$, $re$\1snout_member\1$re$, 'g');
	r := regexp_replace(r, $re$(\m([Tt][Oo]|[Ff][Rr][Oo][Mm])\s+|,\s*)authenticated(?=\s*([,;)']|$))$re$, $re$\1snout_member$re$, 'g');
	r := regexp_replace(r, $re$(^|[^.A-Za-z0-9_$])service_role\M$re$, $re$\1snout_server$re$, 'g');
	r := regexp_replace(r, $re$(^|[^.A-Za-z0-9_$])authenticator\M$re$, $re$\1snout_gate$re$, 'g');
	r := regexp_replace(r, $re$(->>?\s*)'app_metadata'$re$, $re$\1'server_data'$re$, 'g');
	r := regexp_replace(r, $re$(->>?\s*)'user_metadata'$re$, $re$\1'profile_data'$re$, 'g');
	return r;
end
$own$;

-- A role's name in the other layout, or null when it is not one of ours.
create function pg_temp.own_layout_role(name text) returns text
	language sql immutable
as $own$
	select case name when 'anon' then 'snout_app' when 'authenticated' then 'snout_member' when 'service_role' then 'snout_server' when 'authenticator' then 'snout_gate' else null end
$own$;

-- A setting's value with schema and role names in this direction: list settings element by element
-- ("$user" and anything else not ours left as written), the data API's anonymous role as a whole.
create function pg_temp.own_layout_setting(name text, value text) returns text
	language plpgsql immutable
as $own$
declare
	parts text[];
	part text;
	bare text;
	mapped text;
	out text[] := '{}';
begin
	if name = 'pgrst.db_anon_role' then
		return coalesce(pg_temp.own_layout_role(value), value);
	end if;
	if name not in ('search_path', 'pgrst.db_schemas', 'pgrst.db_extra_search_path') then
		return value;
	end if;
	parts := string_to_array(value, ',');
	foreach part in array parts loop
		bare := btrim(part);
		if bare like '"%"' then
			bare := replace(substr(bare, 2, length(bare) - 2), '""', '"');
		end if;
		mapped := case bare when 'auth' then 'snout_identity' when 'storage' then 'snout_files' when 'realtime' then 'snout_live' when 'graphql_public' then 'snout_graph' when 'extensions' then 'snout_ext' else null end;
		out := out || coalesce(mapped, btrim(part));
	end loop;
	return array_to_string(out, ', ');
end
$own$;

-- Is this function one of ours to rewrite? SQL and PL/pgSQL, not an extension's, not a catalog's,
-- not an aggregate, and not one of the two claim helpers whose bodies name old roles on purpose.
create function pg_temp.own_layout_rewritable(fn oid) returns boolean
	language sql stable
as $own$
	select exists (
		select
		from pg_proc p
		join pg_namespace n on n.oid = p.pronamespace
		join pg_language l on l.oid = p.prolang
		where p.oid = fn
			and l.lanname in ('sql', 'plpgsql')
			and p.prokind in ('f', 'p')
			and n.nspname not in ('pg_catalog', 'information_schema')
			and n.nspname !~ '^pg_(toast|temp)'
			and not exists (select from pg_depend d where d.classid = 'pg_proc'::regclass and d.objid = p.oid and d.deptype = 'e')
			and not (n.nspname = 'snout_identity' and p.proname in ('claims', 'access'))
	)
$own$;

-- Everything still in the vocabulary this run leaves. Empty is the only acceptable answer.
create function pg_temp.own_layout_scan() returns table (kind text, name text)
	language plpgsql stable
as $own$
declare
	people regclass;
begin
	return query select 'schema'::text, nspname::text from pg_namespace where nspname in ('auth', 'storage', 'realtime', 'graphql_public', 'extensions');
	return query select 'role'::text, left_.rolname::text from (select rolname from pg_roles where rolname = 'authenticator'
		union all select a.old from (values ('anon', 'snout_app'), ('authenticated', 'snout_member'), ('service_role', 'snout_server')) a(old, new)
			join pg_roles r on r.rolname = a.old
			where r.rolcanlogin or not exists (
				select from pg_auth_members m
				where m.member = r.oid and m.roleid = to_regrole(a.new) and m.inherit_option
			)) left_(rolname);
	return query
		select 'function'::text, p.oid::regprocedure::text
		from pg_proc p
		where pg_temp.own_layout_rewritable(p.oid)
			and pg_temp.own_layout_text(pg_get_functiondef(p.oid)) is distinct from pg_get_functiondef(p.oid);
	return query
		select 'function setting'::text, p.oid::regprocedure::text
		from pg_proc p, unnest(p.proconfig) cfg
		where pg_temp.own_layout_rewritable(p.oid)
			and cfg like 'search_path=%'
			and pg_temp.own_layout_setting('search_path', substr(cfg, 13)) <> substr(cfg, 13);
	return query
		select 'policy'::text, format('%I on %s', pol.polname, pol.polrelid::regclass)
		from pg_policy pol
		where pg_temp.own_layout_text(pg_get_expr(pol.polqual, pol.polrelid)) is distinct from pg_get_expr(pol.polqual, pol.polrelid)
			or pg_temp.own_layout_text(pg_get_expr(pol.polwithcheck, pol.polrelid)) is distinct from pg_get_expr(pol.polwithcheck, pol.polrelid);
	return query
		select case c.relkind when 'm' then 'materialized view' else 'view' end, c.oid::regclass::text
		from pg_class c
		join pg_namespace n on n.oid = c.relnamespace
		where c.relkind in ('v', 'm')
			and n.nspname not in ('pg_catalog', 'information_schema')
			and not exists (select from pg_depend d where d.classid = 'pg_class'::regclass and d.objid = c.oid and d.deptype = 'e')
			and pg_temp.own_layout_text(pg_get_viewdef(c.oid)) is distinct from pg_get_viewdef(c.oid);
	return query
		select 'setting'::text, format('%s for %s', split_part(cfg, '=', 1), case when s.setrole = 0 then 'the database' else 'role ' || s.setrole::regrole end)
		from pg_db_role_setting s, unnest(s.setconfig) cfg
		where s.setdatabase in (0, (select oid from pg_database where datname = current_database()))
			and pg_temp.own_layout_setting(split_part(cfg, '=', 1), substr(cfg, strpos(cfg, '=') + 1))
				<> substr(cfg, strpos(cfg, '=') + 1);
	if to_regclass('cron.job') is not null then
		return query execute $cron$
			select 'cron job'::text, jobid::text
			from cron.job
			where database = current_database()
				and (pg_temp.own_layout_text(command) is distinct from command or pg_temp.own_layout_role(username) is not null)
		$cron$;
	end if;
	-- Whichever name the people table has right now: the scan runs before a rename as well as after.
	-- Only one with both columns: a database whose people table is a stand-in without them (the
	-- control plane's sql/test stub) has no role or audience to carry an old name.
	people := coalesce(to_regclass('snout_identity.people'), to_regclass('auth.users'));
	if people is not null and (select count(*) from pg_attribute where attrelid = people and attname in ('role', 'aud') and not attisdropped) = 2 then
		return query execute format(
			'select %L::text, count(*)::text from %s where role in (''anon'', ''authenticated'', ''service_role'') or aud in (''anon'', ''authenticated'', ''service_role'') having count(*) > 0',
			'person role or aud', people
		);
	end if;
end
$own$;

-- 1. Refuse unless this is the old layout, untouched by any other session.
do $own$
begin
	if exists (select from pg_roles where rolname in ('snout_app', 'snout_member', 'snout_server', 'snout_gate'))
		or exists (select from pg_namespace where nspname in ('snout_identity', 'snout_files', 'snout_live', 'snout_graph', 'snout_ext')) then
		raise exception 'own-layout: this database already has a name of the new layout; nothing was changed';
	end if;
	if (select count(*) from pg_roles where rolname in ('anon', 'authenticated', 'service_role')) <> 3 then
		raise exception 'own-layout: this database does not have the old layout''s roles; nothing was changed';
	end if;
	-- A rename keeps a SCRAM verifier and clears an MD5 one, which would lock the data API out.
	if exists (select from pg_authid where rolname = 'authenticator' and rolpassword like 'md5%') then
		raise exception 'own-layout: authenticator has an MD5 password, which a rename clears; set it again as SCRAM first';
	end if;
	perform pg_temp.own_layout_locks();
end
$own$;

-- 1b. The old claim helpers' definitions as this database has them, for the inverse.
do $own$
begin
	if to_regnamespace('auth') is null then
		return;
	end if;
	drop table if exists auth.own_layout_helpers;
	create table auth.own_layout_helpers (signature text primary key, definition text not null);
	revoke all on auth.own_layout_helpers from public;
	insert into auth.own_layout_helpers
	select sig, pg_get_functiondef(to_regprocedure(sig))
	from unnest(array['auth.uid()', 'auth.jwt()', 'auth.role()', 'auth.email()']) sig
	where to_regprocedure(sig) is not null;
end
$own$;

-- 2. The roles. Grants, memberships, default privileges, policies' role lists and ownership refer
-- to a role by OID and follow it.
do $own$
begin
	if to_regrole('anon') is not null then
		execute format('alter role %I rename to %I', 'anon', 'snout_app');
	end if;
	if to_regrole('authenticated') is not null then
		execute format('alter role %I rename to %I', 'authenticated', 'snout_member');
	end if;
	if to_regrole('service_role') is not null then
		execute format('alter role %I rename to %I', 'service_role', 'snout_server');
	end if;
	if to_regrole('authenticator') is not null then
		execute format('alter role %I rename to %I', 'authenticator', 'snout_gate');
	end if;
end
$own$;

-- 3. The old names, as aliases of the new (section 3, item 3): a token still carrying an old role
-- claim is switched into the alias, which inherits everything the new role has. BYPASSRLS is an
-- attribute, so the server key's alias carries it itself. Role settings (the data API's statement
-- timeouts) are copied, because a role's settings are not inherited either.
do $own$
declare
	a record;
	s record;
begin
	for a in select * from (values ('anon', 'snout_app', false), ('authenticated', 'snout_member', false), ('service_role', 'snout_server', true)) v(old, new, bypass) loop
		if to_regrole(a.new) is null then
			continue;
		end if;
		execute format('create role %I nologin%s', a.old, case when a.bypass then ' bypassrls' else '' end);
		execute format('grant %I to %I with inherit true', a.new, a.old);
		if to_regrole('snout_gate') is not null then
			execute format('grant %I to %I', a.old, 'snout_gate');
		end if;
		for s in
			select st.setdatabase, cfg
			from pg_db_role_setting st, unnest(st.setconfig) cfg
			where st.setrole = to_regrole(a.new)
		loop
			execute format(
				'alter role %I%s set %s = %s',
				a.old,
				case when s.setdatabase = 0 then '' else format(' in database %I', (select datname from pg_database where oid = s.setdatabase)) end,
				split_part(s.cfg, '=', 1),
				quote_literal(substr(s.cfg, strpos(s.cfg, '=') + 1))
			);
		end loop;
	end loop;
end
$own$;

-- 4. Schemas, then the tables and functions in them.
do $own$
begin
	if to_regnamespace('auth') is not null then
		execute format('alter schema %I rename to %I', 'auth', 'snout_identity');
	end if;
	if to_regnamespace('storage') is not null then
		execute format('alter schema %I rename to %I', 'storage', 'snout_files');
	end if;
	if to_regnamespace('realtime') is not null then
		execute format('alter schema %I rename to %I', 'realtime', 'snout_live');
	end if;
	if to_regnamespace('graphql_public') is not null then
		execute format('alter schema %I rename to %I', 'graphql_public', 'snout_graph');
	end if;
	if to_regnamespace('extensions') is not null then
		execute format('alter schema %I rename to %I', 'extensions', 'snout_ext');
	end if;
end
$own$;

do $own$
declare
	fn record;
	sq record;
begin
	if to_regclass('snout_identity.users') is not null then
		execute format('alter table %I.%I rename to %I', 'snout_identity', 'users', 'people');
		-- The sequences it owns, named after it (a serial or identity column's), go with it.
		for sq in
			select s.oid::regclass as seq, s.relname
			from pg_class s
			join pg_depend d on d.classid = 'pg_class'::regclass and d.objid = s.oid and d.deptype in ('a', 'i')
			where s.relkind = 'S' and d.refobjid = to_regclass('snout_identity.people') and s.relname like 'users_%'
		loop
			execute format('alter sequence %s rename to %I', sq.seq, 'people' || substr(sq.relname, 6));
		end loop;
	end if;
	if to_regclass('snout_identity.identities') is not null then
		execute format('alter table %I.%I rename to %I', 'snout_identity', 'identities', 'provider_links');
		-- The sequences it owns, named after it (a serial or identity column's), go with it.
		for sq in
			select s.oid::regclass as seq, s.relname
			from pg_class s
			join pg_depend d on d.classid = 'pg_class'::regclass and d.objid = s.oid and d.deptype in ('a', 'i')
			where s.relkind = 'S' and d.refobjid = to_regclass('snout_identity.provider_links') and s.relname like 'identities_%'
		loop
			execute format('alter sequence %s rename to %I', sq.seq, 'provider_links' || substr(sq.relname, 11));
		end loop;
	end if;
	if to_regclass('snout_files.buckets') is not null then
		execute format('alter table %I.%I rename to %I', 'snout_files', 'buckets', 'spaces');
		-- The sequences it owns, named after it (a serial or identity column's), go with it.
		for sq in
			select s.oid::regclass as seq, s.relname
			from pg_class s
			join pg_depend d on d.classid = 'pg_class'::regclass and d.objid = s.oid and d.deptype in ('a', 'i')
			where s.relkind = 'S' and d.refobjid = to_regclass('snout_files.spaces') and s.relname like 'buckets_%'
		loop
			execute format('alter sequence %s rename to %I', sq.seq, 'spaces' || substr(sq.relname, 8));
		end loop;
	end if;
	if to_regclass('snout_files.objects') is not null then
		execute format('alter table %I.%I rename to %I', 'snout_files', 'objects', 'files');
		-- The sequences it owns, named after it (a serial or identity column's), go with it.
		for sq in
			select s.oid::regclass as seq, s.relname
			from pg_class s
			join pg_depend d on d.classid = 'pg_class'::regclass and d.objid = s.oid and d.deptype in ('a', 'i')
			where s.relkind = 'S' and d.refobjid = to_regclass('snout_files.files') and s.relname like 'objects_%'
		loop
			execute format('alter sequence %s rename to %I', sq.seq, 'files' || substr(sq.relname, 8));
		end loop;
	end if;
	if to_regclass('snout_live.messages') is not null then
		execute format('alter table %I.%I rename to %I', 'snout_live', 'messages', 'signals');
		-- The sequences it owns, named after it (a serial or identity column's), go with it.
		for sq in
			select s.oid::regclass as seq, s.relname
			from pg_class s
			join pg_depend d on d.classid = 'pg_class'::regclass and d.objid = s.oid and d.deptype in ('a', 'i')
			where s.relkind = 'S' and d.refobjid = to_regclass('snout_live.signals') and s.relname like 'messages_%'
		loop
			execute format('alter sequence %s rename to %I', sq.seq, 'signals' || substr(sq.relname, 9));
		end loop;
	end if;
	if to_regclass('snout_live.subscription') is not null then
		execute format('alter table %I.%I rename to %I', 'snout_live', 'subscription', 'listeners');
		-- The sequences it owns, named after it (a serial or identity column's), go with it.
		for sq in
			select s.oid::regclass as seq, s.relname
			from pg_class s
			join pg_depend d on d.classid = 'pg_class'::regclass and d.objid = s.oid and d.deptype in ('a', 'i')
			where s.relkind = 'S' and d.refobjid = to_regclass('snout_live.listeners') and s.relname like 'subscription_%'
		loop
			execute format('alter sequence %s rename to %I', sq.seq, 'listeners' || substr(sq.relname, 13));
		end loop;
	end if;
	for fn in select p.oid::regprocedure as sig from pg_proc p where p.pronamespace = to_regnamespace('snout_identity') and p.proname = 'uid' loop
		execute format('alter function %s rename to %I', fn.sig, 'user_id');
	end loop;
	for fn in select p.oid::regprocedure as sig from pg_proc p where p.pronamespace = to_regnamespace('snout_identity') and p.proname = 'jwt' loop
		execute format('alter function %s rename to %I', fn.sig, 'claims');
	end loop;
	for fn in select p.oid::regprocedure as sig from pg_proc p where p.pronamespace = to_regnamespace('snout_identity') and p.proname = 'role' loop
		execute format('alter function %s rename to %I', fn.sig, 'access');
	end loop;
	for fn in select p.oid::regprocedure as sig from pg_proc p where p.pronamespace = to_regnamespace('snout_identity') and p.proname = 'email' loop
		execute format('alter function %s rename to %I', fn.sig, 'user_email');
	end loop;
	for fn in select p.oid::regprocedure as sig from pg_proc p where p.pronamespace = to_regnamespace('snout_live') and p.proname = 'send' loop
		execute format('alter function %s rename to %I', fn.sig, 'signal');
	end loop;
end
$own$;

-- 5a. Functions' search_path settings, before their bodies: a function's definition prints them.
do $own$
declare
	p record;
	value text;
	mapped text;
begin
	for p in
		select fn.oid::regprocedure as sig, fn.prokind, cfg
		from pg_proc fn, unnest(fn.proconfig) cfg
		where pg_temp.own_layout_rewritable(fn.oid) and cfg like 'search_path=%'
	loop
		value := substr(p.cfg, 13);
		mapped := pg_temp.own_layout_setting('search_path', value);
		if mapped <> value then
			execute format('alter %s %s set search_path = %s', case when p.prokind = 'p' then 'procedure' else 'function' end, p.sig, mapped);
		end if;
	end loop;
end
$own$;

-- 5b. Function bodies: each definition read back, rewritten, and replaced when it changed. Bodies are
-- not checked here (check_function_bodies): a customer's function that was already broken stays
-- exactly as broken, rather than stopping the run.
do $own$
declare
	p record;
	def text;
	rewritten text;
begin
	perform set_config('check_function_bodies', 'off', true);
	for p in select fn.oid from pg_proc fn where pg_temp.own_layout_rewritable(fn.oid) loop
		def := pg_get_functiondef(p.oid);
		rewritten := pg_temp.own_layout_text(def);
		if rewritten is distinct from def then
			execute rewritten;
		end if;
	end loop;
end
$own$;

-- 5c. The claim helpers, as this layout writes them (helpers.ts).
do $own$
begin
	if to_regprocedure('snout_identity.user_id()') is null then
		return;
	end if;
	execute 'create or replace function snout_identity.claims() returns jsonb
	language sql stable
	set search_path = ''''
as $snoutpod$
	select c
		|| case c ->> ''role''
			when ''anon'' then ''{"role": "snout_app"}''
			when ''authenticated'' then ''{"role": "snout_member"}''
			when ''service_role'' then ''{"role": "snout_server"}''
			else ''{}''
		end::jsonb
		|| case when c ? ''app_metadata'' and not c ? ''server_data''
			then jsonb_build_object(''server_data'', c -> ''app_metadata'') else ''{}''::jsonb end
		|| case when c ? ''user_metadata'' and not c ? ''profile_data''
			then jsonb_build_object(''profile_data'', c -> ''user_metadata'') else ''{}''::jsonb end
	from (
		select coalesce(
			nullif(current_setting(''request.jwt.claim'', true), ''''),
			nullif(current_setting(''request.jwt.claims'', true), '''')
		)::jsonb as c
	) claims
$snoutpod$;

create or replace function snout_identity.user_id() returns uuid
	language sql stable
	set search_path = ''''
as $snoutpod$
	select coalesce(
		nullif(current_setting(''request.jwt.claim.sub'', true), ''''),
		(nullif(current_setting(''request.jwt.claims'', true), '''')::jsonb ->> ''sub'')
	)::uuid
$snoutpod$;

create or replace function snout_identity.access() returns text
	language sql stable
	set search_path = ''''
as $snoutpod$
	select case r
		when ''anon'' then ''snout_app''
		when ''authenticated'' then ''snout_member''
		when ''service_role'' then ''snout_server''
		else r
	end
	from (
		select coalesce(
			nullif(current_setting(''request.jwt.claim.role'', true), ''''),
			(nullif(current_setting(''request.jwt.claims'', true), '''')::jsonb ->> ''role'')
		)::text as r
	) claim
$snoutpod$;

create or replace function snout_identity.user_email() returns text
	language sql stable
	set search_path = ''''
as $snoutpod$
	select coalesce(
		nullif(current_setting(''request.jwt.claim.email'', true), ''''),
		(nullif(current_setting(''request.jwt.claims'', true), '''')::jsonb ->> ''email'')
	)::text
$snoutpod$;
';
end
$own$;

-- 5d. Policy expressions. Their role lists are OIDs and followed already; what is left is literals.
do $own$
declare
	p record;
	q text;
	c text;
begin
	for p in
		select pol.polname, pol.polrelid::regclass as rel,
			pg_get_expr(pol.polqual, pol.polrelid) as qual,
			pg_get_expr(pol.polwithcheck, pol.polrelid) as checks
		from pg_policy pol
	loop
		q := pg_temp.own_layout_text(p.qual);
		c := pg_temp.own_layout_text(p.checks);
		if q is distinct from p.qual or c is distinct from p.checks then
			execute format('alter policy %I on %s', p.polname, p.rel)
				|| case when q is null then '' else format(' using (%s)', q) end
				|| case when c is null then '' else format(' with check (%s)', c) end;
		end if;
	end loop;
end
$own$;

-- 5e. View definitions. A materialized view cannot be replaced in place; the scan names one.
do $own$
declare
	v record;
	def text;
	rewritten text;
begin
	for v in
		select c.oid, c.oid::regclass as name, c.reloptions
		from pg_class c
		join pg_namespace n on n.oid = c.relnamespace
		where c.relkind = 'v'
			and n.nspname not in ('pg_catalog', 'information_schema')
			and not exists (select from pg_depend d where d.classid = 'pg_class'::regclass and d.objid = c.oid and d.deptype = 'e')
	loop
		def := pg_get_viewdef(v.oid);
		rewritten := pg_temp.own_layout_text(def);
		if rewritten is distinct from def then
			execute format(
				'create or replace view %s%s as %s',
				v.name,
				case when v.reloptions is null then '' else format(' with (%s)', array_to_string(v.reloptions, ', ')) end,
				rewritten
			);
		end if;
	end loop;
end
$own$;

-- 5f. Database and role settings: the database's search_path, the auth server's, the data API's
-- in-database configuration if it has any.
do $own$
declare
	s record;
	name text;
	value text;
	mapped text;
	target text;
begin
	for s in
		select st.setdatabase, st.setrole, cfg
		from pg_db_role_setting st, unnest(st.setconfig) cfg
		where st.setdatabase in (0, (select oid from pg_database where datname = current_database()))
	loop
		name := split_part(s.cfg, '=', 1);
		value := substr(s.cfg, strpos(s.cfg, '=') + 1);
		mapped := pg_temp.own_layout_setting(name, value);
		if mapped = value then
			continue;
		end if;
		if s.setrole = 0 then
			target := format('database %I', (select datname from pg_database where oid = s.setdatabase));
		elsif s.setdatabase = 0 then
			target := format('role %I', s.setrole::regrole);
		else
			target := format('role %I in database %I', s.setrole::regrole, (select datname from pg_database where oid = s.setdatabase));
		end if;
		execute format(
			'alter %s set %s = %s',
			target,
			name,
			case when name = 'pgrst.db_anon_role' then quote_literal(mapped) else mapped end
		);
	end loop;
end
$own$;

-- 5g. pg_cron's jobs: the command and the role a job runs as are both text.
do $own$
begin
	if to_regclass('cron.job') is null then
		return;
	end if;
	execute $cron$
		update cron.job
		set command = pg_temp.own_layout_text(command),
			username = coalesce(pg_temp.own_layout_role(username), username)
		where database = current_database()
			and (pg_temp.own_layout_text(command) is distinct from command or pg_temp.own_layout_role(username) is not null)
	$cron$;
end
$own$;

-- 5g'. snout-realtime's own ledger: it keeps each file's hash and re-runs a file whose hash it has not
-- recorded, in the layout the database now has. Forgetting the rows makes it write its functions
-- again in this layout on its next connect, rather than keep the bodies this run rewrote.
do $own$
begin
	if to_regclass('snout_realtime.migrations') is not null then
		delete from snout_realtime.migrations;
	end if;
end
$own$;

-- 5h. Every person's role and audience, which the auth server copies into the tokens it mints.
-- Triggers are held for these two columns' own update (session_replication_role), so a customer's
-- "on update of a person" trigger does not take a rename for an edit.
do $own$
begin
	if to_regclass('snout_identity.people') is null
		or (select count(*) from pg_attribute where attrelid = to_regclass('snout_identity.people') and attname in ('role', 'aud') and not attisdropped) <> 2 then
		return;
	end if;
	perform set_config('session_replication_role', 'replica', true);
	execute format(
		'update %s set role = coalesce(pg_temp.own_layout_role(role), role), aud = coalesce(pg_temp.own_layout_role(aud), aud) where role in (''anon'', ''authenticated'', ''service_role'') or aud in (''anon'', ''authenticated'', ''service_role'')',
		'snout_identity.people'
	);
	perform set_config('session_replication_role', 'origin', true);
end
$own$;

-- 7. The scan. Anything it finds rolls the whole run back.
do $own$
declare
	found text;
begin
	select string_agg(format('%s %s', kind, name), '; ') into found from pg_temp.own_layout_scan();
	if found is not null then
		raise exception 'own-layout: still in the old vocabulary after the run, so nothing was changed: %', found;
	end if;
end
$own$;

drop function pg_temp.own_layout_scan();
drop function pg_temp.own_layout_rewritable(oid);
drop function pg_temp.own_layout_setting(text, text);
drop function pg_temp.own_layout_role(text);
drop function pg_temp.own_layout_text(text);
drop function pg_temp.own_layout_locks();

-- The aliases, to every role that switches into the new roles (rename.ts, aliasMembersSql).
do $own$
declare
	a record;
	m record;
begin
	for a in select * from (values ('anon', 'snout_app'), ('authenticated', 'snout_member'), ('service_role', 'snout_server')) v(old, new) loop
		if to_regrole(a.old) is null or to_regrole(a.new) is null then
			continue;
		end if;
		for m in
			select am.member::regrole::text as member, am.inherit_option
			from pg_auth_members am
			where am.roleid = to_regrole(a.new)
				and am.set_option
				and am.member <> to_regrole(a.old)
				and not exists (
					select 1 from pg_auth_members x where x.roleid = to_regrole(a.old) and x.member = am.member
				)
		loop
			execute format('grant %I to %s with inherit %s', a.old, m.member, case when m.inherit_option then 'true' else 'false' end);
		end loop;
	end loop;
end
$own$;
