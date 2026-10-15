do $push$
begin
	if not exists (select 1 from pg_roles where rolname = 'snout_push_admin') then
		create role snout_push_admin;
	end if;
end
$push$;
alter role snout_push_admin with login noinherit;
-- The own layout's API roles (the own layout), or the old names in a database not yet renamed.
do $push$
begin
	if to_regrole('snout_member') is not null then
		grant snout_app, snout_member, snout_server to snout_push_admin;
		-- And the aliases: push runs a token's role claim as written, and a key minted with the
		-- old claims names one of them.
		if to_regrole('service_role') is not null then
			grant anon, authenticated, service_role to snout_push_admin;
		end if;
	else
		grant anon, authenticated, service_role to snout_push_admin;
	end if;
end
$push$;
do $push$
begin
	execute format('grant connect on database %I to snout_push_admin', current_database());
end
$push$;
grant snout_push_admin to current_user with set true, inherit false;
create schema if not exists push authorization snout_push_admin;
do $push$
declare
	auth_role name;
	-- snout_identity and its people in the own layout, auth and its users before it.
	home name := (select nspname from pg_namespace where nspname in ('snout_identity', 'auth') order by nspname = 'snout_identity' desc limit 1);
	people regclass := coalesce(to_regclass('snout_identity.people'), to_regclass('auth.users'));
begin
	if home is not null then
		execute format('grant usage on schema %I to snout_push_admin', home);
		if people is not null then
			execute format('grant references on %s to snout_push_admin', people);
		end if;
		-- The auth server's role, under its name since 2026-09-30 or the one before it on a pod
		-- the host agent has not renamed yet.
		for auth_role in select rolname from pg_roles where rolname in ('snout_auth_admin') loop
			if pg_has_role(auth_role, 'usage') then
				execute format('alter default privileges for role %I in schema %I grant references on tables to snout_push_admin', auth_role, home);
			end if;
		end loop;
	end if;
exception when insufficient_privilege then
	raise notice 'push cannot be linked to the people table: %', sqlerrm;
end
$push$;