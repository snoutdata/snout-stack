do $push$
begin
	if not exists (select 1 from pg_roles where rolname = 'snout_push_admin') then
		create role snout_push_admin;
	end if;
end
$push$;
alter role snout_push_admin with login noinherit;
grant anon, authenticated, service_role to snout_push_admin;
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
begin
	if exists (select 1 from pg_namespace where nspname = 'auth') then
		grant usage on schema auth to snout_push_admin;
		if to_regclass('auth.users') is not null then
			grant references on auth.users to snout_push_admin;
		end if;
		-- The auth server's role, under its name since 2026-09-30 or the one before it on a pod
		-- the host agent has not renamed yet.
		for auth_role in select rolname from pg_roles where rolname in ('snout_auth_admin') loop
			if pg_has_role(auth_role, 'usage') then
				execute format('alter default privileges for role %I in schema auth grant references on tables to snout_push_admin', auth_role);
			end if;
		end loop;
	end if;
exception when insufficient_privilege then
	raise notice 'push cannot be linked to auth.users: %', sqlerrm;
end
$push$;