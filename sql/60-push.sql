do $push$
begin
	if not exists (select 1 from pg_roles where rolname = 'snout_push_admin') then
		create role snout_push_admin;
	end if;
end
$push$;
alter role snout_push_admin with login noinherit;
grant snout_app, snout_member, snout_server to snout_push_admin;
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
	if exists (select 1 from pg_namespace where nspname = 'snout_identity') then
		grant usage on schema snout_identity to snout_push_admin;
		if to_regclass('snout_identity.people') is not null then
			grant references on snout_identity.people to snout_push_admin;
		end if;
		-- The auth server's role, under its name since 2026-09-30 or the one before it on a pod
		-- the host agent has not renamed yet.
		for auth_role in select rolname from pg_roles where rolname in ('snout_auth_admin') loop
			if pg_has_role(auth_role, 'usage') then
				execute format('alter default privileges for role %I in schema snout_identity grant references on tables to snout_push_admin', auth_role);
			end if;
		end loop;
	end if;
exception when insufficient_privilege then
	raise notice 'push cannot be linked to snout_identity.people: %', sqlerrm;
end
$push$;